use serde::{Deserialize, Serialize};

use super::{
    action::Action, automaton::AutomatonDefinition, condition::Condition, expression::Expression,
    world::World,
};

const MAX_GENERATIONS: usize = 1_000;
pub(crate) const MAX_HISTORY_UNITS: usize = 100_000;
const MAX_RULES: usize = 64;
const MAX_NEIGHBORS: usize = 32;
const MAX_RULE_NODES: usize = 512;
const MAX_RULE_DEPTH: usize = 16;
const MAX_PROPERTY_NAME_BYTES: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationRequest {
    pub automaton: AutomatonDefinition,
    pub initial_world: World,
    pub generations: usize,
    pub seed: u64,
    #[serde(default = "default_save_history")]
    pub save_history: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationValidationResponse {
    pub valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationGeneration {
    pub generation: usize,
    pub world: World,
}

// Internal continuation data; the world itself is stored with its generation.
#[derive(Debug, Serialize, Deserialize)]
pub struct SimulationCheckpoint {
    pub rng_word_pos: u128,
    pub last_agent_id: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationStepRequest {
    pub generation: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationResponse {
    pub id: i64,
    pub seed: u64,
    pub history: Vec<SimulationGeneration>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredSimulation {
    pub id: i64,
    pub created_at: String,
    pub request: SimulationRequest,
    pub history: Vec<SimulationGeneration>,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SimulationListQuery {
    pub page: u32,
    pub page_size: u32,
    pub search: String,
    pub sort: SimulationSort,
    pub order: SortOrder,
}

impl Default for SimulationListQuery {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 20,
            search: String::new(),
            sort: SimulationSort::CreatedAt,
            order: SortOrder::Desc,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationSort {
    CreatedAt,
    Name,
    Generations,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimulationSummary {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub width: usize,
    pub height: usize,
    pub generations: usize,
    pub save_history: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimulationListResponse {
    pub items: Vec<SimulationSummary>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl SimulationRequest {
    pub fn validate_limits(&self) -> Result<(), String> {
        if self.generations > MAX_GENERATIONS {
            return Err(format!(
                "At most {MAX_GENERATIONS} generations may be requested"
            ));
        }

        if self.automaton.rules.len() > MAX_RULES {
            return Err(format!("At most {MAX_RULES} rules may be defined"));
        }

        if self.automaton.neighborhood.offsets.len() > MAX_NEIGHBORS {
            return Err(format!(
                "At most {MAX_NEIGHBORS} neighborhood offsets may be defined"
            ));
        }

        let mut max_properties = 0;
        for object_type in &self.automaton.object_types {
            max_properties = max_properties.max(object_type.properties.len());
            for name in object_type.properties.keys() {
                if name.len() > MAX_PROPERTY_NAME_BYTES {
                    return Err(format!(
                        "Property names may use at most {MAX_PROPERTY_NAME_BYTES} bytes"
                    ));
                }
            }
        }

        // A state and each of its possible properties count as one history unit.
        // The largest declared type covers both object and terrain type changes.
        let retained_generations = if self.save_history {
            self.generations + 1
        } else {
            1
        };
        let history_units = world_history_units(&self.initial_world, max_properties)
            .and_then(|count| count.checked_mul(retained_generations));

        if history_units.is_none_or(|count| count > MAX_HISTORY_UNITS) {
            return Err(format!(
                "Simulation history exceeds {MAX_HISTORY_UNITS} state/property units"
            ));
        }

        let mut remaining_nodes = MAX_RULE_NODES;
        for rule in &self.automaton.rules {
            check_condition(&rule.condition, 0, &mut remaining_nodes)?;
            check_expression(&rule.probability, 0, &mut remaining_nodes)?;
            for action in &rule.actions {
                check_node(0, &mut remaining_nodes)?;
                match action {
                    Action::SetProperty { value, .. }
                    | Action::SetCellProperty { value, .. }
                    | Action::AddNeighborProperty { value, .. } => {
                        check_expression(value, 0, &mut remaining_nodes)?;
                    }
                    Action::Move {
                        row_offset,
                        col_offset,
                    } => {
                        check_expression(row_offset, 0, &mut remaining_nodes)?;
                        check_expression(col_offset, 0, &mut remaining_nodes)?;
                    }
                    Action::SpawnAgent {
                        row_offset,
                        col_offset,
                        properties,
                        ..
                    } => {
                        check_expression(row_offset, 0, &mut remaining_nodes)?;
                        check_expression(col_offset, 0, &mut remaining_nodes)?;
                        for expression in properties.values() {
                            check_expression(expression, 0, &mut remaining_nodes)?;
                        }
                    }
                    Action::SetType { .. }
                    | Action::RemoveNeighborAgent { .. }
                    | Action::MoveToFreeNeighbor { .. }
                    | Action::MoveToFreeCell { .. }
                    | Action::MoveToRemovedNeighbor {}
                    | Action::SetCellType { .. }
                    | Action::RemoveAgent {} => {}
                }
            }
        }

        Ok(())
    }
}

fn default_save_history() -> bool {
    true
}

pub(crate) fn world_history_units(world: &World, max_properties: usize) -> Option<usize> {
    let mut objects = world.agents.len();
    for row in &world.cells {
        objects = objects.checked_add(row.len())?;
    }
    objects.checked_mul(max_properties.checked_add(1)?)
}

fn check_node(depth: usize, remaining_nodes: &mut usize) -> Result<(), String> {
    if depth > MAX_RULE_DEPTH {
        return Err(format!("Rule nesting exceeds depth {MAX_RULE_DEPTH}"));
    }
    if *remaining_nodes == 0 {
        return Err(format!(
            "Rules exceed {MAX_RULE_NODES} condition/action/expression nodes"
        ));
    }
    *remaining_nodes -= 1;
    Ok(())
}

fn check_condition(
    condition: &Condition,
    depth: usize,
    remaining_nodes: &mut usize,
) -> Result<(), String> {
    check_node(depth, remaining_nodes)?;
    match condition {
        Condition::Compare { left, right, .. } => {
            check_expression(left, depth + 1, remaining_nodes)?;
            check_expression(right, depth + 1, remaining_nodes)?;
        }
        Condition::All(conditions) | Condition::Any(conditions) => {
            for child in conditions {
                check_condition(child, depth + 1, remaining_nodes)?;
            }
        }
        Condition::Not(child) => check_condition(child, depth + 1, remaining_nodes)?,
        Condition::Always
        | Condition::TypeIs(_)
        | Condition::CellTypeIs(_)
        | Condition::NeighborTypeIs { .. } => {}
    }
    Ok(())
}

fn check_expression(
    expression: &Expression,
    depth: usize,
    remaining_nodes: &mut usize,
) -> Result<(), String> {
    check_node(depth, remaining_nodes)?;
    match expression {
        Expression::Add(left, right)
        | Expression::Subtract(left, right)
        | Expression::Multiply(left, right)
        | Expression::Divide(left, right)
        | Expression::Power(left, right) => {
            check_expression(left, depth + 1, remaining_nodes)?;
            check_expression(right, depth + 1, remaining_nodes)?;
        }
        Expression::Constant(_)
        | Expression::Property(_)
        | Expression::CellProperty(_)
        | Expression::NeighborProperty { .. }
        | Expression::CountNeighbors { .. }
        | Expression::SumNeighbors { .. }
        | Expression::CountNeighborAgents { .. }
        | Expression::SumNeighborAgents { .. } => {}
    }
    Ok(())
}
