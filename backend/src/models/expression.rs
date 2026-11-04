use serde::{Deserialize, Serialize};

use super::{automaton::AutomatonDefinition, neighborhood::Neighbors, state::ObjectState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "args",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Expression {
    Constant(f64),
    Property(String),
    CellProperty(String),
    NeighborProperty {
        offset: (i32, i32),
        property: String,
    },
    CountNeighbors {
        type_id: u8,
    },
    SumNeighbors {
        property: String,
    },
    CountNeighborAgents {
        type_id: u8,
    },
    SumNeighborAgents {
        type_id: u8,
        property: String,
    },
    Add(Box<Expression>, Box<Expression>),
    Subtract(Box<Expression>, Box<Expression>),
    Multiply(Box<Expression>, Box<Expression>),
    Divide(Box<Expression>, Box<Expression>),
    Power(Box<Expression>, Box<Expression>),
}

impl Expression {
    pub fn validate(&self, automaton: &AutomatonDefinition) -> Result<(), String> {
        match self {
            Self::Constant(_) => {}
            Self::Property(name)
            | Self::CellProperty(name)
            | Self::SumNeighbors { property: name } => {
                automaton.validate_property_reference(name)?;
            }
            Self::NeighborProperty { offset, property } => {
                automaton.neighborhood.validate_offset(*offset)?;
                automaton.validate_property_reference(property)?;
            }
            Self::CountNeighbors { type_id } | Self::CountNeighborAgents { type_id } => {
                automaton.object_type(*type_id)?;
            }
            Self::SumNeighborAgents { type_id, property } => {
                if !automaton
                    .object_type(*type_id)?
                    .properties
                    .contains_key(property)
                {
                    return Err(format!(
                        "Unknown property '{property}' for neighbor agent type {type_id}"
                    ));
                }
            }
            Self::Add(left, right)
            | Self::Subtract(left, right)
            | Self::Multiply(left, right)
            | Self::Divide(left, right)
            | Self::Power(left, right) => {
                left.validate(automaton)?;
                right.validate(automaton)?;

                if matches!(self, Self::Divide(_, _)) && right.constant_value() == Some(0.0) {
                    return Err("Division by zero in constant denominator".to_owned());
                }
            }
        }

        if self
            .constant_value()
            .is_some_and(|value| !value.is_finite())
        {
            return Err("Constant expression result must be finite".to_owned());
        }

        Ok(())
    }

    pub(crate) fn constant_value(&self) -> Option<f64> {
        match self {
            Self::Constant(value) => Some(*value),
            Self::Add(left, right) => Some(left.constant_value()? + right.constant_value()?),
            Self::Subtract(left, right) => Some(left.constant_value()? - right.constant_value()?),
            Self::Multiply(left, right) => Some(left.constant_value()? * right.constant_value()?),
            Self::Divide(left, right) => Some(left.constant_value()? / right.constant_value()?),
            Self::Power(base, exponent) => {
                Some(base.constant_value()?.powf(exponent.constant_value()?))
            }
            Self::Property(_)
            | Self::CellProperty(_)
            | Self::NeighborProperty { .. }
            | Self::CountNeighbors { .. }
            | Self::SumNeighbors { .. }
            | Self::CountNeighborAgents { .. }
            | Self::SumNeighborAgents { .. } => None,
        }
    }

    pub fn evaluate(
        &self,
        current: &ObjectState,
        cell: &ObjectState,
        neighbors: &Neighbors<'_>,
    ) -> Result<f64, String> {
        let value = match self {
            Self::Constant(value) => *value,
            Self::Property(name) => *current
                .properties
                .get(name)
                .ok_or_else(|| format!("Missing property '{name}'"))?,
            Self::CellProperty(name) => *cell
                .properties
                .get(name)
                .ok_or_else(|| format!("Current cell is missing property '{name}'"))?,
            Self::NeighborProperty { offset, property } => *neighbors
                .cell_at_offset(*offset)?
                .properties
                .get(property)
                .ok_or_else(|| {
                    format!(
                        "Neighbor at offset ({}, {}) is missing property '{property}'",
                        offset.0, offset.1
                    )
                })?,
            Self::CountNeighbors { type_id } => neighbors
                .cells
                .iter()
                .filter(|neighbor| neighbor.type_id == *type_id)
                .count() as f64,
            Self::SumNeighbors { property } => {
                let mut sum = 0.0;

                for (index, neighbor) in neighbors.cells.iter().enumerate() {
                    let value = neighbor.properties.get(property).ok_or_else(|| {
                        format!("Neighbor {index} is missing property '{property}'")
                    })?;
                    sum += *value;
                }

                sum
            }
            Self::CountNeighborAgents { type_id } => neighbors
                .agents
                .iter()
                .filter(|agent| agent.state.type_id == *type_id)
                .count() as f64,
            Self::SumNeighborAgents { type_id, property } => {
                let mut sum = 0.0;
                for agent in neighbors
                    .agents
                    .iter()
                    .filter(|agent| agent.state.type_id == *type_id)
                {
                    let value = agent.state.properties.get(property).ok_or_else(|| {
                        format!(
                            "Neighbor agent {} is missing property '{property}'",
                            agent.id
                        )
                    })?;
                    sum += *value;
                }
                sum
            }
            Self::Add(left, right) => {
                left.evaluate(current, cell, neighbors)?
                    + right.evaluate(current, cell, neighbors)?
            }
            Self::Subtract(left, right) => {
                left.evaluate(current, cell, neighbors)?
                    - right.evaluate(current, cell, neighbors)?
            }
            Self::Multiply(left, right) => {
                left.evaluate(current, cell, neighbors)?
                    * right.evaluate(current, cell, neighbors)?
            }
            Self::Divide(left, right) => {
                let numerator = left.evaluate(current, cell, neighbors)?;
                let denominator = right.evaluate(current, cell, neighbors)?;

                if denominator == 0.0 {
                    return Err("Division by zero".to_owned());
                }

                numerator / denominator
            }
            Self::Power(base, exponent) => base
                .evaluate(current, cell, neighbors)?
                .powf(exponent.evaluate(current, cell, neighbors)?),
        };

        if !value.is_finite() {
            return Err("Expression result must be finite".to_owned());
        }

        Ok(value)
    }
}
