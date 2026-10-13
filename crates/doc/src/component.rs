//! Component properties and instance overrides, as Figma has them (public documentation:
//! https://help.figma.com/hc/en-us/articles/5579474826519), modelled on FigCraft's
//! (storytold/figcraft, MIT OR Apache-2.0): a main component defines properties (Boolean, Text,
//! Instance swap) bound to its inner layers; each instance keeps its own values and per-layer
//! overrides (fill, stroke, visibility, text), which survive edits of the main.

use std::collections::BTreeMap;

use logocraft_color::Paint;
use serde::{Deserialize, Serialize};

/// A component property's definition on the main component.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PropDef {
    /// Shows or hides the layers bound to it (`visible`).
    Boolean { default: bool },
    /// The content of the type layers bound to it (`characters`).
    Text { default: String },
    /// The component a nested instance bound to it shows (`mainComponent`; a symbol name).
    InstanceSwap { default: String },
}

impl PropDef {
    /// The layer field the property drives.
    pub fn field(&self) -> &'static str {
        match self {
            PropDef::Boolean { .. } => "visible",
            PropDef::Text { .. } => "characters",
            PropDef::InstanceSwap { .. } => "mainComponent",
        }
    }
    /// Its type name (`boolean`, `text`, `instanceSwap`).
    pub fn type_name(&self) -> &'static str {
        match self {
            PropDef::Boolean { .. } => "boolean",
            PropDef::Text { .. } => "text",
            PropDef::InstanceSwap { .. } => "instanceSwap",
        }
    }
    /// The default value, as JSON.
    pub fn default_value(&self) -> serde_json::Value {
        match self {
            PropDef::Boolean { default } => serde_json::Value::Bool(*default),
            PropDef::Text { default } | PropDef::InstanceSwap { default } => serde_json::Value::String(default.clone()),
        }
    }
}

/// One layer's overrides in an instance (unset: the main's).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LayerOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Paint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Paint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub characters: Option<String>,
}

impl LayerOverride {
    pub fn is_empty(&self) -> bool {
        *self == LayerOverride::default()
    }
}

/// An instance's link to its component and what it changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InstanceData {
    /// The component (its symbol name) the instance is of; the instance may show a symbol of its
    /// own, made from the component with its values and overrides.
    pub component: String,
    /// Property values (name → value), where they differ from the defaults.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<String, serde_json::Value>,
    /// Per-layer overrides, by the id of the main component's layer.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<u64, LayerOverride>,
}
