//! The Properties panel's component property sections, laid out like Figma's (public
//! documentation: https://help.figma.com/hc/en-us/articles/5579474826519), modelled on FigCraft's
//! component panel: a main component's properties (+ Boolean, Text, Instance swap; rename, default,
//! delete); for a layer inside a main, which property drives it; for an instance, its property
//! values and its layer overrides (fill through the colour picker, visibility, text), each with a
//! reset. Every change is a `component.*` / `instance.*` command.

use egui::{Sense, Ui, vec2};
use logocraft_color::{Color, Paint};
use logocraft_doc::{Node, NodeKind};
use serde_json::{Value, json};

use super::properties::COMPONENT_PURPLE;
use crate::LogocraftApp;
use crate::widgets::{self, section_header};

fn run(app: &mut LogocraftApp, cmd: &str, params: Value) {
    if let Err(e) = app.run(cmd, params) {
        app.status(e);
    }
}

/// `component.properties` for `n` (a main, a layer in one, or an instance).
fn describe(app: &mut LogocraftApp, n: &Node) -> Option<Value> {
    app.session.execute("component.properties", &json!({ "id": n.id.0 })).ok()
}

fn label(ui: &mut Ui, text: &str, w: f32) {
    ui.add_sized([w, 22.0], egui::Label::new(egui::RichText::new(text).size(11.5)).truncate());
}

/// The sections for the selected `n`. → whether one drew.
pub(crate) fn sections(app: &mut LogocraftApp, ui: &mut Ui, n: &Node) -> bool {
    let is_main = n.extras().component.is_some();
    let is_instance = matches!(n.kind, NodeKind::SymbolInstance { .. }) && logocraft_engine::cmd::components::base_of(n).is_some();
    let Some(info) = describe(app, n) else { return false };
    if is_main {
        main_props(app, ui, n, &info);
        return true;
    }
    if is_instance {
        instance_values(app, ui, &info);
        return true;
    }
    // A layer inside a main component.
    layer_binding(app, ui, n, &info)
}

fn props(info: &Value) -> Vec<Value> {
    info["props"].as_array().cloned().unwrap_or_default()
}

/// The component names (not the symbols made for overrides).
fn component_names(app: &LogocraftApp) -> Vec<String> {
    app.session
        .active()
        .map(|st| st.doc.symbols.iter().filter(|s| !logocraft_engine::cmd::components::is_derived(&s.name)).map(|s| s.name.clone()).collect())
        .unwrap_or_default()
}

fn type_glyph(t: &str) -> &'static str {
    match t {
        "boolean" => "◐",
        "text" => "T",
        _ => "◇",
    }
}

/// A main component's properties.
fn main_props(app: &mut LogocraftApp, ui: &mut Ui, n: &Node, info: &Value) {
    let id = n.id.0;
    ui.horizontal(|ui| {
        section_header(ui, tl!("Properties"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let resp = widgets::icon_button(ui, "plus", tl!("Add property"), false, 22.0);
            egui::Popup::menu(&resp).show(|ui| {
                let existing = props(info);
                let free = |base: &str| {
                    (1..)
                        .map(|i| if i == 1 { base.to_string() } else { format!("{base} {i}") })
                        .find(|c| !existing.iter().any(|p| p["name"] == c.as_str()))
                        .unwrap_or_default()
                };
                if ui.button(tl!("Boolean")).clicked() {
                    run(app, "component.addProperty", json!({ "id": id, "name": free("Show"), "type": "boolean" }));
                }
                if ui.button(tl!("Text")).clicked() {
                    run(app, "component.addProperty", json!({ "id": id, "name": free("Text"), "type": "text", "default": "Text" }));
                }
                let names = component_names(app);
                if let Some(first) = names.first()
                    && ui.button(tl!("Instance swap")).clicked()
                {
                    run(app, "component.addProperty", json!({ "id": id, "name": free("Instance"), "type": "instanceSwap", "default": first }));
                }
            });
        });
    });
    let list = props(info);
    if list.is_empty() {
        widgets::dim_label(ui, tl!("Add properties, then bind layers to them (select a layer inside)."));
    }
    for p in list {
        let name = p["name"].as_str().unwrap_or_default().to_string();
        let kind = p["type"].as_str().unwrap_or_default().to_string();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(egui::RichText::new(type_glyph(&kind)).color(COMPONENT_PURPLE));
            if let Some(to) = widgets::text_field(ui, ("cp-name", name.as_str()), Some(&name), 80.0, 1)
                && !to.is_empty()
            {
                run(app, "component.editProperty", json!({ "id": id, "name": name, "newName": to }));
            }
            match kind.as_str() {
                "boolean" => {
                    let mut v = p["default"].as_bool().unwrap_or(true);
                    if ui.checkbox(&mut v, "").on_hover_text(tl!("Default")).changed() {
                        run(app, "component.editProperty", json!({ "id": id, "name": name, "default": v }));
                    }
                }
                "text" => {
                    let cur = p["default"].as_str().unwrap_or_default().to_string();
                    if let Some(t) = widgets::text_field(ui, ("cp-default", name.as_str()), Some(&cur), 70.0, 1) {
                        run(app, "component.editProperty", json!({ "id": id, "name": name, "default": t }));
                    }
                }
                _ => {
                    let names = component_names(app);
                    let cur = p["default"].as_str().unwrap_or_default().to_string();
                    let opts: Vec<&str> = names.iter().map(String::as_str).collect();
                    if let Some(i) = widgets::dropdown_names(ui, ("cp-swap", name.as_str()), &cur, &opts, 80.0)
                        && let Some(to) = names.get(i)
                    {
                        run(app, "component.editProperty", json!({ "id": id, "name": name, "default": to }));
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, "minus", tl!("Delete property"), false, 20.0).clicked() {
                    run(app, "component.deleteProperty", json!({ "id": id, "name": name }));
                }
            });
        });
    }
}

/// For a layer inside a main component: which property drives its visibility, text or component.
fn layer_binding(app: &mut LogocraftApp, ui: &mut Ui, n: &Node, info: &Value) -> bool {
    let all = props(info);
    let refs = n.extras().prop_refs.clone().unwrap_or_default();
    let mut rows: Vec<(&str, &str, &str)> = vec![("visible", "boolean", "Visibility")];
    match &n.kind {
        NodeKind::Text(_) => rows.push(("characters", "text", "Text")),
        NodeKind::SymbolInstance { .. } => rows.push(("mainComponent", "instanceSwap", "Instance")),
        _ => {}
    }
    section_header(ui, tl!("Component property"));
    for (field, kind, title) in rows {
        let options: Vec<String> = all.iter().filter(|p| p["type"] == kind).filter_map(|p| p["name"].as_str().map(str::to_string)).collect();
        let current = refs.get(field).cloned();
        ui.horizontal(|ui| {
            label(ui, tl!(title), 64.0);
            let none = tl!("None").to_string();
            let create = format!("{}…", tl!("Create property"));
            let mut labels: Vec<String> = vec![none.clone()];
            labels.extend(options.iter().cloned());
            labels.push(create.clone());
            let shown = current.clone().unwrap_or(none.clone());
            let refs_: Vec<&str> = labels.iter().map(String::as_str).collect();
            if let Some(i) = widgets::dropdown_names(ui, ("cp-bind", field), &shown, &refs_, 130.0) {
                match labels.get(i) {
                    Some(l) if *l == none => run(app, "component.bindProperty", json!({ "layer": n.id.0, "property": null, "field": field })),
                    Some(l) if *l == create => {
                        let base = match kind {
                            "text" => "Text",
                            "instanceSwap" => "Instance",
                            _ => "Show",
                        };
                        let name = (1..)
                            .map(|k| if k == 1 { base.to_string() } else { format!("{base} {k}") })
                            .find(|c| !all.iter().any(|p| p["name"] == c.as_str()))
                            .unwrap_or_default();
                        run(app, "component.addProperty", json!({ "name": name, "type": kind, "layer": n.id.0 }));
                    }
                    Some(l) => run(app, "component.bindProperty", json!({ "layer": n.id.0, "property": l, "field": field })),
                    None => {}
                }
            }
        });
    }
    true
}

/// An instance's property values, then its layers with their overrides.
fn instance_values(app: &mut LogocraftApp, ui: &mut Ui, info: &Value) {
    let list = props(info);
    if !list.is_empty() {
        section_header(ui, tl!("Properties"));
    }
    for p in &list {
        let name = p["name"].as_str().unwrap_or_default().to_string();
        ui.horizontal(|ui| {
            label(ui, &name, 80.0);
            match p["type"].as_str() {
                Some("boolean") => {
                    let mut v = p["value"].as_bool().unwrap_or(true);
                    if ui.checkbox(&mut v, "").changed() {
                        run(app, "instance.setProperties", json!({ "properties": { name.as_str(): v } }));
                    }
                }
                Some("text") => {
                    let cur = p["value"].as_str().unwrap_or_default().to_string();
                    if let Some(t) = widgets::text_field(ui, ("ip-text", name.as_str()), Some(&cur), ui.available_width() - 4.0, 1) {
                        run(app, "instance.setProperties", json!({ "properties": { name.as_str(): t } }));
                    }
                }
                _ => {
                    let names = component_names(app);
                    let cur = p["value"].as_str().unwrap_or_default().to_string();
                    let opts: Vec<&str> = names.iter().map(String::as_str).collect();
                    if let Some(i) = widgets::dropdown_names(ui, ("ip-swap", name.as_str()), &cur, &opts, ui.available_width() - 4.0)
                        && let Some(to) = names.get(i)
                    {
                        run(app, "instance.setProperties", json!({ "properties": { name.as_str(): to } }));
                    }
                }
            }
        });
    }
    let layers = info["layers"].as_array().cloned().unwrap_or_default();
    if layers.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        section_header(ui, tl!("Layers"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::flat_button(ui, tl!("Reset all"), 70.0).on_hover_text(tl!("Back to the main component's look")).clicked() {
                run(app, "component.resetOverrides", json!({}));
            }
        });
    });
    for l in layers {
        layer_row(app, ui, &l);
    }
}

/// One layer of an instance: its name (purple when overridden), fill chip, visibility and text.
fn layer_row(app: &mut LogocraftApp, ui: &mut Ui, l: &Value) {
    let layer = l["id"].as_u64().unwrap_or_default();
    let o = &l["override"];
    let overridden = o.is_object();
    // The overridden fill, else the main's.
    let fill = o
        .get("fill")
        .and_then(|f| serde_json::from_value::<Paint>(f.clone()).ok())
        .and_then(|p| p.color())
        .or_else(|| l["fill"].as_str().and_then(Color::from_hex));
    let visible = o["visible"].as_bool().unwrap_or(l["visible"].as_bool().unwrap_or(true));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let eye = if visible { "eye" } else { "eye-off" };
        if widgets::icon_button(ui, eye, tl!("Show or hide in this instance"), false, 20.0).clicked() {
            run(app, "instance.setOverride", json!({ "layer": layer, "visible": !visible }));
        }
        if let Some(c) = fill {
            let (rect, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
            widgets::swatch_tile(ui, rect, &Paint::solid(c), false, resp.hovered());
            let resp = resp.on_hover_text(tl!("Fill in this instance"));
            crate::widgets::popover(&resp, resp.clicked(), |ui| {
                ui.set_width(super::fill_picker::WIDTH);
                let e = super::fill_picker::color_controls(ui, &format!("ov-{layer}"), c, None);
                if let Some((nc, phase)) = e.color
                    && phase != widgets::Live::Dragging
                {
                    run(app, "instance.setOverride", json!({ "layer": layer, "fill": super::color_json(&nc) }));
                }
            });
        }
        let name = l["name"].as_str().unwrap_or_default();
        let text = egui::RichText::new(name).size(11.5);
        ui.label(if overridden { text.color(COMPONENT_PURPLE).strong() } else { text });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if overridden && widgets::icon_button(ui, "x", tl!("Reset this layer"), false, 18.0).clicked() {
                run(app, "component.resetOverrides", json!({ "layer": layer }));
            }
        });
    });
    if let Some(chars) = l["characters"].as_str() {
        let cur = o["characters"].as_str().unwrap_or(chars).to_string();
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            if let Some(t) = widgets::text_field(ui, ("ov-text", layer), Some(&cur), ui.available_width() - 4.0, 1) {
                run(app, "instance.setOverride", json!({ "layer": layer, "characters": t }));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mains_layers_and_instances_show_their_sections() {
        use super::super::tests_appearance::run as exec;
        let mut app = LogocraftApp::new(logocraft_engine::Session::new(), Default::default());
        exec(&mut app, "file.new", json!({"width": 300, "height": 300}));
        let bg = exec(&mut app, "shape.rectangle", json!({"x": 10, "y": 10, "width": 80, "height": 30}))["id"].clone();
        let label = exec(&mut app, "text.create", json!({"x": 20, "y": 30, "text": "Go"}))["id"].clone();
        exec(&mut app, "select.set", json!({"ids": [bg, label]}));
        let main = exec(&mut app, "component.create", json!({"name": "Button"}))["id"].clone();
        exec(&mut app, "component.addProperty", json!({"id": main, "name": "Label", "layer": label}));
        let text = crate::tests_labels::painted_text(&mut app, super::super::properties::show);
        assert!(text.lines().any(|l| l == "Properties") && text.contains("Label"), "{text}");
        // A layer inside: which property drives it.
        exec(&mut app, "select.set", json!({"ids": [label]}));
        let text = crate::tests_labels::painted_text(&mut app, super::super::properties::show);
        assert!(text.contains("Component property") && text.contains("Visibility"), "{text}");
        // An instance: its values and layers.
        let inst = exec(&mut app, "symbol.place", json!({"name": "Button", "x": 200, "y": 200}))["id"].clone();
        exec(&mut app, "select.set", json!({"ids": [inst]}));
        let text = crate::tests_labels::painted_text(&mut app, super::super::properties::show);
        assert!(text.contains("Layers") && text.contains("Reset all"), "{text}");
    }
}
