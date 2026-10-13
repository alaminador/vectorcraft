//! Design variables in the UI, laid out like Figma's (public documentation:
//! https://help.figma.com/hc/en-us/articles/15339657135383), modelled on FigCraft's variables
//! panel and variable picker: the Local variables window (collections on the left; a table of
//! variables with a column per mode; + variable, + mode, rename, delete), a bind button beside
//! bindable properties (a variable picker; once bound, the variable's name with detach), and the
//! Variable modes rows of a frame or an instance. Every change is a `variable.*` command.

use egui::{Sense, Ui, vec2};
use logocraft_color::{Color, Paint};
use logocraft_doc::Node;
use serde_json::{Value, json};

use super::{pstate, set_pstate};
use crate::LogocraftApp;
use crate::widgets::{self, section_header};

/// Panel state: the window's selected collection.
const COLLECTION: &str = "variables-collection";

fn run(app: &mut LogocraftApp, cmd: &str, params: Value) {
    if let Err(e) = app.run(cmd, params) {
        app.status(e);
    }
}

fn list(app: &mut LogocraftApp) -> Value {
    app.session.execute("variable.list", &json!({})).unwrap_or(Value::Null)
}

fn arr(v: &Value, k: &str) -> Vec<Value> {
    v[k].as_array().cloned().unwrap_or_default()
}

/// The colour a value cell holds.
fn color_of(v: &Value) -> Option<Color> {
    match v {
        Value::String(s) => Color::from_hex(s),
        Value::Object(_) if v.get("alias").is_none() => logocraft_engine::cmd::color_value(v),
        _ => None,
    }
}

/// The Local variables window (drawn every frame; shows while open).
pub(crate) fn window(app: &mut LogocraftApp, ctx: &egui::Context) {
    if !app.ui.variables_open || app.session.active().is_none() {
        return;
    }
    let mut open = true;
    egui::Window::new(tl!("Local variables")).open(&mut open).default_size(vec2(720.0, 420.0)).collapsible(false).show(ctx, |ui| body(app, ui));
    if !open {
        app.ui.variables_open = false;
    }
}

fn body(app: &mut LogocraftApp, ui: &mut Ui) {
    let data = list(app);
    let collections = arr(&data, "collections");
    let ctx = ui.ctx().clone();
    let mut current: String = pstate(&ctx, COLLECTION);
    if !collections.iter().any(|c| c["id"] == current.as_str()) {
        current = collections.first().and_then(|c| c["id"].as_str()).unwrap_or_default().to_string();
    }
    ui.horizontal_top(|ui| {
        // Collections.
        ui.vertical(|ui| {
            ui.set_width(170.0);
            ui.horizontal(|ui| {
                section_header(ui, tl!("Collections"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, "plus", tl!("New collection"), false, 20.0).clicked() {
                        let n = collections.len() + 1;
                        run(app, "variable.createCollection", json!({ "name": format!("Collection {n}") }));
                    }
                });
            });
            for c in &collections {
                let id = c["id"].as_str().unwrap_or_default();
                let name = c["name"].as_str().unwrap_or_default();
                if ui.selectable_label(id == current, name).clicked() {
                    set_pstate(&ctx, COLLECTION, id.to_string());
                }
            }
            if collections.is_empty() {
                widgets::dim_label(ui, tl!("No variables yet: make a collection."));
            }
        });
        ui.separator();
        let Some(coll) = collections.iter().find(|c| c["id"] == current.as_str()).cloned() else { return };
        ui.vertical(|ui| table(app, ui, &data, &coll));
    });
}

/// The variables of collection `coll`: name, a value per mode.
fn table(app: &mut LogocraftApp, ui: &mut Ui, data: &Value, coll: &Value) {
    let cid = coll["id"].as_str().unwrap_or_default().to_string();
    let modes = arr(coll, "modes");
    ui.horizontal(|ui| {
        if let Some(n) = widgets::text_field(ui, ("var-coll-name", cid.as_str()), coll["name"].as_str(), 150.0, 1)
            && !n.is_empty()
        {
            run(app, "variable.renameCollection", json!({ "collection": cid, "name": n }));
        }
        let resp = widgets::flat_button(ui, &format!("+ {}", tl!("Variable")), 90.0);
        egui::Popup::menu(&resp).show(|ui| {
            for (label, kind) in [("Color", "color"), ("Number", "number"), ("String", "string"), ("Boolean", "boolean")] {
                if ui.button(tl!(label)).clicked() {
                    let n = arr(data, "variables").iter().filter(|v| v["collection"] == cid.as_str()).count() + 1;
                    run(app, "variable.create", json!({ "collection": cid, "name": format!("{kind} {n}"), "type": kind }));
                }
            }
        });
        if widgets::flat_button(ui, &format!("+ {}", tl!("Mode")), 70.0).clicked() {
            run(app, "variable.addMode", json!({ "collection": cid, "name": format!("Mode {}", modes.len() + 1) }));
        }
        if widgets::icon_button(ui, "trash-2", tl!("Delete collection"), false, 20.0).clicked() {
            run(app, "variable.deleteCollection", json!({ "collection": cid }));
        }
    });
    ui.add_space(4.0);
    egui::ScrollArea::both().id_salt(("var-table", cid.as_str())).show(ui, |ui| {
        egui::Grid::new(("var-grid", cid.as_str())).striped(true).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
            ui.label(egui::RichText::new(tl!("Name")).strong());
            for m in &modes {
                let mid = m["id"].as_str().unwrap_or_default().to_string();
                ui.horizontal(|ui| {
                    if let Some(n) = widgets::text_field(ui, ("var-mode", mid.as_str()), m["name"].as_str(), 90.0, 1)
                        && !n.is_empty()
                    {
                        run(app, "variable.renameMode", json!({ "collection": cid, "mode": mid, "name": n }));
                    }
                    if modes.len() > 1 && widgets::icon_button(ui, "x", tl!("Remove mode"), false, 16.0).clicked() {
                        run(app, "variable.removeMode", json!({ "collection": cid, "mode": mid }));
                    }
                });
            }
            ui.label("");
            ui.end_row();
            for v in arr(data, "variables").into_iter().filter(|v| v["collection"] == cid.as_str()) {
                row(app, ui, &v, &modes);
                ui.end_row();
            }
        });
    });
}

fn type_glyph(t: &str) -> &'static str {
    match t {
        "color" => "◉",
        "number" => "#",
        "string" => "T",
        _ => "◐",
    }
}

/// One variable: its name and a value cell per mode.
fn row(app: &mut LogocraftApp, ui: &mut Ui, v: &Value, modes: &[Value]) {
    let id = v["id"].as_str().unwrap_or_default().to_string();
    let kind = v["type"].as_str().unwrap_or_default().to_string();
    ui.horizontal(|ui| {
        ui.label(type_glyph(&kind));
        if let Some(n) = widgets::text_field(ui, ("var-name", id.as_str()), v["name"].as_str(), 120.0, 1)
            && !n.is_empty()
        {
            run(app, "variable.rename", json!({ "variable": id, "name": n }));
        }
    });
    for m in modes {
        let mid = m["id"].as_str().unwrap_or_default().to_string();
        let val = v["values"][mid.as_str()].clone();
        ui.horizontal(|ui| cell(app, ui, &id, &mid, &kind, &val));
    }
    if widgets::icon_button(ui, "minus", tl!("Delete variable"), false, 18.0).clicked() {
        run(app, "variable.delete", json!({ "variable": id }));
    }
}

/// A value: a colour chip (the colour picker), a number, text or a checkbox; an alias shows its
/// target with detach.
fn cell(app: &mut LogocraftApp, ui: &mut Ui, id: &str, mode: &str, kind: &str, val: &Value) {
    if let Some(target) = val.get("alias").and_then(Value::as_str) {
        ui.label(egui::RichText::new(format!("→ {target}")).color(super::properties::COMPONENT_PURPLE));
        return;
    }
    match kind {
        "color" => {
            let c = color_of(val).unwrap_or(Color::BLACK);
            let (rect, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
            widgets::swatch_tile(ui, rect, &Paint::solid(c), false, resp.hovered());
            crate::widgets::popover(&resp, resp.clicked(), |ui| {
                ui.set_width(super::fill_picker::WIDTH);
                let e = super::fill_picker::color_controls(ui, &format!("var-{id}-{mode}"), c, None);
                if let Some((nc, phase)) = e.color
                    && phase != widgets::Live::Dragging
                {
                    run(app, "variable.set", json!({ "variable": id, "mode": mode, "value": super::color_json(&nc) }));
                }
            });
            ui.label(egui::RichText::new(c.to_hex().trim_start_matches('#').to_ascii_uppercase()).size(11.5));
        }
        "number" => {
            if let Some(x) = widgets::plain_field(ui, ("var-num", id, mode), val.as_f64().unwrap_or(0.0), "", 2, 70.0) {
                run(app, "variable.set", json!({ "variable": id, "mode": mode, "value": x }));
            }
        }
        "string" => {
            if let Some(t) = widgets::text_field(ui, ("var-str", id, mode), val.as_str(), 110.0, 1) {
                run(app, "variable.set", json!({ "variable": id, "mode": mode, "value": t }));
            }
        }
        _ => {
            let mut b = val.as_bool().unwrap_or(true);
            if ui.checkbox(&mut b, "").changed() {
                run(app, "variable.set", json!({ "variable": id, "mode": mode, "value": b }));
            }
        }
    }
}

/// The variable type a field binds.
fn field_kind(field: &str) -> &'static str {
    logocraft_engine::cmd::designvars::FIELDS.iter().find(|(f, _)| *f == field).map(|(_, k)| k.name()).unwrap_or("number")
}

/// Beside a bindable property of the selected `n`: the bound variable's name (click: detach),
/// else a button opening the variable picker.
pub(crate) fn bind_button(app: &mut LogocraftApp, ui: &mut Ui, n: &Node, field: &str) {
    let data = list(app);
    let vars = arr(&data, "variables");
    let kind = field_kind(field);
    let bound = n.extras().bound.as_ref().and_then(|b| b.get(field)).cloned();
    if let Some(vid) = bound {
        let name = vars.iter().find(|v| v["id"] == vid.as_str()).and_then(|v| v["name"].as_str()).unwrap_or("?").to_string();
        let chip = ui.add(egui::Button::new(egui::RichText::new(format!("⬡ {name}")).size(11.0).color(super::properties::COMPONENT_PURPLE)).small());
        if chip.on_hover_text(tl!("Bound to a variable: click to detach (the value stays)")).clicked() {
            run(app, "variable.bind", json!({ "field": field, "variable": null }));
        }
        return;
    }
    let resp = ui.add(egui::Button::new(egui::RichText::new("⬡").size(12.0)).small()).on_hover_text(tl!("Apply variable"));
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(180.0);
        let fitting: Vec<&Value> = vars.iter().filter(|v| v["type"] == kind || (field == "characters" && v["type"] == "number")).collect();
        if fitting.is_empty() {
            widgets::dim_label(ui, tl!("No variables of this type yet."));
        }
        for c in arr(&data, "collections") {
            let in_coll: Vec<&&Value> = fitting.iter().filter(|v| v["collection"] == c["id"]).collect();
            if in_coll.is_empty() {
                continue;
            }
            ui.label(egui::RichText::new(c["name"].as_str().unwrap_or_default()).size(11.0).weak());
            for v in in_coll {
                let label = v["name"].as_str().unwrap_or_default();
                if ui.button(label).clicked() {
                    run(app, "variable.bind", json!({ "field": field, "variable": v["id"] }));
                }
            }
        }
        ui.separator();
        if ui.button(tl!("Manage variables…")).clicked() {
            app.ui.variables_open = true;
        }
    });
}

/// The Variable modes rows of a selected container or instance: Auto (the parent's) or a mode per
/// collection that has more than one. → whether it drew.
pub(crate) fn mode_section(app: &mut LogocraftApp, ui: &mut Ui, n: &Node) -> bool {
    let container = n.is_container() || matches!(n.kind, logocraft_doc::NodeKind::SymbolInstance { .. });
    if !container {
        return false;
    }
    let data = list(app);
    let colls: Vec<Value> = arr(&data, "collections").into_iter().filter(|c| arr(c, "modes").len() > 1).collect();
    if colls.is_empty() {
        return false;
    }
    section_header(ui, tl!("Variable modes"));
    let own = n.extras().var_modes.clone().unwrap_or_default();
    for c in colls {
        let cid = c["id"].as_str().unwrap_or_default().to_string();
        let modes = arr(&c, "modes");
        let names: Vec<String> = modes.iter().filter_map(|m| m["name"].as_str().map(str::to_string)).collect();
        let auto = tl!("Auto").to_string();
        let mut options = vec![auto.clone()];
        options.extend(names.iter().cloned());
        let current = own
            .get(&cid)
            .and_then(|mid| modes.iter().find(|m| m["id"] == mid.as_str()))
            .and_then(|m| m["name"].as_str())
            .map(str::to_string)
            .unwrap_or(auto.clone());
        ui.horizontal(|ui| {
            ui.add_sized([90.0, 22.0], egui::Label::new(egui::RichText::new(c["name"].as_str().unwrap_or_default()).size(11.5)).truncate());
            let opts: Vec<&str> = options.iter().map(String::as_str).collect();
            if let Some(i) = widgets::dropdown_names(ui, ("var-mode-pick", cid.as_str()), &current, &opts, 120.0) {
                let mode = if i == 0 { Value::Null } else { json!(options[i]) };
                run(app, "variable.setMode", json!({ "collection": cid, "mode": mode }));
            }
        });
    }
    if widgets::flat_button(ui, tl!("Manage variables…"), 140.0).clicked() {
        app.ui.variables_open = true;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_lists_variables_and_modes() {
        use super::super::tests_appearance::{app_with_rect, run as exec};
        let mut app = app_with_rect();
        exec(&mut app, "variable.createCollection", json!({"name": "Theme", "modes": ["Light", "Dark"]}));
        exec(&mut app, "variable.create", json!({"collection": "Theme", "name": "bg", "type": "color", "value": "#ffffff"}));
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        app.ui.variables_open = true;
        let mut texts = String::new();
        for _ in 0..2 {
            let mut out = ctx.run_ui(Default::default(), |ui| window(&mut app, ui.ctx()));
            out.textures_delta.clear();
            texts.clear();
            for c in &out.shapes {
                collect(&c.shape, &mut texts);
            }
        }
        assert!(texts.contains("Local variables") && texts.contains("Theme"), "{texts}");
        fn collect(s: &egui::Shape, out: &mut String) {
            match s {
                egui::Shape::Text(t) => {
                    out.push_str(t.galley.text());
                    out.push('\n');
                }
                egui::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
                _ => {}
            }
        }
    }
}
