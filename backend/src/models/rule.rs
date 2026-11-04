use rand::{Rng, RngExt};
use serde::{Deserialize, Deserializer, Serialize};

use super::{
    action::{Action, ActionResult, apply_actions},
    automaton::AutomatonDefinition,
    condition::Condition,
    expression::Expression,
    neighborhood::Neighbors,
    state::ObjectState,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub target: RuleTarget,
    pub condition: Condition,
    #[serde(deserialize_with = "deserialize_probability")]
    pub probability: Expression,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleTarget {
    Cell,
    Agent,
}

impl Rule {
    pub fn validate(&self, automaton: &AutomatonDefinition) -> Result<(), String> {
        self.probability
            .validate(automaton)
            .map_err(|error| format!("probability: {error}"))?;
        if let Some(value) = self.probability.constant_value() {
            checked_probability(value).map_err(|error| format!("probability: {error}"))?;
        }

        self.condition
            .validate(automaton)
            .map_err(|error| format!("Condition: {error}"))?;

        let mut target_type = None;
        let mut target_cell_type = None;
        let mut has_movement = false;
        let mut has_spawn = false;
        let mut has_neighbor_removal = false;
        for (index, action) in self.actions.iter().enumerate() {
            action
                .validate(automaton)
                .map_err(|error| format!("Action {index}: {error}"))?;

            match action {
                Action::AddNeighborProperty { .. } => {
                    if self.target != RuleTarget::Cell {
                        return Err(format!(
                            "Action {index}: AddNeighborProperty is only allowed in cell rules"
                        ));
                    }
                }
                Action::RemoveNeighborAgent { .. } => {
                    if self.target != RuleTarget::Agent {
                        return Err(format!(
                            "Action {index}: RemoveNeighborAgent is only allowed in agent rules"
                        ));
                    }
                    if has_neighbor_removal {
                        return Err(format!(
                            "Action {index}: A rule may contain at most one RemoveNeighborAgent action"
                        ));
                    }
                    has_neighbor_removal = true;
                }
                Action::SetCellType { type_id } => {
                    if self.target != RuleTarget::Agent {
                        return Err(format!(
                            "Action {index}: SetCellType is only allowed in agent rules"
                        ));
                    }
                    target_cell_type = Some(automaton.object_type(*type_id)?);
                }
                Action::SetCellProperty { property, .. } => {
                    if self.target != RuleTarget::Agent {
                        return Err(format!(
                            "Action {index}: SetCellProperty is only allowed in agent rules"
                        ));
                    }
                    if let Some(object_type) = target_cell_type
                        && !object_type.properties.contains_key(property)
                    {
                        return Err(format!(
                            "Action {index}: Unknown cell property '{property}' for type {}",
                            object_type.id
                        ));
                    }
                }
                Action::SpawnAgent { .. } => {
                    if has_spawn {
                        return Err(format!(
                            "Action {index}: A rule may contain at most one SpawnAgent action"
                        ));
                    }
                    has_spawn = true;
                }
                Action::RemoveAgent {} => {
                    if self.target != RuleTarget::Agent {
                        return Err(format!(
                            "Action {index}: RemoveAgent is only allowed in agent rules"
                        ));
                    }
                    if self.actions.len() != 1 {
                        return Err(format!(
                            "Action {index}: RemoveAgent must be the only action in its rule"
                        ));
                    }
                }
                Action::SetType { type_id } => {
                    target_type = Some(automaton.object_type(*type_id)?);
                }
                Action::SetProperty { property, .. } => {
                    if let Some(object_type) = target_type
                        && !object_type.properties.contains_key(property)
                    {
                        return Err(format!(
                            "Action {index}: Unknown property '{property}' for type {}",
                            object_type.id
                        ));
                    }
                }
                Action::Move { .. }
                | Action::MoveToFreeNeighbor { .. }
                | Action::MoveToFreeCell { .. }
                | Action::MoveToRemovedNeighbor {} => {
                    if self.target != RuleTarget::Agent {
                        let name = match action {
                            Action::Move { .. } => "Move",
                            Action::MoveToFreeNeighbor { .. } => "MoveToFreeNeighbor",
                            Action::MoveToFreeCell { .. } => "MoveToFreeCell",
                            _ => "MoveToRemovedNeighbor",
                        };
                        return Err(format!(
                            "Action {index}: {name} is only allowed in agent rules"
                        ));
                    }
                    if has_movement {
                        return Err(format!(
                            "Action {index}: A rule may contain at most one movement action"
                        ));
                    }
                    has_movement = true;
                }
            }
        }

        if self
            .actions
            .iter()
            .any(|action| matches!(action, Action::MoveToRemovedNeighbor {}))
            && !has_neighbor_removal
        {
            return Err(
                "MoveToRemovedNeighbor requires RemoveNeighborAgent in the same rule".to_owned(),
            );
        }

        Ok(())
    }
}

pub fn apply_rules(
    target: RuleTarget,
    current: &ObjectState,
    cell: &ObjectState,
    neighbors: &Neighbors<'_>,
    automaton: &AutomatonDefinition,
    rng: &mut impl Rng,
) -> Result<ActionResult, String> {
    automaton.validate()?;
    automaton.validate_object_state(current)?;

    for (index, rule) in automaton.rules.iter().enumerate() {
        if rule.target != target {
            continue;
        }

        let matches = rule
            .condition
            .evaluate(current, cell, neighbors)
            .map_err(|error| format!("Rule {index}: {error}"))?;

        if !matches {
            continue;
        }

        let probability = rule
            .probability
            .evaluate(current, cell, neighbors)
            .and_then(checked_probability)
            .map_err(|error| format!("Rule {index}: probability: {error}"))?;

        if !rng.random_bool(probability) {
            break;
        }

        return apply_actions(&rule.actions, current, cell, neighbors, automaton, rng)
            .map_err(|error| format!("Rule {index}: {error}"));
    }

    Ok(ActionResult::unchanged(current))
}

fn checked_probability(value: f64) -> Result<f64, String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err("Rule probability must be finite and between 0 and 1".to_owned());
    }
    Ok(value)
}

fn deserialize_probability<'de, D>(deserializer: D) -> Result<Expression, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ProbabilityInput {
        Number(f64),
        Formula(Expression),
    }

    Ok(match ProbabilityInput::deserialize(deserializer)? {
        ProbabilityInput::Number(value) => Expression::Constant(value),
        ProbabilityInput::Formula(expression) => expression,
    })
}
