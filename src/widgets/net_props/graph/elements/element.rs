use std::fmt::Display;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct Node {
    id: Uuid,
    name: String,
    deleted: bool,
    selected: bool,
}

impl Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

impl Node {
    fn build(id: Uuid, name: String) -> Node {
        Node {
            id,
            name,
            deleted: false,
            selected: false,
        }
    }

    pub fn new(name: String) -> Node {
        Node::build(Uuid::new_v4(), name)
    }

    pub fn new_with_id(id: Uuid, name: String) -> Node {
        Node::build(id, name)
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn set_name(&mut self, new_name: String) {
        self.name = new_name
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn deleted(&self) -> bool {
        self.deleted
    }

    pub fn delete(&mut self) {
        self.deleted = true
    }

    pub fn restore(&mut self) {
        self.deleted = false
    }

    pub fn select(&mut self) {
        self.selected = true
    }

    pub fn deselect(&mut self) {
        self.selected = false
    }

    pub fn selected(&self) -> bool {
        self.selected
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct Edge {
    id: Uuid,
    weight_x_10_6: i32,
    start: Uuid,
    end: Uuid,
    name: String,
    deleted: bool,
    selected: bool,
}

impl Display for Edge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

impl Edge {
    fn build(id: Uuid, start: &Node, end: &Node, weight: f64) -> Edge {
        let million = 1_000_000_f64;
        let weight_x_10_6 = (weight * million) as i32;

        // otherwise hack with multiplying by million does not work
        assert!(weight < million);

        let name = [start.name().clone(), end.name().clone()].join(" -> ");
        Edge {
            weight_x_10_6,
            name,
            id,
            start: *start.id(),
            end: *end.id(),
            deleted: false,
            selected: false,
        }
    }

    pub fn new(start: &Node, end: &Node, weight: f64) -> Edge {
        Edge::build(Uuid::new_v4(), start, end, weight)
    }

    pub fn new_with_id(id: Uuid, start: &Node, end: &Node, weight: f64) -> Edge {
        Edge::build(id, start, end, weight)
    }

    pub fn start(&self) -> &Uuid {
        &self.start
    }

    pub fn end(&self) -> &Uuid {
        &self.end
    }

    pub fn weight(&self) -> f64 {
        self.weight_x_10_6 as f64 / 1_000_000_f64
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn delete(&mut self) {
        self.deleted = true
    }

    pub fn selected(&self) -> bool {
        self.selected
    }

    pub fn select(&mut self) {
        self.selected = true
    }

    pub fn deselect(&mut self) {
        self.selected = false
    }

    pub fn restore(&mut self) {
        self.deleted = false
    }

    pub fn deleted(&self) -> bool {
        self.deleted
    }
}

mod test {
    use super::*;

    #[test]
    fn test_edge_weight() {
        let w = 1.234567;
        let n1 = &Node::new("n1".to_string());
        let n2 = &Node::new("n2".to_string());

        assert_eq!(w, Edge::new(n1, n2, w).weight())
    }
}
