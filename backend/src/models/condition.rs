use serde::{Deserialize, Serialize};

use super::{
    automaton::AutomatonDefinition, expression::Expression, neighborhood::Neighbors,
    state::ObjectState,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "args",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Condition {
    Always,
    TypeIs(u8),
    CellTypeIs(u8),
    NeighborTypeIs {
        offset: (i32, i32),
        type_id: u8,
    },
    Compare {
        left: Expression,
        operator: Comparison,
        right: Expression,
    },
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    ApproximatelyEqual { tolerance: f64 },
}

impl Condition {
    pub fn validate(&self, automaton: &AutomatonDefinition) -> Result<(), String> {
        match self {
            Self::Always => {}
            Self::TypeIs(type_id) | Self::CellTypeIs(type_id) => {
                automaton.object_type(*type_id)?;
            }
            Self::NeighborTypeIs { offset, type_id } => {
                automaton.neighborhood.validate_offset(*offset)?;
                automaton.object_type(*type_id)?;
            }
            Self::Compare {
                left,
                operator,
                right,
            } => {
                operator.validate()?;
                left.validate(automaton)?;
                right.validate(automaton)?;
            }
            Self::All(conditions) | Self::Any(conditions) => {
                for (index, condition) in conditions.iter().enumerate() {
                    condition
                        .validate(automaton)
                        .map_err(|error| format!("Branch {index}: {error}"))?;
                }
            }
            Self::Not(condition) => condition.validate(automaton)?,
        }

        Ok(())
    }

    pub fn evaluate(
        &self,
        current: &ObjectState,
        cell: &ObjectState,
        neighbors: &Neighbors<'_>,
    ) -> Result<bool, String> {
        match self {
            Self::Always => Ok(true),
            Self::TypeIs(type_id) => Ok(current.type_id == *type_id),
            Self::CellTypeIs(type_id) => Ok(cell.type_id == *type_id),
            Self::NeighborTypeIs { offset, type_id } => {
                Ok(neighbors.cell_at_offset(*offset)?.type_id == *type_id)
            }
            Self::Compare {
                left,
                operator,
                right,
            } => {
                let left = left.evaluate(current, cell, neighbors)?;
                let right = right.evaluate(current, cell, neighbors)?;
                operator.validate()?;

                let result = match operator {
                    Comparison::Equal => left == right,
                    Comparison::NotEqual => left != right,
                    Comparison::Less => left < right,
                    Comparison::LessOrEqual => left <= right,
                    Comparison::Greater => left > right,
                    Comparison::GreaterOrEqual => left >= right,
                    Comparison::ApproximatelyEqual { tolerance } => {
                        (left - right).abs() <= *tolerance
                    }
                };

                Ok(result)
            }
            Self::All(conditions) => {
                for condition in conditions {
                    if !condition.evaluate(current, cell, neighbors)? {
                        return Ok(false);
                    }
                }

                Ok(true)
            }
            Self::Any(conditions) => {
                for condition in conditions {
                    if condition.evaluate(current, cell, neighbors)? {
                        return Ok(true);
                    }
                }

                Ok(false)
            }
            Self::Not(condition) => Ok(!condition.evaluate(current, cell, neighbors)?),
        }
    }
}

impl Comparison {
    pub fn validate(&self) -> Result<(), String> {
        if let Self::ApproximatelyEqual { tolerance } = self
            && (!tolerance.is_finite() || *tolerance < 0.0)
        {
            return Err("Comparison tolerance must be finite and non-negative".to_owned());
        }

        Ok(())
    }
}
