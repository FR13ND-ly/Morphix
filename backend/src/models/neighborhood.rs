use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{
    agent::Agent, automaton::Boundary, position::Position, state::ObjectState, world::World,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Neighborhood {
    pub offsets: Vec<(i32, i32)>,
}

#[derive(Debug, Clone, Default)]
pub struct Neighbors<'a> {
    pub offsets: Vec<(i32, i32)>,
    pub free_offsets: Vec<(i32, i32)>,
    pub cells: Vec<&'a ObjectState>,
    pub positions: Vec<Option<Position>>,
    pub agents: Vec<&'a Agent>,
}

impl<'a> Neighbors<'a> {
    pub fn position_at_offset(&self, offset: (i32, i32)) -> Option<Position> {
        for (index, candidate) in self.offsets.iter().enumerate() {
            if *candidate == offset {
                return self.positions.get(index).copied().flatten();
            }
        }
        None
    }

    pub fn is_free_offset(&self, offset: (i32, i32)) -> bool {
        if self.free_offsets.contains(&offset) {
            return true;
        }
        if let Some(position) = self.position_at_offset(offset) {
            for free in &self.free_offsets {
                if self.position_at_offset(*free) == Some(position) {
                    return true;
                }
            }
        }
        false
    }

    pub fn cell_at_offset(&self, offset: (i32, i32)) -> Result<&'a ObjectState, String> {
        for (candidate, state) in self.offsets.iter().zip(&self.cells) {
            if *candidate == offset {
                return Ok(*state);
            }
        }
        Err(format!(
            "No cell was read at neighbor offset ({}, {})",
            offset.0, offset.1
        ))
    }
}

impl Neighborhood {
    pub fn validate(&self) -> Result<(), String> {
        let mut offsets = HashSet::new();

        for &(row, col) in &self.offsets {
            if row == 0 && col == 0 {
                return Err("Neighborhood must not include the current cell (0, 0)".to_owned());
            }

            if !offsets.insert((row, col)) {
                return Err(format!("Duplicate neighborhood offset ({row}, {col})"));
            }
        }

        Ok(())
    }

    pub fn validate_offset(&self, offset: (i32, i32)) -> Result<(), String> {
        if !self.offsets.contains(&offset) {
            return Err(format!(
                "Neighbor offset ({}, {}) is not included in the neighborhood",
                offset.0, offset.1
            ));
        }
        Ok(())
    }

    pub fn read<'a>(
        &self,
        world: &'a World,
        position: Position,
        boundary: &'a Boundary,
    ) -> Result<Neighbors<'a>, String> {
        self.validate()?;
        world.validate()?;

        if position.row >= world.height() || position.col >= world.width() {
            return Err("Current position is outside the world".to_owned());
        }
        Ok(self.read_validated(world, position, boundary))
    }

    // Call only for a validated world and a position inside that world.
    pub(crate) fn read_validated<'a>(
        &self,
        world: &'a World,
        position: Position,
        boundary: &'a Boundary,
    ) -> Neighbors<'a> {
        let height = world.height() as i64;
        let width = world.width() as i64;
        let mut neighbors = Vec::with_capacity(self.offsets.len());
        let mut agent_positions = HashSet::new();
        let occupied: HashSet<_> = world.agents.iter().map(|agent| agent.position).collect();
        let mut free_offsets = Vec::new();
        let mut positions = Vec::with_capacity(self.offsets.len());

        for &(dr, dc) in &self.offsets {
            let row = position.row as i64 + i64::from(dr);
            let col = position.col as i64 + i64::from(dc);

            let (neighbor, target) = match boundary {
                Boundary::Wrap => {
                    let row = row.rem_euclid(height) as usize;
                    let col = col.rem_euclid(width) as usize;
                    (&world.cells[row][col], Some(Position { row, col }))
                }
                Boundary::Fixed { state } => {
                    if row < 0 || col < 0 || row >= height || col >= width {
                        (state, None)
                    } else {
                        (
                            &world.cells[row as usize][col as usize],
                            Some(Position {
                                row: row as usize,
                                col: col as usize,
                            }),
                        )
                    }
                }
            };

            if let Some(target) = target
                && target != position
                && agent_positions.insert(target)
                && !occupied.contains(&target)
            {
                free_offsets.push((dr, dc));
            }
            neighbors.push(neighbor);
            positions.push(target);
        }

        Neighbors {
            offsets: self.offsets.clone(),
            free_offsets,
            cells: neighbors,
            positions,
            agents: world
                .agents
                .iter()
                .filter(|agent| agent_positions.contains(&agent.position))
                .collect(),
        }
    }
}
