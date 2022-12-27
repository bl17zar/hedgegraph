use std::{collections::HashSet, string};

use chrono::{DateTime, Utc};
use crossbeam::channel::{unbounded, Sender};
use egui::{ScrollArea, Ui};
use graphviz_rust::dot_structures::{Id, NodeId};
use petgraph::{
    algo::all_simple_paths,
    dot::Dot,
    stable_graph::{NodeIndex, StableDiGraph},
    visit::{EdgeRef, IntoNodeReferences},
    Direction::{Incoming, Outgoing},
};
use tracing::error;

use crate::{
    netstrat::{channels, Bus, Message},
    widgets::AppWidget,
};

use super::{step_difference::StepDifference, Step};

#[derive(Clone, Default)]
pub struct History {
    tree: StableDiGraph<Step, usize>,
    bus: Bus,
    current_step: usize,
    max_gen: usize,
    root: NodeIndex,
}

impl History {
    pub fn new(first_step_name: String, bus: Bus) -> History {
        let mut tree = StableDiGraph::default();
        let root = tree.add_node(Step {
            name: first_step_name,
            parent_difference: Default::default(),
        });

        History {
            root,
            bus,
            tree,
            max_gen: 0,
            current_step: root.index(),
        }
    }

    pub fn add_step(&mut self, step_name: String, difference: StepDifference) {
        let new_node_idx = self.tree.add_node(Step {
            name: step_name,
            parent_difference: Some(difference),
        });

        match self.is_leaf(self.current_step) {
            true => self.push(new_node_idx),
            false => self.branch(new_node_idx),
        };

        self.current_step = new_node_idx.index();
    }

    /// gets generation for provided node checking generation of the incoming edge
    fn get_generation(&self, idx: usize) -> usize {
        if idx == self.root.index() {
            return 0;
        }

        *self
            .tree
            .edges_directed(NodeIndex::from(idx as u32), Incoming)
            .next()
            .unwrap()
            .weight()
    }

    /// pushes node to current history branch
    fn push(&mut self, new_node_idx: NodeIndex) -> Option<()> {
        self.tree.add_edge(
            NodeIndex::from(self.current_step as u32),
            new_node_idx,
            self.get_generation(self.current_step),
        );

        Some(())
    }

    /// creates new history branch and pushes node to it
    fn branch(&mut self, new_node_idx: NodeIndex) -> Option<()> {
        let next_generation = self.max_gen + 1;

        self.tree.add_edge(
            NodeIndex::from(self.current_step as u32),
            new_node_idx,
            next_generation,
        );

        self.max_gen = next_generation;

        Some(())
    }

    /// shows if current step is a leaf in the tree
    pub fn is_leaf(&self, idx: usize) -> bool {
        self.tree
            .edges_directed(NodeIndex::from(idx as u32), Outgoing)
            .count()
            == 0
    }

    // pub fn dot(&self) -> String {
    //     // TODO: use the same method to color graph dot.
    //     Dot::with_attr_getters(&self.tree, &[], &|g, r| String::new(), &|g, r| {
    //         if r.0.index() == self.current_step {
    //             return "color=red".to_string();
    //         }

    //         String::new()
    //     })
    //     .to_string()
    // }

    fn draw_history_recursive(
        &self,
        node: NodeIndex,
        prev_generation: usize,
        ui: &mut Ui,
    ) -> Option<usize> {
        let node_weight = self.tree.node_weight(node).unwrap();
        let step_name = node_weight.name.clone();
        let generation = self.get_generation(node.index());
        let children_edges = self.tree.edges_directed(node, Outgoing).collect::<Vec<_>>();
        let mut selected_step = None;
        let mut children_selected_steps = vec![];
        let step_button = |ui: &mut Ui| {
            ui.horizontal(|ui| {
                let mut btn = ui.selectable_label(node.index() == self.current_step, step_name);
                if let Some(diff) = node_weight.clone().parent_difference {
                    let mut tooltip_vec = vec![];
                    if let Some(elements_diff) = diff.elements {
                        tooltip_vec.push(format!("elements {}", elements_diff))
                    };
                    if let Some(colored_diff) = diff.colored {
                        tooltip_vec.push(format!("color {}", colored_diff))
                    };
                    if let Some(signal_diff) = diff.signal_holders {
                        tooltip_vec.push(format!("signal {}", signal_diff))
                    };

                    if !tooltip_vec.is_empty() {
                        btn = btn.on_hover_text(tooltip_vec.join("\n"));
                    }
                };

                if btn.clicked() {
                    selected_step = Some(node.index())
                };
            });
        };

        if generation == prev_generation {
            step_button(ui);
            children_edges.iter().for_each(|ce| {
                children_selected_steps.push(self.draw_history_recursive(
                    ce.target(),
                    generation,
                    ui,
                ));
            });
        } else {
            ui.collapsing(format!("split {}", generation), |ui| {
                step_button(ui);
                children_edges.iter().for_each(|ce| {
                    children_selected_steps.push(self.draw_history_recursive(
                        ce.target(),
                        generation,
                        ui,
                    ))
                });
            });
        };

        if selected_step.is_some() {
            return selected_step;
        }

        for s in children_selected_steps {
            if s.is_some() {
                return s;
            }
        }

        selected_step
    }

    fn update(&mut self, new_current_step: Option<usize>) {
        if let Some(step) = new_current_step {
            self.send_diff(self.compute_diff(step));
            self.current_step = step;
        }
    }

    fn compute_diff(&self, step: usize) -> StepDifference {
        let to = NodeIndex::from(step as u32);
        let rollback_point = lca(
            &self.tree,
            self.root,
            NodeIndex::from(self.current_step as u32),
            to,
        )
        .unwrap();

        // walk back to rollback point collecting diffs
        let mut backward_steps = vec![];
        let mut curr_step = NodeIndex::from(self.current_step as u32);
        while curr_step != rollback_point {
            self.tree
                .neighbors_directed(curr_step, Incoming)
                .for_each(|n| {
                    if let Some(diff) = self
                        .tree
                        .node_weight(curr_step)
                        .unwrap()
                        .parent_difference
                        .clone()
                    {
                        backward_steps.push(diff);
                    };

                    curr_step = n
                })
        }

        // squash backward steps
        let backward_diff = backward_steps
            .iter()
            .fold(StepDifference::default(), |accum, diff| {
                accum.squash(&diff.reverse())
            });

        // walk forward to selected step sending difference insructions
        let mut forward_steps = vec![];
        if let Some(path) =
            all_simple_paths::<Vec<_>, _>(&self.tree, rollback_point, to, 0, None).next()
        {
            path.iter().for_each(|n| {
                if let Some(diff) = self.tree.node_weight(*n).unwrap().parent_difference.clone() {
                    forward_steps.push(diff);
                };
            });
        }

        // squash forward steps
        forward_steps
            .iter_mut()
            .fold(backward_diff, |accum, diff| accum.squash(&diff))
    }

    fn send_diff(&mut self, diff: StepDifference) {
        let payload = serde_json::to_string(&diff).unwrap();
        if let Err(err) = self.bus.write(
            channels::HISTORY_DIFFERENCE.to_string(),
            Message::new(payload),
        ) {
            error!("failed to publish message: {err}");
        }
    }
}

impl AppWidget for History {
    fn show(&mut self, ui: &mut Ui) {
        let mut selected_step = None;

        ui.collapsing("History", |ui| {
            ScrollArea::both()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    selected_step = self.draw_history_recursive(
                        self.root,
                        self.get_generation(self.root.index()),
                        ui,
                    );
                });
        });

        self.update(selected_step)
    }
}

/// Finds the lowest common ancestor (LCA) of nodes `a` and `b` in the directed graph `g`.
///
/// This function first stores all of the ancestors of `a` in a set by traversing the graph starting
/// from `a` and following incoming edges until reaching the root node. It then traverses the ancestors
/// of `b` and returns the first ancestor that is also present in the set of `a`'s ancestors. If no
/// common ancestor is found, the function returns `None`.
///
/// # Arguments
///
/// * `g` - The directed graph to search for the LCA in.
/// * `root` - The index of the root node in the graph.
/// * `a` - The index of the first node to find the LCA for.
/// * `b` - The index of the second node to find the LCA for.
///
/// # Returns
///
/// The index of the LCA of `a` and `b`, or `None` if no common ancestor is found.
fn lca(
    g: &StableDiGraph<Step, usize>,
    root: NodeIndex,
    a: NodeIndex,
    b: NodeIndex,
) -> Option<NodeIndex> {
    let mut parents = HashSet::new();

    // walk through all parents till root and store them in map
    let mut curr_node = a;
    loop {
        parents.insert(curr_node);
        if curr_node == root {
            break;
        }

        g.neighbors_directed(curr_node, Incoming).for_each(|n| {
            parents.insert(n);
            curr_node = n;
        });
    }

    // walk through all parents and check for existence in the map;
    // first that exist is lca
    let mut result: Option<NodeIndex> = Default::default();
    let mut curr_node = b;
    while result.is_none() {
        if parents.contains(&curr_node) {
            result = Some(curr_node);
            break;
        };

        g.neighbors_directed(curr_node, Incoming).for_each(|n| {
            curr_node = n;
        })
    }

    result
}
mod test {
    use super::*;

    #[test]
    fn test_lca() {
        // create a new tree with the following structure:
        //
        //          1
        //        /   \
        //       2     3
        //      / \   / \
        //     4   5 6   7
        //
        let mut g = StableDiGraph::new();
        let root = g.add_node(Step {
            name: "1".to_string(),
            parent_difference: Default::default(),
        });
        let node2 = g.add_node(Step {
            name: "2".to_string(),
            parent_difference: Default::default(),
        });
        let node3 = g.add_node(Step {
            name: "3".to_string(),
            parent_difference: Default::default(),
        });
        let node4 = g.add_node(Step {
            name: "4".to_string(),
            parent_difference: Default::default(),
        });
        let node5 = g.add_node(Step {
            name: "5".to_string(),
            parent_difference: Default::default(),
        });
        let node6 = g.add_node(Step {
            name: "6".to_string(),
            parent_difference: Default::default(),
        });
        let node7 = g.add_node(Step {
            name: "7".to_string(),
            parent_difference: Default::default(),
        });
        g.add_edge(root, node2, 0);
        g.add_edge(root, node3, 0);
        g.add_edge(node2, node4, 0);
        g.add_edge(node2, node5, 0);
        g.add_edge(node3, node6, 0);
        g.add_edge(node3, node7, 0);

        // test LCA for nodes within the same branch
        assert_eq!(lca(&g, root, node4, node5), Some(node2));
        assert_eq!(lca(&g, root, node6, node7), Some(node3));

        // test LCA for nodes in different branches
        assert_eq!(lca(&g, root, node4, node6), Some(root));
        assert_eq!(lca(&g, root, node5, node7), Some(root));

        // test LCA for nodes where one of them is lca
        assert_eq!(lca(&g, root, node4, node2), Some(node2));
        assert_eq!(lca(&g, root, node5, node5), Some(node5));

        // test LCA for nodes where one of them is root
        assert_eq!(lca(&g, root, node4, root), Some(root));
    }
}
