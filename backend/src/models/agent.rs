use serde::{Deserialize, Serialize};

use super::{position::Position, state::ObjectState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub id: u64,
    pub position: Position,
    pub state: ObjectState,
}
