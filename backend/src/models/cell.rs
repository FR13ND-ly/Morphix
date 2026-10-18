use serde::{Deserialize, Serialize};

pub type CellType = i64;

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    pub cell_type: CellType,
    pub value: i64
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellTypeDefinition {
    pub id: CellType,
    pub name: String,
    pub min_value: i64,
    pub max_value: i64,
}