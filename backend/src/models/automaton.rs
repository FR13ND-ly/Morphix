use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{
    neighborhood::Neighborhood, object_type::ObjectType, rule::Rule, state::ObjectState,
    world::World,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomatonDefinition {
    pub name: String,
    pub object_types: Vec<ObjectType>,
    pub neighborhood: Neighborhood,
    pub boundary: Boundary,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Boundary {
    Wrap,
    Fixed { state: ObjectState },
}

impl AutomatonDefinition {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Automaton name must not be empty".to_owned());
        }

        if self.object_types.is_empty() {
            return Err("Automaton must define at least one object type".to_owned());
        }

        self.neighborhood.validate()?;

        let mut type_ids = HashSet::new();
        let mut type_names = HashSet::new();

        for object_type in &self.object_types {
            object_type
                .validate()
                .map_err(|error| format!("Object type {}: {error}", object_type.id))?;

            if !type_ids.insert(object_type.id) {
                return Err(format!("Duplicate object type id {}", object_type.id));
            }

            if !type_names.insert(&object_type.name) {
                return Err(format!("Duplicate object type name '{}'", object_type.name));
            }
        }

        if let Boundary::Fixed { state } = &self.boundary {
            self.validate_object_state(state)
                .map_err(|error| format!("Fixed boundary: {error}"))?;
        }

        for (index, rule) in self.rules.iter().enumerate() {
            rule.validate(self)
                .map_err(|error| format!("Rule {index}: {error}"))?;
        }

        Ok(())
    }

    pub fn validate_world(&self, world: &World) -> Result<(), String> {
        self.validate()?;
        world.validate()?;

        for (row, cells) in world.cells.iter().enumerate() {
            for (col, state) in cells.iter().enumerate() {
                self.validate_object_state(state)
                    .map_err(|error| format!("Cell ({row}, {col}): {error}"))?;
            }
        }

        for agent in &world.agents {
            self.validate_object_state(&agent.state)
                .map_err(|error| format!("Agent {}: {error}", agent.id))?;
        }

        Ok(())
    }

    pub fn validate_object_state(&self, state: &ObjectState) -> Result<(), String> {
        self.object_type(state.type_id)?.validate_state(state)
    }

    pub(crate) fn object_type(&self, type_id: u8) -> Result<&ObjectType, String> {
        self.object_types
            .iter()
            .find(|object_type| object_type.id == type_id)
            .ok_or_else(|| format!("Unknown object type id {type_id}"))
    }

    pub(crate) fn validate_property_reference(&self, name: &str) -> Result<(), String> {
        if !self
            .object_types
            .iter()
            .any(|object_type| object_type.properties.contains_key(name))
        {
            return Err(format!("Unknown property '{name}'"));
        }

        Ok(())
    }
}
