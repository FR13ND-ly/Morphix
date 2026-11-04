use std::collections::{BTreeMap, HashSet};

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use super::{
    automaton::AutomatonDefinition, expression::Expression, neighborhood::Neighbors,
    state::ObjectState,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    SetProperty {
        property: String,
        value: Expression,
    },
    SetType {
        type_id: u8,
    },
    SetCellProperty {
        property: String,
        value: Expression,
    },
    SetCellType {
        type_id: u8,
    },
    AddNeighborProperty {
        offset: (i32, i32),
        property: String,
        value: Expression,
    },
    Move {
        row_offset: Expression,
        col_offset: Expression,
    },
    MoveToFreeNeighbor {
        selection: NeighborSelection,
        #[serde(default)]
        cell_type: Option<u8>,
        #[serde(default)]
        offsets: Option<Vec<(i32, i32)>>,
    },
    MoveToFreeCell {
        selection: NeighborSelection,
        #[serde(default)]
        cell_type: Option<u8>,
    },
    MoveToRemovedNeighbor {},
    RemoveNeighborAgent {
        type_id: u8,
        selection: NeighborSelection,
    },
    SpawnAgent {
        type_id: u8,
        row_offset: Expression,
        col_offset: Expression,
        #[serde(default)]
        properties: BTreeMap<String, Expression>,
    },
    RemoveAgent {},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NeighborSelection {
    First,
    Random,
}

#[derive(Debug, Clone)]
pub struct AgentSpawn {
    pub state: ObjectState,
    pub offset: (i32, i32),
}

#[derive(Debug, Clone)]
pub struct NeighborPropertyUpdate {
    pub offset: (i32, i32),
    pub property: String,
    pub value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Movement {
    Offset((i32, i32)),
    FreeCell {
        selection: NeighborSelection,
        cell_type: Option<u8>,
    },
    RemovedNeighbor,
}

#[derive(Debug, Clone, Default)]
pub struct CellUpdate {
    pub type_id: Option<u8>,
    pub properties: BTreeMap<String, f64>,
}

impl CellUpdate {
    pub fn apply(
        &self,
        cell: &ObjectState,
        automaton: &AutomatonDefinition,
    ) -> Result<ObjectState, String> {
        let mut next = match self.type_id {
            Some(type_id) => automaton.object_type(type_id)?.create_state()?,
            None => cell.clone(),
        };
        for (name, value) in &self.properties {
            if !next.properties.contains_key(name) {
                return Err(format!(
                    "Unknown cell property '{name}' for type {}",
                    next.type_id
                ));
            }
            next.properties.insert(name.clone(), *value);
        }
        automaton.validate_object_state(&next)?;
        Ok(next)
    }
}

#[derive(Debug, Clone)]
pub struct ActionResult {
    pub state: ObjectState,
    pub movement: Option<Movement>,
    pub remove_agent: bool,
    pub neighbor_to_remove: Option<u64>,
    pub spawn: Option<AgentSpawn>,
    pub cell_update: Option<CellUpdate>,
    pub neighbor_updates: Vec<NeighborPropertyUpdate>,
}

impl ActionResult {
    pub fn unchanged(current: &ObjectState) -> Self {
        Self {
            state: current.clone(),
            movement: None,
            remove_agent: false,
            neighbor_to_remove: None,
            spawn: None,
            cell_update: None,
            neighbor_updates: Vec::new(),
        }
    }
}

impl Action {
    pub(crate) fn is_movement(&self) -> bool {
        matches!(
            self,
            Self::Move { .. }
                | Self::MoveToFreeNeighbor { .. }
                | Self::MoveToFreeCell { .. }
                | Self::MoveToRemovedNeighbor {}
        )
    }

    pub fn validate(&self, automaton: &AutomatonDefinition) -> Result<(), String> {
        match self {
            Self::RemoveAgent {} | Self::MoveToRemovedNeighbor {} => {}
            Self::AddNeighborProperty {
                offset,
                property,
                value,
            } => {
                automaton.neighborhood.validate_offset(*offset)?;
                automaton.validate_property_reference(property)?;
                value.validate(automaton)?;
            }
            Self::SetProperty { property, value } | Self::SetCellProperty { property, value } => {
                automaton.validate_property_reference(property)?;
                value.validate(automaton)?;
            }
            Self::SetType { type_id }
            | Self::SetCellType { type_id }
            | Self::RemoveNeighborAgent { type_id, .. } => {
                automaton.object_type(*type_id)?;
            }
            Self::Move {
                row_offset,
                col_offset,
            } => {
                validate_offsets(row_offset, col_offset, automaton)?;
            }
            Self::MoveToFreeNeighbor { cell_type, .. } | Self::MoveToFreeCell { cell_type, .. } => {
                if let Some(type_id) = cell_type {
                    automaton.object_type(*type_id)?;
                }
                if let Self::MoveToFreeNeighbor {
                    offsets: Some(offsets),
                    ..
                } = self
                {
                    if offsets.len() > 32 {
                        return Err("At most 32 movement offsets may be specified".to_owned());
                    }
                    let mut seen = HashSet::new();
                    for offset in offsets {
                        automaton.neighborhood.validate_offset(*offset)?;
                        if !seen.insert(*offset) {
                            return Err("Duplicate movement offset".to_owned());
                        }
                    }
                }
            }
            Self::SpawnAgent {
                type_id,
                row_offset,
                col_offset,
                properties,
            } => {
                validate_offsets(row_offset, col_offset, automaton)?;
                let object_type = automaton.object_type(*type_id)?;
                let mut state = object_type.create_state()?;
                for (name, expression) in properties {
                    if !object_type.properties.contains_key(name) {
                        return Err(format!(
                            "Unknown spawn property '{name}' for type {type_id}"
                        ));
                    }
                    expression.validate(automaton)?;
                    if let Some(value) = expression.constant_value() {
                        state.properties.insert(name.clone(), value);
                    }
                }
                object_type.validate_state(&state)?;
            }
        }

        Ok(())
    }
}

pub fn apply_actions(
    actions: &[Action],
    current: &ObjectState,
    cell: &ObjectState,
    neighbors: &Neighbors<'_>,
    automaton: &AutomatonDefinition,
    rng: &mut impl Rng,
) -> Result<ActionResult, String> {
    automaton.validate()?;
    automaton.validate_object_state(current)?;
    apply_actions_validated(actions, current, cell, neighbors, automaton, rng)
}

// The simulator has already validated the definition and current world.
pub(crate) fn apply_actions_validated(
    actions: &[Action],
    current: &ObjectState,
    cell: &ObjectState,
    neighbors: &Neighbors<'_>,
    automaton: &AutomatonDefinition,
    rng: &mut impl Rng,
) -> Result<ActionResult, String> {
    if actions.len() > 1
        && actions
            .iter()
            .any(|action| matches!(action, Action::RemoveAgent {}))
    {
        return Err("RemoveAgent must be the only action in its action list".to_owned());
    }
    if actions.iter().filter(|action| action.is_movement()).count() > 1 {
        return Err("At most one movement action may be applied to an object per step".to_owned());
    }
    if actions
        .iter()
        .any(|action| matches!(action, Action::MoveToRemovedNeighbor {}))
        && !actions
            .iter()
            .any(|action| matches!(action, Action::RemoveNeighborAgent { .. }))
    {
        return Err(
            "MoveToRemovedNeighbor requires RemoveNeighborAgent in the same rule".to_owned(),
        );
    }

    let mut next = current.clone();
    if actions
        .iter()
        .filter(|action| matches!(action, Action::RemoveNeighborAgent { .. }))
        .count()
        > 1
    {
        return Err("At most one RemoveNeighborAgent action may be applied per step".to_owned());
    }
    let mut neighbor_to_remove = None;
    for action in actions {
        if let Action::RemoveNeighborAgent { type_id, selection } = action {
            action.validate(automaton)?;
            let candidates: Vec<_> = neighbors
                .agents
                .iter()
                .filter(|agent| agent.state.type_id == *type_id)
                .collect();
            if candidates.is_empty() {
                return Ok(ActionResult::unchanged(current));
            }
            let index = match selection {
                NeighborSelection::First => 0,
                NeighborSelection::Random => rng.random_range(0..candidates.len()),
            };
            neighbor_to_remove = Some(candidates[index].id);
        }
    }
    let mut movement = None;
    let mut remove_agent = false;
    let mut spawn = None;
    let mut cell_update: Option<CellUpdate> = None;
    let mut neighbor_updates = Vec::new();

    for action in actions {
        match action {
            Action::RemoveAgent {} => remove_agent = true,
            Action::RemoveNeighborAgent { .. } => {}
            Action::AddNeighborProperty {
                offset,
                property,
                value,
            } => {
                action.validate(automaton)?;
                neighbor_updates.push(NeighborPropertyUpdate {
                    offset: *offset,
                    property: property.clone(),
                    value: value.evaluate(current, cell, neighbors)?,
                });
            }
            Action::MoveToFreeCell {
                selection,
                cell_type,
            } => {
                action.validate(automaton)?;
                movement = Some(Movement::FreeCell {
                    selection: *selection,
                    cell_type: *cell_type,
                });
            }
            Action::MoveToRemovedNeighbor {} => movement = Some(Movement::RemovedNeighbor),
            Action::SetProperty { property, value } => {
                if !next.properties.contains_key(property) {
                    return Err(format!(
                        "Unknown property '{property}' for type {}",
                        next.type_id
                    ));
                }

                let value = value.evaluate(current, cell, neighbors)?;
                next.properties.insert(property.clone(), value);
            }
            Action::SetType { type_id } => {
                next = automaton.object_type(*type_id)?.create_state()?;
            }
            Action::SetCellProperty { property, value } => {
                let type_id = cell_update
                    .as_ref()
                    .and_then(|update| update.type_id)
                    .unwrap_or(cell.type_id);
                if !automaton
                    .object_type(type_id)?
                    .properties
                    .contains_key(property)
                {
                    return Err(format!(
                        "Unknown cell property '{property}' for type {type_id}"
                    ));
                }
                let value = value.evaluate(current, cell, neighbors)?;
                cell_update
                    .get_or_insert_with(CellUpdate::default)
                    .properties
                    .insert(property.clone(), value);
            }
            Action::SetCellType { type_id } => {
                automaton.object_type(*type_id)?;
                cell_update = Some(CellUpdate {
                    type_id: Some(*type_id),
                    properties: BTreeMap::new(),
                });
            }
            Action::Move {
                row_offset,
                col_offset,
            } => {
                let row = movement_offset(row_offset.evaluate(current, cell, neighbors)?)
                    .map_err(|error| format!("row_offset: {error}"))?;
                let col = movement_offset(col_offset.evaluate(current, cell, neighbors)?)
                    .map_err(|error| format!("col_offset: {error}"))?;
                movement = Some(Movement::Offset((row, col)));
            }
            Action::MoveToFreeNeighbor {
                selection,
                cell_type,
                offsets,
            } => {
                action.validate(automaton)?;
                let mut candidates = Vec::new();
                let mut seen = HashSet::new();
                for &offset in offsets.as_deref().unwrap_or(&neighbors.free_offsets) {
                    if !neighbors.is_free_offset(offset) {
                        continue;
                    }
                    if let Some(position) = neighbors.position_at_offset(offset)
                        && !seen.insert(position)
                    {
                        continue;
                    }
                    let neighbor = neighbors.cell_at_offset(offset)?;
                    if cell_type.is_none_or(|type_id| neighbor.type_id == type_id) {
                        candidates.push(offset);
                    }
                }
                movement = match selection {
                    NeighborSelection::First => candidates.first().copied().map(Movement::Offset),
                    NeighborSelection::Random if !candidates.is_empty() => Some(Movement::Offset(
                        candidates[rng.random_range(0..candidates.len())],
                    )),
                    NeighborSelection::Random => None,
                };
            }
            Action::SpawnAgent {
                type_id,
                row_offset,
                col_offset,
                properties,
            } => {
                if spawn.is_some() {
                    return Err(
                        "At most one SpawnAgent action may be applied to an object per step"
                            .to_owned(),
                    );
                }
                let row = movement_offset(row_offset.evaluate(current, cell, neighbors)?)
                    .map_err(|error| format!("SpawnAgent row_offset: {error}"))?;
                let col = movement_offset(col_offset.evaluate(current, cell, neighbors)?)
                    .map_err(|error| format!("SpawnAgent col_offset: {error}"))?;
                let mut state = automaton.object_type(*type_id)?.create_state()?;
                for (name, expression) in properties {
                    let value = expression
                        .evaluate(current, cell, neighbors)
                        .map_err(|error| format!("SpawnAgent property '{name}': {error}"))?;
                    state.properties.insert(name.clone(), value);
                }
                automaton
                    .validate_object_state(&state)
                    .map_err(|error| format!("SpawnAgent: {error}"))?;
                spawn = Some(AgentSpawn {
                    state,
                    offset: (row, col),
                });
            }
        }
    }

    automaton.validate_object_state(&next)?;
    if let Some(update) = &cell_update {
        update
            .apply(cell, automaton)
            .map_err(|error| format!("Cell update: {error}"))?;
    }

    Ok(ActionResult {
        state: next,
        movement,
        remove_agent,
        neighbor_to_remove,
        spawn,
        cell_update,
        neighbor_updates,
    })
}

fn validate_offsets(
    row: &Expression,
    col: &Expression,
    automaton: &AutomatonDefinition,
) -> Result<(), String> {
    for (name, expression) in [("row_offset", row), ("col_offset", col)] {
        expression
            .validate(automaton)
            .map_err(|error| format!("{name}: {error}"))?;
        if let Some(value) = expression.constant_value() {
            movement_offset(value).map_err(|error| format!("{name}: {error}"))?;
        }
    }
    Ok(())
}

fn movement_offset(value: f64) -> Result<i32, String> {
    if !value.is_finite()
        || value.fract() != 0.0
        || value < f64::from(i32::MIN)
        || value > f64::from(i32::MAX)
    {
        return Err("Movement offset must be a finite integer in the i32 range".to_owned());
    }
    Ok(value as i32)
}
