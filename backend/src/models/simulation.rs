use serde::{Deserialize, Serialize};

use super::{automaton::AutomatonDefinition, grid::Grid};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationRequest {
    pub automaton: AutomatonDefinition,
    pub grid: Grid,
    pub generations: u32,
    pub seed: u64
}

#[derive(Serialize)]
pub struct SimulationResult {
    pub grid: Grid,
    pub generations: u32,
}