//! The Properties panel's Effects section, laid out like Figma's (public documentation:
//! https://help.figma.com/hc/en-us/articles/360041488473): + adds an effect; each row has its eye,
//! its type (Drop shadow, Inner shadow, Layer blur, Background blur, Noise, Texture) and its
//! settings (offset, blur, spread, colour and opacity through the app's colour picker, or the
//! effect's own), and − removes it. The type menu also lists every other effect of the app, grouped
//! by its menu (Blur, Distort & Transform, Stylize, Artistic…); those keep their dialog behind the gear. Every change is an `effect.*` command on the object's own effects.

use egui::{Sense, Ui, vec2};
use logocraft_color::{Color, Paint};
use logocraft_doc::Effect;
use serde_json::{Value, json};

use super::{live_run, pstate, set_pstate};
use crate::LogocraftApp;
use crate::widgets::{self, Live, section_header};

/// The Figma effect types: label, effect id, Figma's defaults.
pub(crate) fn kinds() -> [(&'static str, &'static str, Value); 6] {
    [
        (
            "Drop shadow",
            "stylize.dropShadow",
            json!({"mode": "normal", "opacity": 25.0, "x": 0.0, "y": 4.0, "blur": 4.0, "spread": 0.0, "color": "#000000"}),
        ),
        (
            "Inner shadow",
            "stylize.innerShadow",
            json!({"mode": "normal", "opacity": 25.0, "x": 0.0, "y": 4.0, "blur": 4.0, "spread": 0.0, "color": "#000000"}),
        ),
        ("Layer blur", "blur.gaussian", json!({"radius": 4.0})),
        ("Background blur", "blur.background", json!({"radius": 4.0})),
        ("Noise", "stylize.noise", json!({"type": "mono", "size": 0.5, "density": 50.0, "opacity": 25.0, "color": "#000000"})),
        ("Texture", "stylize.texture", json!({"size": 2.0, "radius": 2.0, "clip": true})),
    ]
}

/// The Figma type of effect `id`, if it is one.
fn kind_of(id: &str) -> Option<usize> {
    kinds().iter().position(|(_, k, _)| *k == id)
}

/// Effect groups: (group, [(label, id)]).
type Groups = Vec<(String, Vec<(String, &'static str)>)>;

/// Every other effect of the catalog, grouped by its menu (its last level: Blur, Distort &
/// Transform, Stylize, Artistic…) in catalog order: (group, [(label, id)]). Built once.
fn catalog_groups() -> &'static Groups {
    static GROUPS: std::sync::OnceLock<Groups> = std::sync::OnceLock::new();
    GROUPS.get_or_init(|| {
        let mut out: Groups = vec![];
        for e in logocraft_effects::effect_catalog() {
            if kind_of(e.id).is_some() {
                continue;
            }
            // A top-level effect (Crop Marks) goes under Other.
            let group = if e.menu.len() > 1 { e.menu.last().copied().unwrap_or("Other") } else { "Other" }.to_string();
            let label = e.label.trim_end_matches('…').to_string();
            match out.iter_mut().find(|(g, _)| *g == group) {
                Some((_, items)) => items.push((label, e.id)),
                None => out.push((group, vec![(label, e.id)])),
            }
        }
        out
    })
}

/// The type menu of an effect row: Figma's six types, then every other effect by group. →
/// the effect chosen, with the params it starts from (Figma's defaults, else the effect's).
fn type_menu(ui: &mut Ui, i: usize, current: &str) -> Option<(&'static str, Value)> {
    let mut chosen = None;
    let resp = ui.add_sized([128.0, 22.0], egui::Button::new(egui::RichText::new(format!("{current}  \u{25BE}")).size(12.0)).truncate());
    egui::Popup::menu(&resp).id(egui::Id::new(("fx-type-menu", i))).show(|ui| {
        ui.set_min_width(170.0);
        for (label, id, defaults) in kinds() {
            if ui.button(tl!(label)).clicked() {
                chosen = Some((id, defaults));
            }
        }
        ui.separator();
        for (group, items) in catalog_groups() {
            ui.menu_button(group.as_str(), |ui| {
                ui.set_min_width(180.0);
                for (label, id) in items {
                    if ui.button(label.as_str()).clicked() {
                        chosen = Some((*id, logocraft_effects::default_params(id).unwrap_or_else(|| json!({}))));
                    }
                }
            });
        }
    });
    chosen
}

/// Panel state: the effect rows whose settings are open (by index).
const OPEN: &str = "effects-open";

fn run(app: &mut LogocraftApp, cmd: &str, params: Value) {
    if let Err(e) = app.run(cmd, params) {
        app.status(e);
    }
}

/// Set effect `index`'s params (on the object's own list), live while dragging.
fn set(app: &mut LogocraftApp, index: usize, params: Value, phase: Live) {
    live_run(app, "Effect", "effect.setParams", json!({ "index": index, "item": null, "params": params }), phase);
}

/// The section, for the first selected object.
pub(crate) fn section(app: &mut LogocraftApp, ui: &mut Ui) {
    let Some(effects) = super::first_node(app).map(|n| n.appearance.effects.clone()) else { return };
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        section_header(ui, tl!("Effects"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Clear of the panel's scroll bar.
            ui.add_space(10.0);
            if widgets::icon_button(ui, "plus", tl!("Add effect"), false, 22.0).clicked() {
                // Figma adds a drop shadow first.
                let (_, id, defaults) = kinds()[0].clone();
                run(app, "effect.apply", json!({ "effect": id, "params": defaults, "item": null }));
                let mut open: Vec<usize> = pstate(&ctx, OPEN);
                open.push(effects.len());
                set_pstate(&ctx, OPEN, open);
            }
        });
    });
    for (i, e) in effects.iter().enumerate() {
        row(app, ui, i, e);
    }
}

/// One effect: eye, type, settings toggle, remove; then its settings when open.
fn row(app: &mut LogocraftApp, ui: &mut Ui, i: usize, e: &Effect) {
    let ctx = ui.ctx().clone();
    let mut open: Vec<usize> = pstate(&ctx, OPEN);
    let is_open = open.contains(&i);
    let kind = kind_of(&e.id);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let eye = if e.visible { "eye" } else { "eye-off" };
        if widgets::icon_button(ui, eye, if e.visible { tl!("Hide effect") } else { tl!("Show effect") }, false, 22.0).clicked() {
            run(app, "effect.setParams", json!({ "index": i, "item": null, "visible": !e.visible }));
        }
        // Any effect: Figma's types first, then every effect of the app by group.
        let current = match kind {
            Some(k) => kinds().get(k).map(|(l, _, _)| tl!(*l).to_string()).unwrap_or_default(),
            None => logocraft_effects::effect_info(&e.id).map(|x| x.label.trim_end_matches('…').to_string()).unwrap_or_else(|| e.id.clone()),
        };
        if let Some((id, params)) = type_menu(ui, i, &current)
            && id != e.id
        {
            run(app, "effect.setType", json!({ "index": i, "item": null, "effect": id, "params": params }));
            // A Figma type opens its settings; another effect has its dialog behind the gear.
            if kind_of(id).is_some() && !open.contains(&i) {
                open.push(i);
                set_pstate(&ctx, OPEN, open.clone());
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, "minus", tl!("Remove effect"), false, 22.0).clicked() {
                run(app, "effect.remove", json!({ "index": i, "item": null }));
                open.retain(|o| *o != i);
                set_pstate(&ctx, OPEN, open.clone());
            }
            // Other effects: the gear opens their dialog.
            if kind.is_none() && widgets::icon_button(ui, "settings", tl!("Effect options"), false, 22.0).clicked() {
                run(app, "effect.dialog", json!({ "effect": e.id, "index": i, "item": null }));
            }
            if kind.is_some() && widgets::icon_button(ui, "settings", tl!("Effect settings"), is_open, 22.0).clicked() {
                if is_open {
                    open.retain(|o| *o != i);
                } else {
                    open.push(i);
                }
                set_pstate(&ctx, OPEN, open.clone());
            }
        });
    });
    if is_open && let Some(k) = kind {
        let p = logocraft_effects::merged_params(&e.id, &e.params);
        ui.indent(("fx-settings", i), |ui| settings(app, ui, i, k, &p));
    }
}

/// A number field after a short label.
fn number(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, label: &str, value: f64, suffix: &str) -> Option<f64> {
    ui.add_sized([40.0, 22.0], egui::Label::new(egui::RichText::new(label).size(11.5)));
    widgets::plain_field(ui, id, value, suffix, 1, 56.0)
}

fn num(p: &Value, k: &str, default: f64) -> f64 {
    p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or(default)
}

/// Effect `i`'s settings, for Figma type `k` (its params `p`, defaults merged).
fn settings(app: &mut LogocraftApp, ui: &mut Ui, i: usize, k: usize, p: &Value) {
    match k {
        // Drop and inner shadow: X, Y, blur, spread, colour.
        0 | 1 => {
            ui.horizontal(|ui| {
                if let Some(v) = number(ui, ("fx-x", i), "X", num(p, "x", 0.0), " pt") {
                    set(app, i, json!({ "x": v }), Live::Released);
                }
                if let Some(v) = number(ui, ("fx-y", i), "Y", num(p, "y", 4.0), " pt") {
                    set(app, i, json!({ "y": v }), Live::Released);
                }
            });
            ui.horizontal(|ui| {
                if let Some(v) = number(ui, ("fx-blur", i), tl!("Blur"), num(p, "blur", 4.0), " pt") {
                    set(app, i, json!({ "blur": v.max(0.0) }), Live::Released);
                }
                if let Some(v) = number(ui, ("fx-spread", i), tl!("Spread"), num(p, "spread", 0.0), " pt") {
                    set(app, i, json!({ "spread": v.max(0.0) }), Live::Released);
                }
            });
            color_row(app, ui, i, p);
        }
        // Layer and background blur: the radius.
        2 | 3 => {
            ui.horizontal(|ui| {
                if let Some(v) = number(ui, ("fx-radius", i), tl!("Blur"), num(p, "radius", 4.0), " pt") {
                    set(app, i, json!({ "radius": v.max(0.0) }), Live::Released);
                }
            });
            if k == 3 {
                widgets::dim_label(ui, tl!("Shows through a translucent fill (lower the fill's opacity)."));
            }
        }
        // Noise: mono or multi, size, density, colour and opacity.
        4 => {
            ui.horizontal(|ui| {
                ui.add_sized([40.0, 22.0], egui::Label::new(egui::RichText::new(tl!("Type")).size(11.5)));
                let multi = p.get("type").and_then(Value::as_str) == Some("multi");
                let options = [tl!("Mono"), tl!("Multi")];
                if let Some(n) = widgets::dropdown(ui, ("fx-noise-type", i), options[usize::from(multi)], &options, 90.0) {
                    set(app, i, json!({ "type": if n == 1 { "multi" } else { "mono" } }), Live::Released);
                }
            });
            ui.horizontal(|ui| {
                if let Some(v) = number(ui, ("fx-size", i), tl!("Size"), num(p, "size", 0.5), " pt") {
                    set(app, i, json!({ "size": v.max(0.05) }), Live::Released);
                }
                if let Some(v) = number(ui, ("fx-density", i), tl!("Density"), num(p, "density", 50.0), "%") {
                    set(app, i, json!({ "density": v.clamp(0.0, 100.0) }), Live::Released);
                }
            });
            if p.get("type").and_then(Value::as_str) != Some("multi") {
                color_row(app, ui, i, p);
            } else {
                ui.horizontal(|ui| {
                    if let Some(v) = number(ui, ("fx-noise-op", i), tl!("Opacity"), num(p, "opacity", 25.0), "%") {
                        set(app, i, json!({ "opacity": v.clamp(0.0, 100.0) }), Live::Released);
                    }
                });
            }
        }
        // Texture: size, radius, clip to shape.
        _ => {
            ui.horizontal(|ui| {
                if let Some(v) = number(ui, ("fx-tsize", i), tl!("Size"), num(p, "size", 2.0), " pt") {
                    set(app, i, json!({ "size": v.max(0.05) }), Live::Released);
                }
                if let Some(v) = number(ui, ("fx-tradius", i), tl!("Radius"), num(p, "radius", 2.0), " pt") {
                    set(app, i, json!({ "radius": v.max(0.0) }), Live::Released);
                }
            });
            let mut clip = p.get("clip").and_then(Value::as_bool).unwrap_or(true);
            if ui.checkbox(&mut clip, tl!("Clip to shape")).changed() {
                set(app, i, json!({ "clip": clip }), Live::Released);
            }
        }
    }
}

/// An effect colour as stored: `#rrggbb`, or `{c, m, y, k}` for a CMYK colour (kept as CMYK).
pub(crate) fn effect_color_json(c: &Color) -> Value {
    match c {
        Color::Cmyk { .. } => super::color_json(c),
        _ => json!(c.to_hex()),
    }
}

/// The colour an effect's `color` param holds.
fn effect_color(p: &Value) -> Color {
    match p.get("color") {
        Some(Value::String(s)) => Color::from_hex(s).unwrap_or(Color::BLACK),
        Some(v) => logocraft_engine::cmd::color_value(v).unwrap_or(Color::BLACK),
        None => Color::BLACK,
    }
}

/// The colour chip (opening the app's colour picker, with the effect's opacity on its rail) and
/// the opacity field.
fn color_row(app: &mut LogocraftApp, ui: &mut Ui, i: usize, p: &Value) {
    let c = effect_color(p);
    let opacity = (num(p, "opacity", 25.0) / 100.0).clamp(0.0, 1.0) as f32;
    ui.horizontal(|ui| {
        let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
        widgets::swatch_tile(ui, rect, &Paint::solid(c), false, resp.hovered());
        let resp = resp.on_hover_text(tl!("Color"));
        crate::widgets::popover(&resp, resp.clicked(), |ui| {
            ui.set_width(super::fill_picker::WIDTH);
            let e = super::fill_picker::color_controls(ui, &format!("fx-color-{i}"), c, Some(opacity));
            if let Some((nc, phase)) = e.color {
                set(app, i, json!({ "color": effect_color_json(&nc) }), phase);
            }
            if let Some((a, phase)) = e.alpha {
                set(app, i, json!({ "opacity": (a * 100.0).round() }), phase);
            }
        });
        ui.label(egui::RichText::new(c.to_hex().trim_start_matches('#').to_ascii_uppercase()).size(11.5));
        if let Some(v) = widgets::plain_field(ui, ("fx-opacity", i), f64::from(opacity * 100.0).round(), "%", 0, 48.0) {
            set(app, i, json!({ "opacity": v.clamp(0.0, 100.0) }), Live::Released);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The type menu lists every effect of the app: Figma's six, then all the others by group.
    #[test]
    fn the_type_menu_lists_every_effect() {
        let listed: Vec<&str> =
            kinds().iter().map(|(_, id, _)| *id).chain(catalog_groups().iter().flat_map(|(_, items)| items.iter().map(|(_, id)| *id))).collect();
        for e in logocraft_effects::effect_catalog() {
            assert_eq!(listed.iter().filter(|id| **id == e.id).count(), 1, "{} listed once", e.id);
        }
        assert!(catalog_groups().len() > 5, "{:?}", catalog_groups().iter().map(|(g, _)| g).collect::<Vec<_>>());
    }

    #[test]
    fn every_figma_kind_is_a_known_effect() {
        for (_, id, defaults) in kinds() {
            assert!(logocraft_effects::effect_info(id).is_some(), "{id}");
            assert!(defaults.is_object());
        }
        assert_eq!(kind_of("blur.background"), Some(3));
        assert_eq!(kind_of("distort.roughen"), None);
        assert_eq!(effect_color_json(&Color::rgb8(255, 0, 0)), json!("#ff0000"));
        assert!(effect_color_json(&Color::cmyk(0.0, 1.0, 1.0, 0.0)).get("m").is_some(), "CMYK stays CMYK");
    }

    #[test]
    fn the_section_adds_and_lists_effects() {
        use super::super::tests_appearance::{app_with_rect, run};
        let mut app = app_with_rect();
        let text = crate::tests_labels::painted_text(&mut app, section);
        assert!(text.lines().any(|l| l == "Effects"), "{text}");
        for (_, id, defaults) in kinds() {
            run(&mut app, "effect.apply", json!({ "effect": id, "params": defaults, "item": null }));
        }
        let text = crate::tests_labels::painted_text(&mut app, section);
        for label in ["Drop shadow", "Inner shadow", "Layer blur", "Background blur", "Noise", "Texture"] {
            assert!(text.contains(label), "{label}: {text}");
        }
        // Switching a type keeps its place, in one step.
        run(&mut app, "effect.setType", json!({ "index": 0, "item": null, "effect": "stylize.innerShadow" }));
        let fx = &super::super::first_node(&app).unwrap().appearance.effects;
        assert_eq!(fx[0].id, "stylize.innerShadow");
        assert_eq!(fx.len(), 6);
    }
}
