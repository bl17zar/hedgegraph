use tracing::info;

use crate::{netstrat::bounds::BoundsSet, sources::binance::Interval};

use super::{loading_state::LoadingState, props::Props};

#[derive(Default, Debug, Clone)]
pub struct State {
    pub loading: LoadingState,
    pub props: Props,
    bounds: BoundsSet,
}

impl State {
    pub fn apply_props(&mut self, props: &Props) {
        info!("Applying props: {props:?}.");

        let subtract_res = props.bounds.subtract(&self.bounds);
        if subtract_res.is_none() {
            info!("Found nothing to load.");
            return;
        }
        let to_load = subtract_res.unwrap();
        info!("Computed difference to load: {to_load:?}.");

        let loading_res = LoadingState::new(&to_load, State::step(props.interval), props.limit);
        if loading_res.is_none() {
            info!("Could not initialize loading state.");
            return;
        }
        let loading = loading_res.unwrap();
        info!("Initialized loading state: {loading:?}.");

        let new_bounds = self.bounds.merge(&props.bounds);
        info!("Computed new_bounds: {new_bounds:?}");

        self.loading = loading;
        self.bounds = new_bounds;
        self.props = props.clone();
    }

    fn step(i: Interval) -> usize {
        match i {
            Interval::Minute => 60,
            Interval::Hour => 60 * 60,
            Interval::Day => 60 * 60 * 24,
        }
    }
}
