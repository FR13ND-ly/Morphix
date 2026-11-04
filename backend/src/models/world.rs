use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{agent::Agent, state::ObjectState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    pub cells: Vec<Vec<ObjectState>>,
    pub agents: Vec<Agent>,
}

impl World {
    pub fn height(&self) -> usize {
        self.cells.len()
    }

    pub fn width(&self) -> usize {
        self.cells.first().map_or(0, |row| row.len())
    }

    pub fn validate(&self) -> Result<(), String> {
        let width = self.width();

        if self.cells.is_empty() || width == 0 {
            return Err("World grid must not be empty".to_owned());
        }

        if self.cells.iter().any(|row| row.len() != width) {
            return Err("World grid must be rectangular".to_owned());
        }

        let mut agent_ids = HashSet::new();

        for agent in &self.agents {
            if agent.position.row >= self.height() || agent.position.col >= width {
                return Err(format!("Agent {} is outside the world", agent.id));
            }

            if !agent_ids.insert(agent.id) {
                return Err(format!("Duplicate agent id {}", agent.id));
            }
        }

        Ok(())
    }
}
