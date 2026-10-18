use serde::{Deserialize, Serialize};

use super::cell::Cell;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grid {
    pub cells: Vec<Vec<Cell>>,
}