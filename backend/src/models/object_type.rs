use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::state::ObjectState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectType {
    pub id: u8,
    pub name: String,
    pub properties: BTreeMap<String, PropertyDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyDefinition {
    pub default_value: f64,
    pub min_value: f64,
    pub max_value: f64,
    pub integer_only: bool,
}

impl ObjectType {
    pub fn create_state(&self) -> Result<ObjectState, String> {
        self.validate()?;

        Ok(ObjectState {
            type_id: self.id,
            properties: self
                .properties
                .iter()
                .map(|(name, property)| (name.clone(), property.default_value))
                .collect(),
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Object type name must not be empty".to_owned());
        }

        for (name, property) in &self.properties {
            if name.trim().is_empty() {
                return Err("Property name must not be empty".to_owned());
            }

            if !property.min_value.is_finite()
                || !property.max_value.is_finite()
                || property.min_value > property.max_value
            {
                return Err(format!("Property '{name}' has invalid bounds"));
            }

            property
                .validate_value(property.default_value)
                .map_err(|error| format!("Property '{name}' has an invalid default: {error}"))?;
        }

        Ok(())
    }

    pub fn validate_state(&self, state: &ObjectState) -> Result<(), String> {
        self.validate()?;

        if state.type_id != self.id {
            return Err("Object state has a different type id".to_owned());
        }

        for name in state.properties.keys() {
            if !self.properties.contains_key(name) {
                return Err(format!("Unknown property '{name}'"));
            }
        }

        for (name, property) in &self.properties {
            let value = state
                .properties
                .get(name)
                .ok_or_else(|| format!("Missing property '{name}'"))?;

            property
                .validate_value(*value)
                .map_err(|error| format!("Property '{name}': {error}"))?;
        }

        Ok(())
    }
}

impl PropertyDefinition {
    fn validate_value(&self, value: f64) -> Result<(), &'static str> {
        if !value.is_finite() {
            return Err("value must be finite");
        }

        if value < self.min_value || value > self.max_value {
            return Err("value is outside the allowed range");
        }

        if self.integer_only && value.fract() != 0.0 {
            return Err("value must be an integer");
        }

        Ok(())
    }
}
