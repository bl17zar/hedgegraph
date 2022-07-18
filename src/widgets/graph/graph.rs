use std::fs::File;

use crossbeam::channel::{unbounded, Receiver, Sender};

use egui::{
    plot::LinkedAxisGroup, CentralPanel, ProgressBar, Response, TopBottomPanel, Ui, Widget,
};
use egui_extras::{Size, StripBuilder};
use poll_promise::Promise;
use tracing::{debug, error, info};

use crate::{
    sources::binance::{Client, Kline},
    windows::{AppWindow, TimeRangeChooser},
};

use super::{
    candles::Candles, data::Data, loading_state::LoadingState, props::Props, volume::Volume,
};

#[derive(Default)]
struct ExportState {
    triggered: bool,
}

pub struct Graph {
    candles: Candles,
    volume: Volume,
    symbol: String,
    symbol_pub: Sender<String>,

    pub time_range_window: Box<dyn AppWindow>,

    klines: Vec<Kline>,
    graph_loading_state: LoadingState,
    export_state: ExportState,
    klines_promise: Option<Promise<Vec<Kline>>>,
    symbol_sub: Receiver<String>,
    props_sub: Receiver<Props>,
    export_sub: Receiver<Props>,
}

impl Default for Graph {
    fn default() -> Self {
        let (s_symbols, r_symbols) = unbounded();
        let (s_props, r_props) = unbounded();
        let (s_export, r_export) = unbounded();

        Self {
            symbol_pub: s_symbols,
            time_range_window: Box::new(TimeRangeChooser::new(
                false,
                r_symbols.clone(),
                s_props,
                s_export,
            )),

            symbol_sub: r_symbols,
            props_sub: r_props,
            export_sub: r_export,

            symbol: Default::default(),
            candles: Default::default(),
            volume: Default::default(),

            klines: Default::default(),
            graph_loading_state: Default::default(),
            klines_promise: Default::default(),
            export_state: Default::default(),
        }
    }
}

impl Graph {
    pub fn new(symbol_chan: Receiver<String>) -> Self {
        let (s_symbols, r_symbols) = unbounded();
        let (s_props, r_props) = unbounded();
        let (s_export, r_export) = unbounded();

        Self {
            symbol_sub: symbol_chan,
            symbol_pub: s_symbols,
            props_sub: r_props,
            export_sub: r_export,
            time_range_window: Box::new(TimeRangeChooser::new(false, r_symbols, s_props, s_export)),
            ..Default::default()
        }
    }

    fn start_download(&mut self, props: Props) {
        info!("starting data download...");

        self.klines = vec![];

        self.graph_loading_state = LoadingState::from_graph_props(&props);
        self.graph_loading_state.triggered = true;

        let start_time = props.start_time().timestamp_millis().clone();
        let pair = self.symbol.to_string();
        let interval = props.interval.clone();
        let mut limit = props.limit.clone();

        if self.graph_loading_state.is_last_page() {
            limit = self.graph_loading_state.last_page_limit
        }

        debug!("setting left edge to: {start_time}");

        self.klines_promise = Some(Promise::spawn_async(async move {
            Client::kline(pair, interval, start_time, limit).await
        }));
    }
}

impl Widget for &mut Graph {
    fn ui(self, ui: &mut Ui) -> Response {
        let export_wrapped = self
            .export_sub
            .recv_timeout(std::time::Duration::from_millis(1));

        match export_wrapped {
            Ok(props) => {
                info!("got props for export: {props:?}");

                self.export_state.triggered = true;
                
                self.start_download(props);
            }
            Err(_) => {}
        }

        let symbol_wrapped = self
            .symbol_sub
            .recv_timeout(std::time::Duration::from_millis(1));

        match symbol_wrapped {
            Ok(symbol) => {
                info!("got symbol: {symbol}");

                self.klines = vec![];

                self.symbol = symbol.clone();

                self.symbol_pub.send(symbol).unwrap();

                self.graph_loading_state = LoadingState::from_graph_props(&Props::default());
                self.graph_loading_state.triggered = true;
                let interval = self.graph_loading_state.props.interval.clone();
                let start = self
                    .graph_loading_state
                    .left_edge()
                    .timestamp_millis()
                    .clone();
                let mut limit = self.graph_loading_state.props.limit.clone();
                if self.graph_loading_state.is_last_page() {
                    limit = self.graph_loading_state.last_page_limit;
                }

                let symbol = self.symbol.clone();
                self.klines_promise = Some(Promise::spawn_async(async move {
                    Client::kline(symbol, interval, start, limit).await
                }));
            }
            Err(_) => {}
        }

        if self.symbol == "" {
            return ui.label("select a symbol");
        }

        let props_wrapped = self
            .props_sub
            .recv_timeout(std::time::Duration::from_millis(1));

        match props_wrapped {
            Ok(props) => {
                info!("got props: {props:?}");

                self.start_download(props);
            }
            Err(_) => {}
        }

        if let Some(promise) = &self.klines_promise {
            if let Some(result) = promise.ready() {
                self.graph_loading_state.inc_received();

                if self.graph_loading_state.received > 0 {
                    result.iter().for_each(|k| {
                        self.klines.push(*k);
                    });
                }

                self.klines_promise = None;

                match self.graph_loading_state.is_finished() {
                    false => {
                        let start = self
                            .graph_loading_state
                            .left_edge()
                            .timestamp_millis()
                            .clone();

                        let symbol = self.symbol.to_string();
                        let interval = self.graph_loading_state.props.interval.clone();
                        let mut limit = self.graph_loading_state.props.limit.clone();
                        if self.graph_loading_state.is_last_page() {
                            limit = self.graph_loading_state.last_page_limit
                        }

                        self.klines_promise = Some(Promise::spawn_async(async move {
                            Client::kline(symbol, interval, start, limit).await
                        }));
                    }
                    true => {
                        let data = Data::new(self.klines.clone());
                        let axes_group = LinkedAxisGroup::new(true, false);
                        self.volume = Volume::new(data.clone(), axes_group.clone());
                        self.candles = Candles::new(data, axes_group);

                        if self.export_state.triggered {
                            info!("exporting to data...");

                            let name = format!(
                                "{}-{}-{}-{:?}",
                                self.symbol,
                                self.graph_loading_state.props.start_time(),
                                self.graph_loading_state.props.end_time(),
                                self.graph_loading_state.props.interval,
                            );
                            let f = File::create(format!("{}.csv", name)).unwrap();

                            let mut wtr = csv::Writer::from_writer(f);
                            self.klines.iter().for_each(|el| {
                                wtr.serialize(el).unwrap();
                            });
                            wtr.flush().unwrap();

                            self.export_state.triggered = false;

                            info!("exported to data: {}.csv", name);
                        }
                    }
                }
            }
        }

        if !self.graph_loading_state.is_finished() {
            return ui
                .centered_and_justified(|ui| {
                    ui.add(
                        ProgressBar::new(self.graph_loading_state.progress())
                            .show_percentage()
                            .animate(true),
                    )
                })
                .response;
        }

        TopBottomPanel::top("graph toolbar")
            .show_inside(ui, |ui| self.time_range_window.toggle_btn(ui));

        CentralPanel::default()
            .show_inside(ui, |ui| {
                self.time_range_window.show(ui);

                StripBuilder::new(ui)
                    .size(Size::relative(0.8))
                    .size(Size::remainder())
                    .vertical(|mut strip| {
                        strip.cell(|ui| {
                            ui.add(&self.candles);
                        });
                        strip.cell(|ui| {
                            ui.add(&self.volume);
                        });
                    })
            })
            .response
    }
}
