use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{
    cell::{Cell, CellTypeDefinition},
    grid::Grid,
    rule::{Action, Rule},
};

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Boundary {
    Fixed { cell: Cell },
    Wrap,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomatonDefinition {
    pub name: String,
    pub cell_types: Vec<CellTypeDefinition>,
    pub neighborhood: Vec<(i32, i32)>,
    pub boundary: Boundary,
    pub rules: Vec<Rule>,
}