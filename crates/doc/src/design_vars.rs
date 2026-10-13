//! Design variables, as Figma has them (public documentation:
//! https://help.figma.com/hc/en-us/articles/15339657135383), modelled on FigCraft's
//! (storytold/figcraft, MIT OR Apache-2.0): collections of colour, number, text and boolean
//! variables with a value per mode (Light, Dark…), aliases to other variables, and layers whose
//! properties are bound to them. A container picks a mode per collection for everything inside it
//! (unset: its parent's, else the collection's first mode). (Not Illustrator's data-driven
//! [`crate::variables`].)

use std::collections::BTreeMap;

use logocraft_color::Color;
use serde::{Deserialize, Serialize};

pub type VariableId = String;
pub type CollectionId = String;
pub type ModeId = String;

/// What a variable holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VariableType {
    Color,
    Number,
    String,
    Boolean,
}

impl VariableType {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "color" | "colour" => Some(VariableType::Color),
            "number" | "float" => Some(VariableType::Number),
            "string" | "text" => Some(VariableType::String),
            "boolean" | "bool" => Some(VariableType::Boolean),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            VariableType::Color => "color",
            VariableType::Number => "number",
            VariableType::String => "string",
            VariableType::Boolean => "boolean",
        }
    }
    /// The value a new variable of this type starts with.
    pub fn initial(self) -> VariableValue {
        match self {
            VariableType::Color => VariableValue::Color(Color::rgb8(0xd9, 0xd9, 0xd9)),
            VariableType::Number => VariableValue::Number(0.0),
            VariableType::String => VariableValue::String(String::new()),
            VariableType::Boolean => VariableValue::Boolean(true),
        }
    }
}

/// A variable's value in one mode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum VariableValue {
    Color(Color),
    Number(f64),
    String(String),
    Boolean(bool),
    /// The value of another variable (of the same type).
    Alias(VariableId),
}

impl VariableValue {
    /// The type of a plain value (an alias has its target's).
    pub fn kind(&self) -> Option<VariableType> {
        match self {
            VariableValue::Color(_) => Some(VariableType::Color),
            VariableValue::Number(_) => Some(VariableType::Number),
            VariableValue::String(_) => Some(VariableType::String),
            VariableValue::Boolean(_) => Some(VariableType::Boolean),
            VariableValue::Alias(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mode {
    pub id: ModeId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub id: CollectionId,
    pub name: String,
    /// The first is the default.
    pub modes: Vec<Mode>,
}

impl Collection {
    pub fn default_mode(&self) -> Option<&ModeId> {
        self.modes.first().map(|m| &m.id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Variable {
    pub id: VariableId,
    pub name: String,
    pub collection: CollectionId,
    #[serde(rename = "type")]
    pub kind: VariableType,
    /// Value per mode id.
    pub values: BTreeMap<ModeId, VariableValue>,
}

/// The document's variables.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DesignVariables {
    #[serde(default)]
    pub collections: Vec<Collection>,
    #[serde(default)]
    pub variables: Vec<Variable>,
}

impl DesignVariables {
    pub fn is_empty(&self) -> bool {
        self.collections.is_empty() && self.variables.is_empty()
    }
    pub fn get(&self, id: &str) -> Option<&Variable> {
        self.variables.iter().find(|v| v.id == id)
    }
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Variable> {
        self.variables.iter_mut().find(|v| v.id == id)
    }
    pub fn collection(&self, id: &str) -> Option<&Collection> {
        self.collections.iter().find(|c| c.id == id)
    }
    /// A variable by id or by name.
    pub fn find(&self, key: &str) -> Option<&Variable> {
        self.get(key).or_else(|| self.variables.iter().find(|v| v.name == key))
    }
    /// A collection by id or by name.
    pub fn find_collection(&self, key: &str) -> Option<&Collection> {
        self.collection(key).or_else(|| self.collections.iter().find(|c| c.name == key))
    }
    /// The value of variable `id` in `modes` (collection → mode; missing: the collection's
    /// default), following aliases (each in the mode of its own collection), at most 32 deep.
    pub fn resolve(&self, id: &str, modes: &BTreeMap<CollectionId, ModeId>) -> Option<VariableValue> {
        let mut cur = id.to_string();
        for _ in 0..32 {
            let var = self.get(&cur)?;
            let coll = self.collection(&var.collection)?;
            let mode = modes.get(&coll.id).filter(|m| var.values.contains_key(*m)).or_else(|| coll.default_mode())?;
            match var.values.get(mode).or_else(|| var.values.values().next())? {
                VariableValue::Alias(next) => cur = next.clone(),
                v => return Some(v.clone()),
            }
        }
        None
    }
    /// Would aliasing `from` to `to` make a loop?
    pub fn would_loop(&self, from: &str, to: &str) -> bool {
        let mut seen = vec![from.to_string()];
        let mut stack = vec![to.to_string()];
        while let Some(cur) = stack.pop() {
            if seen.contains(&cur) {
                return true;
            }
            seen.push(cur.clone());
            if let Some(v) = self.get(&cur) {
                for val in v.values.values() {
                    if let VariableValue::Alias(next) = val {
                        stack.push(next.clone());
                    }
                }
            }
            if seen.len() > 256 {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn themed() -> DesignVariables {
        let modes = vec![Mode { id: "c/light".into(), name: "Light".into() }, Mode { id: "c/dark".into(), name: "Dark".into() }];
        let coll = Collection { id: "c".into(), name: "Theme".into(), modes };
        let bg = Variable {
            id: "v1".into(),
            name: "bg".into(),
            collection: "c".into(),
            kind: VariableType::Color,
            values: [("c/light".to_string(), VariableValue::Color(Color::WHITE)), ("c/dark".to_string(), VariableValue::Color(Color::BLACK))].into(),
        };
        let surface = Variable {
            id: "v2".into(),
            name: "surface".into(),
            collection: "c".into(),
            kind: VariableType::Color,
            values: [("c/light".to_string(), VariableValue::Alias("v1".into()))].into(),
        };
        DesignVariables { collections: vec![coll], variables: vec![bg, surface] }
    }

    #[test]
    fn values_resolve_per_mode_through_aliases() {
        let v = themed();
        assert_eq!(v.resolve("v1", &BTreeMap::new()), Some(VariableValue::Color(Color::WHITE)), "default mode");
        let dark: BTreeMap<String, String> = [("c".to_string(), "c/dark".to_string())].into();
        assert_eq!(v.resolve("v1", &dark), Some(VariableValue::Color(Color::BLACK)));
        // The alias has no Dark value: its first value (the alias) is used, resolved in Dark.
        assert_eq!(v.resolve("v2", &dark), Some(VariableValue::Color(Color::BLACK)));
        assert!(v.would_loop("v1", "v2"), "v2 → v1 already");
        assert!(!v.would_loop("v2", "v1"));
        assert_eq!(v.find("surface").map(|x| x.id.as_str()), Some("v2"));
    }
}
