//! The Properties panel's variant sections (Figma's variants, public documentation:
//! https://help.figma.com/hc/en-us/articles/360056440594): a component set's properties (renamed
//! in place) and Add Variant; a variant's property values; an instance's properties, each a
//! dropdown of the set's values that switches it to that variant. Every change is a
//! `component.*` command.

use egui::Ui;
use logocraft_doc::{Node, NodeKind};
use serde_json::{Value, json};

use super::properties::COMPONENT_PURPLE;
use crate::LogocraftApp;
use crate::widgets::{self, section_header};

fn run(app: &mut LogocraftApp, cmd: &str, params: Value) -> Option<Value> {
    match app.run(cmd, params) {
        Ok(v) => Some(v),
        Err(e) => {
            app.status(e);
            None
        }
    }
}

/// The set `n` belongs to (as a set, a variant or an instance), from `component.variants`.
fn variants(app: &mut LogocraftApp, n: &Node) -> Option<Value> {
    let in_set = n.extras().component_set.is_some() || n.extras().variant.is_some() || matches!(n.kind, NodeKind::SymbolInstance { .. });
    if !in_set {
        return None;
    }
    app.session.execute("component.variants", &json!({ "id": n.id.0 })).ok()
}

/// A label before a field.
fn label(ui: &mut Ui, text: &str) {
    ui.add_sized([72.0, 22.0], egui::Label::new(egui::RichText::new(text).size(11.5)).truncate());
}

/// The variant sections for the selected `n`. → whether one drew.
pub(crate) fn section(app: &mut LogocraftApp, ui: &mut Ui, n: &Node) -> bool {
    let Some(v) = variants(app, n) else { return false };
    let props: Vec<(String, Vec<String>)> = v["properties"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let vals = p["values"].as_array()?.iter().filter_map(|x| x.as_str().map(str::to_string)).collect();
                    Some((p["name"].as_str()?.to_string(), vals))
                })
                .collect()
        })
        .unwrap_or_default();
    let set_name = v["name"].as_str().unwrap_or_default().to_string();
    let w = ui.available_width();
    if n.extras().component_set.is_some() {
        section_header(ui, tl!("Component set"));
        ui.label(egui::RichText::new(format!("❖ {set_name}")).size(12.5).color(COMPONENT_PURPLE).strong());
        for (name, _) in &props {
            ui.horizontal(|ui| {
                label(ui, tl!("Property"));
                if let Some(to) = widgets::text_field(ui, ("var-prop", name.as_str()), Some(name), ui.available_width() - 4.0, 1)
                    && !to.is_empty()
                {
                    run(app, "component.renameProperty", json!({ "id": n.id.0, "from": name, "to": to }));
                }
            });
        }
        ui.horizontal(|ui| {
            let half = (w - 6.0) / 2.0;
            if widgets::flat_button(ui, tl!("Add Variant"), half).clicked() {
                run(app, "component.addVariant", json!({ "id": n.id.0 }));
            }
            if widgets::flat_button(ui, tl!("Add Property"), half).clicked()
                && let Some(first) = v["variants"][0]["id"].as_u64()
            {
                let name = (1..).map(|i| format!("Property {i}")).find(|p| !props.iter().any(|(n, _)| n == p)).unwrap_or_default();
                run(app, "component.setVariant", json!({ "ids": [first], "props": { name: "Default" } }));
            }
        });
        return true;
    }
    if n.extras().variant.is_some() {
        section_header(ui, tl!("Variant"));
        let mine: Vec<(String, String)> = n.extras().variant.clone().unwrap_or_default();
        for (name, value) in &mine {
            ui.horizontal(|ui| {
                label(ui, name);
                if let Some(to) = widgets::text_field(ui, ("var-value", name.as_str()), Some(value), ui.available_width() - 4.0, 1)
                    && !to.is_empty()
                {
                    run(app, "component.setVariant", json!({ "props": { name.as_str(): to } }));
                }
            });
        }
        if widgets::flat_button(ui, tl!("Add Variant"), w).clicked() {
            run(app, "component.addVariant", json!({}));
        }
        return true;
    }
    // An instance of a variant: switch by property.
    let current: Vec<(String, String)> = v["variants"]
        .as_array()
        .and_then(|vs| {
            let base = logocraft_engine::cmd::components::base_of(n)?;
            vs.iter().find(|x| x["symbol"].as_str() == Some(base.as_str()))
        })
        .and_then(|x| x["props"].as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string())).collect()))
        .unwrap_or_default();
    section_header(ui, &format!("{} · {set_name}", tl!("Variant")));
    for (name, values) in &props {
        ui.horizontal(|ui| {
            label(ui, name);
            let now = current.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str()).unwrap_or("");
            let options: Vec<&str> = values.iter().map(String::as_str).collect();
            if let Some(i) = widgets::dropdown_names(ui, ("var-switch", name.as_str()), now, &options, ui.available_width() - 4.0)
                && let Some(to) = values.get(i)
                && to != now
            {
                run(app, "component.setInstanceVariant", json!({ "props": { name.as_str(): to } }));
            }
        });
    }
    true
}
