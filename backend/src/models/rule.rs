use serde::{Deserialize, Serialize};

use super::{cell::CellType, expression::Expression};

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    SetType {
        cell_type: CellType,
    },
    SetValue {
        expression: Expression,
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub probability: f64,
    pub actions: Vec<Action>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub cell_type: CellType,
    pub when: Expression,
    pub outcomes: Vec<Outcome>,
}