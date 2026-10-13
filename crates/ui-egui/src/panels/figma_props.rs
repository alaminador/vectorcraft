//! The Properties panel as Figma lays it out (the Figma workspace, `UiState::figma_panel`), after
//! Figma's public documentation of the Design panel
//! (https://help.figma.com/hc/en-us/articles/360039832014): the align row on top; Appearance
//! (opacity, blend mode); Fill and Stroke as lists of rows (colour chip, hex or paint kind,
//! opacity, eye to hide, minus to remove, plus to add), the stroke's weight and position;
//! Effects; Export (scale, format, Export). Every change is a command.
//!
//! The fills and strokes are the object's appearance items (`appearance.*`): a fill or stroke
//! whose paint is None is Illustrator's empty slot and shows no row, as Figma shows none; the
//! plus paints that slot before it adds a new item, and the minus on the last one empties it.

use egui::{Sense, Ui, vec2};
use logocraft_color::{BlendMode, Color, Paint};
use logocraft_doc::{AppearanceItem, Node, NodeKind};
use serde_json::{Value, json};

use super::{fill_picker, live_run};
use crate::LogocraftApp;
use crate::theme::Tokens;
use crate::widgets::{self, section_header};

/// Figma's default new fill (#D9D9D9) and stroke (black).
const NEW_FILL: &str = "#D9D9D9";
const NEW_STROKE: &str = "#000000";

fn run(app: &mut LogocraftApp, cmd: &str, params: Value) {
    if let Err(e) = app.run(cmd, params) {
        app.status(e);
    }
}

/// The align buttons, first, as Figma puts them.
pub(crate) fn align_row(app: &mut LogocraftApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        super::align::align_buttons(app, ui, 24.0);
        if widgets::icon_button(ui, "ellipsis", tl!("More Align options"), false, 24.0).clicked() {
            app.ui.open_panel = Some("align".into());
        }
    });
}

/// Appearance: the object's opacity and blend mode, as Figma's layer row.
pub(crate) fn appearance_section(app: &mut LogocraftApp, ui: &mut Ui, n: &Node) {
    section_header(ui, tl!("Appearance"));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let labels: Vec<&str> = BlendMode::ALL.iter().map(|m| m.label()).collect();
        if let Some(i) = widgets::dropdown(ui, "fg-blend", n.blend.label(), &labels, 110.0)
            && let Some(m) = BlendMode::ALL.get(i)
        {
            run(app, "object.setProps", json!({ "blend": m.label() }));
        }
        widgets::dim_label(ui, "◐");
        if let Some(o) = widgets::plain_field(ui, "fg-opacity", f64::from(n.opacity) * 100.0, "%", 0, 52.0) {
            run(app, "object.setProps", json!({ "opacity": o.clamp(0.0, 100.0) }));
        }
        super::variables::bind_button(app, ui, n, "opacity");
    });
}

/// One fill or stroke of the object: where it sits in the appearance stack and what it paints.
struct Row {
    index: usize,
    paint: Paint,
    opacity: f32,
    visible: bool,
    /// The topmost of its kind: the one the full picker (Color, Gradient, Image) edits.
    top: bool,
}

/// The rows of `n`'s fills (`stroke` false) or strokes, top first as Figma lists them; and
/// whether it has an empty slot (an item painting None) the plus can paint.
fn rows(n: &Node, stroke: bool) -> (Vec<Row>, Option<usize>) {
    let items = &n.appearance.items;
    let kind = |i: &AppearanceItem| matches!((i, stroke), (AppearanceItem::Fill(_), false) | (AppearanceItem::Stroke(_), true));
    let top = items.iter().rposition(kind);
    let mut out = vec![];
    let mut empty = None;
    for (index, item) in items.iter().enumerate().rev() {
        let (paint, opacity, visible) = match item {
            AppearanceItem::Fill(f) if !stroke => (f.paint.clone(), f.opacity, f.visible),
            AppearanceItem::Stroke(s) if stroke => (s.paint.clone(), s.opacity, s.visible),
            _ => continue,
        };
        if matches!(paint, Paint::None) {
            empty = empty.or(Some(index));
            continue;
        }
        out.push(Row { index, paint, opacity, visible, top: Some(index) == top });
    }
    (out, empty)
}

/// What a row's text shows: the hex of a colour, else the kind of paint.
fn paint_text(p: &Paint) -> String {
    match p {
        Paint::Solid { color, .. } => color.to_hex().trim_start_matches('#').to_ascii_uppercase(),
        Paint::Gradient(g) => match g.gradient.kind {
            logocraft_color::GradientKind::Linear => tl!("Linear").to_string(),
            logocraft_color::GradientKind::Radial => tl!("Radial").to_string(),
            _ => tl!("Gradient").to_string(),
        },
        Paint::None => tl!("None").to_string(),
        _ => tl!("Image").to_string(),
    }
}

/// The Fill and Stroke sections.
pub(crate) fn paint_sections(app: &mut LogocraftApp, ui: &mut Ui, n: &Node) {
    paint_section(app, ui, n, false);
    widgets::divider(ui);
    paint_section(app, ui, n, true);
}

fn paint_section(app: &mut LogocraftApp, ui: &mut Ui, n: &Node, stroke: bool) {
    let (list, empty) = rows(n, stroke);
    // Type keeps its paint in its characters: the proxy's paint stands for it.
    let text_paint = matches!(n.kind, NodeKind::Text(_)) && list.is_empty() && empty.is_none();
    ui.horizontal(|ui| {
        section_header(ui, if stroke { tl!("Stroke") } else { tl!("Fill") });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Clear of the panel's scroll bar.
            ui.add_space(10.0);
            let tip = if stroke { tl!("Add stroke") } else { tl!("Add fill") };
            if widgets::icon_button(ui, "plus", tip, false, 22.0).clicked() {
                add(app, n, stroke, empty, list.is_empty());
            }
        });
    });
    if text_paint {
        ui.horizontal(|ui| {
            super::paint_chip(app, ui, stroke, 20.0, false);
            let (fill, stroke_paint) = super::current_paints(app);
            ui.label(egui::RichText::new(paint_text(if stroke { &stroke_paint } else { &fill })).size(11.5));
        });
        return;
    }
    let only = list.len() == 1;
    for r in &list {
        row(app, ui, n, r, stroke, only);
    }
    if stroke && !list.is_empty() {
        weight_row(app, ui, n);
    }
}

/// Plus: paint the empty slot (Illustrator's None fill or stroke) or, when every slot paints,
/// add an item on top.
fn add(app: &mut LogocraftApp, n: &Node, stroke: bool, empty: Option<usize>, none_shown: bool) {
    let color = if stroke { NEW_STROKE } else { NEW_FILL };
    match empty {
        Some(index) if none_shown => {
            let mut p = json!({ "index": index, "color": color, "visible": true });
            if stroke && stroke_width(n, index) <= 0.0 {
                p["weight"] = json!(1.0);
            }
            run(app, "appearance.setItem", p);
        }
        _ => {
            run(app, if stroke { "appearance.addStroke" } else { "appearance.addFill" }, json!({}));
            // The new item is a copy of the top one: Figma's starts from its default colour.
            let top = app.session.active().and_then(|st| st.doc.node(n.id)).and_then(|m| {
                m.appearance.items.iter().rposition(|i| matches!((i, stroke), (AppearanceItem::Fill(_), false) | (AppearanceItem::Stroke(_), true)))
            });
            if let Some(index) = top {
                run(app, "appearance.setItem", json!({ "index": index, "color": color, "opacity": 100, "visible": true }));
            }
        }
    }
}

fn stroke_width(n: &Node, index: usize) -> f64 {
    match n.appearance.items.get(index) {
        Some(AppearanceItem::Stroke(s)) => s.width,
        _ => 0.0,
    }
}

/// A fill or stroke row: chip (the picker), hex or paint kind, opacity, eye, minus.
fn row(app: &mut LogocraftApp, ui: &mut Ui, n: &Node, r: &Row, stroke: bool, only: bool) {
    let t = Tokens::get(ui.ctx());
    let key = (if stroke { "fg-stroke" } else { "fg-fill" }, r.index);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if r.top {
            // The top one opens the full picker: Color, Gradient, Image and the swatches.
            super::paint_chip(app, ui, stroke, 20.0, false);
        } else {
            let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
            widgets::swatch_tile(ui, rect, &r.paint, false, resp.hovered());
            let c = r.paint.color().unwrap_or(Color::BLACK);
            widgets::popover(&resp, resp.clicked(), |ui| {
                ui.set_width(fill_picker::WIDTH);
                let e = fill_picker::color_controls(ui, &format!("{}-{}", key.0, key.1), c, None);
                if let Some((c, phase)) = e.color {
                    live_run(app, "Appearance", "appearance.setItem", json!({ "index": r.index, "color": c.to_hex() }), phase);
                }
            });
        }
        let text = paint_text(&r.paint);
        let dim = if r.visible { t.text } else { t.text_dim };
        match r.paint.color() {
            Some(_) => {
                if let Some(s) = widgets::text_field(ui, ("fg-hex", key), Some(text.as_str()), 62.0, 1)
                    && let Some(c) = Color::from_hex(s.trim())
                {
                    run(app, "appearance.setItem", json!({ "index": r.index, "color": c.to_hex() }));
                }
            }
            None => {
                ui.add_sized([62.0, 20.0], egui::Label::new(egui::RichText::new(text).size(11.5).color(dim)));
            }
        }
        if let Some(o) = widgets::plain_field(ui, ("fg-op", key), f64::from(r.opacity) * 100.0, "%", 0, 44.0) {
            run(app, "appearance.setItem", json!({ "index": r.index, "opacity": o.clamp(0.0, 100.0) }));
        }
        let (icon, tip) = if r.visible { ("eye", tl!("Hide")) } else { ("eye-off", tl!("Show")) };
        if widgets::icon_button(ui, icon, tip, false, 20.0).clicked() {
            run(app, "appearance.setItem", json!({ "index": r.index, "visible": !r.visible }));
        }
        let tip = if stroke { tl!("Remove stroke") } else { tl!("Remove fill") };
        if widgets::icon_button(ui, "minus", tip, false, 20.0).clicked() {
            if only {
                // The last one: the slot stays, painting nothing (no fill / no stroke).
                run(app, "appearance.setItem", json!({ "index": r.index, "none": true }));
            } else {
                run(app, "appearance.removeItem", json!({ "index": r.index }));
            }
        }
        if r.top {
            super::variables::bind_button(app, ui, n, if stroke { "stroke" } else { "fill" });
        }
    });
}

/// The stroke's weight and position (Inside, Center, Outside), as Figma's stroke row.
fn weight_row(app: &mut LogocraftApp, ui: &mut Ui, n: &Node) {
    let Some(s) = n.appearance.stroke() else { return };
    let (width, align) = (s.width, s.align);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let options = [tl!("Inside"), tl!("Center"), tl!("Outside")];
        let ids = ["inside", "center", "outside"];
        let current = match align {
            logocraft_doc::StrokeAlign::Inside => 0,
            logocraft_doc::StrokeAlign::Outside => 2,
            _ => 1,
        };
        if let Some(i) = widgets::dropdown(ui, "fg-stroke-align", options[current], &options, 90.0)
            && let Some(id) = ids.get(i)
        {
            run(app, "stroke.set", json!({ "align": id }));
        }
        widgets::dim_label(ui, "≡");
        if let Some(w) = widgets::plain_field(ui, "fg-stroke-w", width, " pt", 1, 60.0) {
            run(app, "stroke.set", json!({ "weight": w.max(0.0) }));
        }
        super::variables::bind_button(app, ui, n, "strokeWeight");
    });
}

/// Export: a scale and a format for the selection, and Export, as Figma's export row.
pub(crate) fn export_section(app: &mut LogocraftApp, ui: &mut Ui, n: &Node) {
    const SCALES: [&str; 5] = ["0.5x", "1x", "2x", "3x", "4x"];
    const FORMATS: [&str; 4] = ["PNG", "JPG", "SVG", "PDF"];
    let ctx = ui.ctx().clone();
    let (mut scale, mut format): (usize, usize) = (super::pstate(&ctx, "fg-export-scale"), super::pstate(&ctx, "fg-export-format"));
    // Fresh state reads 0: Figma starts at 1x.
    let started: bool = super::pstate(&ctx, "fg-export-started");
    if !started {
        scale = 1;
        format = 0;
    }
    section_header(ui, tl!("Export"));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if let Some(i) = widgets::dropdown(ui, "fg-export-scale", SCALES[scale.min(SCALES.len() - 1)], &SCALES, 64.0) {
            scale = i;
            super::set_pstate(&ctx, "fg-export-started", true);
        }
        if let Some(i) = widgets::dropdown(ui, "fg-export-format", FORMATS[format.min(FORMATS.len() - 1)], &FORMATS, 70.0) {
            format = i;
            super::set_pstate(&ctx, "fg-export-started", true);
        }
    });
    super::set_pstate(&ctx, "fg-export-scale", scale);
    super::set_pstate(&ctx, "fg-export-format", format);
    let name = n.name.clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| tl!(n.kind_label()).to_string());
    if widgets::flat_button(ui, &format!("{} {name}", tl!("Export")), ui.available_width()).clicked() {
        let ext = FORMATS.get(format).copied().unwrap_or("PNG").to_ascii_lowercase();
        let factor = [0.5, 1.0, 2.0, 3.0, 4.0].get(scale).copied().unwrap_or(1.0);
        let file = format!("{}.{ext}", name.replace(['/', '\\', ':'], "-"));
        match crate::io::save_command_output_named(app, "document.exportSelection", &file, json!({ "format": ext, "scale": factor })) {
            Ok(_) => {}
            Err(e) => app.status(e),
        }
    }
}

/// Figma's selection actions in its panel: Create component, Combine as variants, Auto layout.
pub(crate) fn actions(app: &mut LogocraftApp, ui: &mut Ui, first: Option<&Node>, n_sel: usize) {
    let mut acts: Vec<(&str, &str)> = vec![];
    let is_component = first.is_some_and(|n| n.extras().component.is_some() || matches!(n.kind, NodeKind::SymbolInstance { .. }));
    let all_mains = n_sel > 1
        && app.session.active().is_some_and(|st| {
            st.selection.objects.iter().all(|id| st.doc.node(*id).is_some_and(|n| n.extras().component.is_some() && n.extras().variant.is_none()))
        });
    if all_mains {
        acts.push((tl!("Combine as Variants"), "component.combineVariants"));
    } else if !is_component {
        acts.push((tl!("Create Component"), "component.create"));
    }
    if !first.is_some_and(|n| n.extras().layout.is_some()) {
        acts.push((tl!("Add Auto Layout"), "layout.add"));
    }
    if n_sel > 1 {
        acts.push((tl!("Group"), "object.group"));
    }
    let w = (ui.available_width() - 6.0) / 2.0;
    for pair in acts.chunks(2) {
        ui.horizontal(|ui| {
            for (label, id) in pair {
                if widgets::flat_button(ui, label, w).clicked() {
                    crate::menus::invoke(app, id, json!({}));
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logocraft_engine::Session;

    fn app() -> LogocraftApp {
        let mut app = LogocraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 300, "height": 300})).unwrap();
        app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 100, "height": 60})).unwrap();
        app
    }

    fn node(app: &LogocraftApp) -> Node {
        let st = app.session.active().unwrap();
        st.doc.node(st.selection.objects[0]).unwrap().clone()
    }

    /// The minus empties the last fill and the last stroke (no fill, no stroke); the plus paints
    /// them again with Figma's defaults; the eye hides and shows.
    #[test]
    fn rows_remove_hide_and_add_fills_and_strokes() {
        let mut app = app();
        let n = node(&app);
        let (fills, _) = rows(&n, false);
        let (strokes, _) = rows(&n, true);
        assert_eq!((fills.len(), strokes.len()), (1, 1), "a new rectangle: one fill, one stroke");
        // Hide, then show, the fill.
        app.run("appearance.setItem", json!({"index": fills[0].index, "visible": false})).unwrap();
        assert!(!rows(&node(&app), false).0[0].visible);
        // Remove both: no rows, empty slots kept.
        for stroke in [false, true] {
            let n = node(&app);
            let (list, _) = rows(&n, stroke);
            app.run("appearance.setItem", json!({"index": list[0].index, "none": true})).unwrap();
        }
        let n = node(&app);
        let ((f, fe), (s, se)) = (rows(&n, false), rows(&n, true));
        assert!(f.is_empty() && s.is_empty() && fe.is_some() && se.is_some(), "no fill, no stroke");
        // Plus paints the empty slots.
        add(&mut app, &n, false, fe, true);
        let n = node(&app);
        add(&mut app, &n, true, se, true);
        let n = node(&app);
        let (f, s) = (rows(&n, false).0, rows(&n, true).0);
        assert_eq!(f[0].paint.color().map(|c| c.to_hex().to_ascii_uppercase()), Some(NEW_FILL.into()));
        assert_eq!(s.len(), 1);
        // A second plus adds a fill on top.
        add(&mut app, &n, false, None, false);
        assert_eq!(rows(&node(&app), false).0.len(), 2);
    }

    /// The Figma workspace turns on Figma's panel and leaves the toolbar's colour block out;
    /// the Classic workspace turns them the other way round.
    #[test]
    fn figma_and_illustrator_workspaces_switch_the_panel_and_toolbar() {
        let mut app = app();
        app.run("window.workspace", json!({"name": "Figma"})).unwrap();
        assert!(app.ui.figma_panel && !app.ui.color_block);
        app.run("window.workspace", json!({"name": "Classic"})).unwrap();
        assert!(!app.ui.figma_panel && app.ui.color_block);
    }

    #[test]
    fn paint_text_names_colours_and_kinds() {
        assert_eq!(paint_text(&Paint::solid(Color::rgb8(0xd9, 0xd9, 0xd9))), "D9D9D9");
        assert_eq!(paint_text(&Paint::None), "None");
    }
}
