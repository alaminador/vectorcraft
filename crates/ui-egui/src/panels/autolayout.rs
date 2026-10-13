//! The Properties panel's Auto layout sections, laid out like Figma's (public documentation:
//! https://help.figma.com/hc/en-us/articles/360040451373): for a frame, its direction (row,
//! column, wrap), width and height (Hug or Fixed), the alignment grid, the gap (or Auto), the
//! padding, and its fill and corner radius; for a child of a frame, how it sizes (Fixed, Hug,
//! Fill) and Absolute position. Every change is a `layout.*` command.

use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, vec2};
use logocraft_doc::Node;
use logocraft_doc::layout::{Align, AutoLayout, Direction, ItemSizing, Sizing};
use serde_json::{Value, json};

use super::{pstate, set_pstate};
use crate::LogocraftApp;
use crate::theme::Tokens;
use crate::widgets::{self, section_header};

/// Panel state: the padding shows all four sides.
const PER_SIDE: &str = "autolayout-per-side";

/// Run a `layout.*` command, reporting a refusal in the status bar.
fn run(app: &mut LogocraftApp, cmd: &str, params: Value) {
    if let Err(e) = app.run(cmd, params) {
        app.status(e);
    }
}

/// Is `n` a child of an auto layout frame?
pub(crate) fn in_frame(app: &LogocraftApp, n: &Node) -> bool {
    app.session.active().and_then(|st| st.doc.parent_of(n.id).and_then(|p| st.doc.node(p))).is_some_and(|p| p.extras().layout.is_some())
}

/// A small button with a painted glyph (`draw` gets the glyph's rect and colour).
fn glyph_button(ui: &mut Ui, tip: &str, selected: bool, draw: impl FnOnce(&egui::Painter, Rect, Color32)) -> bool {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 24.0), Sense::click());
    let bg = if selected {
        t.tool_active
    } else if resp.hovered() {
        t.hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 3.0, bg);
    let color = if selected { t.text_strong } else { t.icon };
    draw(ui.painter(), rect.shrink(6.0), color);
    resp.on_hover_text(tip).clicked()
}

/// An arrow from `a` to `b`.
fn arrow(p: &egui::Painter, a: Pos2, b: Pos2, c: Color32) {
    let s = Stroke::new(1.5, c);
    p.line_segment([a, b], s);
    let d = (b - a).normalized();
    let n = vec2(-d.y, d.x);
    p.line_segment([b, b - d * 4.0 + n * 3.0], s);
    p.line_segment([b, b - d * 4.0 - n * 3.0], s);
}

/// The 3 × 3 alignment grid: a dot per position, the current one a bar; returns the clicked
/// (x, y) alignment.
fn align_grid(ui: &mut Ui, l: &AutoLayout) -> Option<(Align, Align)> {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(66.0, 66.0), Sense::hover());
    ui.painter().rect_filled(rect, 4.0, t.input);
    ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, t.input_border), StrokeKind::Inside);
    let (x_now, y_now) = if l.direction == Direction::Horizontal { (l.align_main, l.align_cross) } else { (l.align_cross, l.align_main) };
    let all = [Align::Start, Align::Center, Align::End];
    let mut clicked = None;
    for (j, y) in all.iter().enumerate() {
        for (i, x) in all.iter().enumerate() {
            let cell = Rect::from_min_size(rect.min + vec2(i as f32 * 22.0, j as f32 * 22.0), vec2(22.0, 22.0));
            let resp = ui.interact(cell, ui.id().with(("al-grid", i, j)), Sense::click());
            let on = *x == x_now && *y == y_now;
            if on {
                // Bars along the direction, as Figma draws the chosen cell.
                let c = cell.center();
                let (w, h) = if l.direction == Direction::Horizontal { (3.0, 10.0) } else { (10.0, 3.0) };
                for k in [-1.0f32, 0.0, 1.0] {
                    let off = if l.direction == Direction::Horizontal { vec2(k * 4.5, 0.0) } else { vec2(0.0, k * 4.5) };
                    ui.painter().rect_filled(Rect::from_center_size(c + off, vec2(w, h)), 1.0, t.accent);
                }
            } else {
                ui.painter().circle_filled(cell.center(), 1.6, if resp.hovered() { t.text_strong } else { t.text_disabled });
            }
            if resp.on_hover_text(grid_name(*x, *y)).clicked() {
                clicked = Some((*x, *y));
            }
        }
    }
    clicked
}

/// `layout.set`'s name of an (x, y) alignment.
pub(crate) fn grid_name(x: Align, y: Align) -> &'static str {
    match (y, x) {
        (Align::Start, Align::Start) => "top-left",
        (Align::Start, Align::Center) => "top",
        (Align::Start, Align::End) => "top-right",
        (Align::Center, Align::Start) => "left",
        (Align::Center, Align::Center) => "center",
        (Align::Center, Align::End) => "right",
        (Align::End, Align::Start) => "bottom-left",
        (Align::End, Align::Center) => "bottom",
        (Align::End, Align::End) => "bottom-right",
    }
}

/// A label before a field.
fn label(ui: &mut Ui, text: &str, width: f32) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
    ui.painter().text(r.left_center(), egui::Align2::LEFT_CENTER, text, egui::FontId::proportional(11.5), t.text_dim);
}

/// The section for the selected frame `n`. → whether it drew.
pub(crate) fn frame_section(app: &mut LogocraftApp, ui: &mut Ui, n: &Node) -> bool {
    let Some(l) = n.extras().layout.as_deref().cloned() else { return false };
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        section_header(ui, tl!("Auto layout"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, "minus", tl!("Remove Auto Layout (Alt+Shift+A)"), false, 22.0).clicked() {
                run(app, "layout.remove", json!({}));
            }
        });
    });
    let horizontal = l.direction == Direction::Horizontal;
    // Direction: column, row, wrap.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if glyph_button(ui, tl!("Vertical layout"), !horizontal, |p, r, c| arrow(p, r.center_top(), r.center_bottom(), c)) {
            run(app, "layout.set", json!({ "direction": "vertical", "wrap": false }));
        }
        if glyph_button(ui, tl!("Horizontal layout"), horizontal && !l.wrap, |p, r, c| arrow(p, r.left_center(), r.right_center(), c)) {
            run(app, "layout.set", json!({ "direction": "horizontal", "wrap": false }));
        }
        let wrap_tip = tl!("Wrap (a fixed-width row flows onto new rows)");
        if glyph_button(ui, wrap_tip, horizontal && l.wrap, |p, r, c| {
            let s = Stroke::new(1.5, c);
            p.line_segment([r.left_top() + vec2(0.0, 2.0), r.right_top() + vec2(0.0, 2.0)], s);
            p.line_segment([r.right_top() + vec2(0.0, 2.0), r.right_center()], s);
            arrow(p, r.right_center(), r.left_center() + vec2(2.0, 0.0), c);
            p.line_segment([r.left_bottom() - vec2(0.0, 1.0), r.right_bottom() - vec2(4.0, 1.0)], s);
        }) {
            // Wrapping needs a width to wrap at.
            let mut p = json!({ "direction": "horizontal", "wrap": true });
            if l.width == Sizing::Hug {
                p["width"] = json!(l.frame.width());
            }
            run(app, "layout.set", p);
        }
    });
    ui.add_space(4.0);
    // Width and height: Hug or Fixed, and the size.
    for (axis, key, sizing, size) in [("W", "width", l.width, l.frame.width()), ("H", "height", l.height, l.frame.height())] {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            label(ui, axis, 14.0);
            if let Some(v) = widgets::plain_field(ui, ("al-size", key), size, " pt", 1, 64.0) {
                run(app, "layout.set", json!({ key: v.max(0.0) }));
            }
            let options = [tl!("Fixed"), tl!("Hug contents")];
            let current = if sizing == Sizing::Hug { options[1] } else { options[0] };
            if let Some(i) = widgets::dropdown(ui, ("al-sizing", key), current, &options, 108.0) {
                run(app, "layout.set", json!({ key: if i == 1 { "hug" } else { "fixed" } }));
            }
        });
    }
    ui.add_space(4.0);
    // The alignment grid beside the gap and the padding.
    ui.horizontal(|ui| {
        if let Some((x, y)) = align_grid(ui, &l) {
            run(app, "layout.set", json!({ "align": grid_name(x, y) }));
        }
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                label(ui, tl!("Gap"), 30.0);
                if l.gap_auto {
                    ui.label(egui::RichText::new(tl!("Auto")).size(12.0));
                } else if let Some(v) = widgets::plain_field(ui, "al-gap", l.gap, "", 1, 48.0) {
                    run(app, "layout.set", json!({ "gap": v }));
                }
                super::variables::bind_button(app, ui, n, "gap");
            });
            let mut auto = l.gap_auto;
            if ui.checkbox(&mut auto, tl!("Auto (space between)")).changed() {
                let mut p = json!({ "gap": if auto { json!("auto") } else { json!(l.gap) } });
                // Spreading needs a fixed main axis.
                let main_hugs = if horizontal { l.width == Sizing::Hug } else { l.height == Sizing::Hug };
                if auto && main_hugs {
                    p[if horizontal { "width" } else { "height" }] = json!(if horizontal { l.frame.width() } else { l.frame.height() });
                }
                run(app, "layout.set", p);
            }
            if l.wrap {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    label(ui, tl!("Rows"), 30.0);
                    if let Some(v) = widgets::plain_field(ui, "al-cross-gap", l.cross_gap, "", 1, 48.0) {
                        run(app, "layout.set", json!({ "crossGap": v }));
                    }
                });
            }
        });
    });
    ui.add_space(4.0);
    // Padding: horizontal and vertical, or each side.
    let per_side: bool = pstate(&ctx, PER_SIDE);
    let [top, right, bottom, left] = l.padding;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        label(ui, tl!("Padding"), 48.0);
        if per_side {
            for (k, v) in [("paddingTop", top), ("paddingRight", right), ("paddingBottom", bottom), ("paddingLeft", left)] {
                if let Some(n) = widgets::plain_field(ui, ("al-pad", k), v, "", 1, 34.0).filter(|n| *n >= 0.0) {
                    run(app, "layout.set", json!({ k: n }));
                }
            }
        } else {
            let h = if (left - right).abs() < 1e-9 { left } else { left.max(right) };
            let v = if (top - bottom).abs() < 1e-9 { top } else { top.max(bottom) };
            label(ui, "↔", 12.0);
            if let Some(n) = widgets::plain_field(ui, "al-pad-h", h, "", 1, 44.0).filter(|n| *n >= 0.0) {
                run(app, "layout.set", json!({ "paddingH": n }));
            }
            label(ui, "↕", 12.0);
            if let Some(n) = widgets::plain_field(ui, "al-pad-v", v, "", 1, 44.0).filter(|n| *n >= 0.0) {
                run(app, "layout.set", json!({ "paddingV": n }));
            }
        }
        if widgets::icon_button(ui, "maximize-2", tl!("Padding for each side"), per_side, 22.0).clicked() {
            set_pstate(&ctx, PER_SIDE, !per_side);
        }
        super::variables::bind_button(app, ui, n, "padding");
    });
    ui.add_space(4.0);
    // The frame's own fill and corner radius (its background).
    let bg = n.children().and_then(|ch| ch.iter().find(|c| c.extras().layout_item.is_some_and(|i| i.background)).cloned());
    let fill = bg.as_ref().and_then(|b| b.appearance.fill_paint().color()).map(|c| c.to_hex().trim_start_matches('#').to_ascii_uppercase());
    let radius = bg.as_ref().and_then(|b| match b.live_shape() {
        Some(logocraft_doc::LiveShape::Rectangle { radii, .. }) => Some(radii[0]),
        _ => None,
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        label(ui, tl!("Fill"), 30.0);
        // The Fill chip: the colour picker for the frame's own fill (a selected frame's proxies
        // stand for its background).
        super::paint_chip(app, ui, false, 22.0, true);
        if let Some(hex) = &fill {
            ui.label(egui::RichText::new(format!("#{hex}")).size(11.5));
        }
        if fill.is_some() && widgets::icon_button(ui, "minus", tl!("No fill"), false, 20.0).clicked() {
            run(app, "layout.set", json!({ "fill": null }));
        }
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        label(ui, tl!("Radius"), 44.0);
        if let Some(v) = widgets::plain_field(ui, "al-radius", radius.unwrap_or(0.0), " pt", 1, 64.0) {
            run(app, "layout.set", json!({ "radius": v.max(0.0) }));
        }
    });
    let mut clip = l.clip;
    if ui.checkbox(&mut clip, tl!("Clip content")).changed() {
        run(app, "layout.set", json!({ "clip": clip }));
    }
    limits_rows(app, ui, n, "al-own");
    true
}

/// Min and max width and height fields (blank: none); a frame's own, or a child's.
fn limits_rows(app: &mut LogocraftApp, ui: &mut Ui, n: &Node, id: &str) {
    let lim = n.extras().layout_item.map(|i| i.limits).unwrap_or_default();
    for (axis, min_k, max_k, min, max) in
        [("W", "minWidth", "maxWidth", lim.min_width, lim.max_width), ("H", "minHeight", "maxHeight", lim.min_height, lim.max_height)]
    {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            label(ui, &format!("{} {axis}", tl!("Min")), 40.0);
            let shown = |v: Option<f64>| v.map(|v| format!("{v}")).unwrap_or_default();
            if let Some(t) = widgets::text_field(ui, (id, min_k), Some(&shown(min)), 52.0, 1) {
                let v = t.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0);
                run(app, "layout.setItem", json!({ "ids": [n.id.0], min_k: v }));
            }
            label(ui, tl!("Max"), 28.0);
            if let Some(t) = widgets::text_field(ui, (id, max_k), Some(&shown(max)), 52.0, 1) {
                let v = t.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0);
                run(app, "layout.setItem", json!({ "ids": [n.id.0], max_k: v }));
            }
        });
    }
}

/// The section for a selected child `n` of a frame. → whether it drew.
pub(crate) fn child_section(app: &mut LogocraftApp, ui: &mut Ui, n: &Node) -> bool {
    let item = n.extras().layout_item.unwrap_or_default();
    if item.background {
        return false;
    }
    section_header(ui, tl!("Auto layout child"));
    let own = n.extras().layout.as_deref();
    let is_text_point = matches!(&n.kind, logocraft_doc::NodeKind::Text(t) if !matches!(t.kind, logocraft_doc::TextKind::Area { .. }));
    for (axis, key, sizing, own_sizing) in [("W", "width", item.width, own.map(|l| l.width)), ("H", "height", item.height, own.map(|l| l.height))] {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            label(ui, axis, 14.0);
            // Hug is a frame's (and point type's, which always hugs); Fill on point type makes it
            // auto-height area type.
            let mut options: Vec<(&str, &str)> = vec![(tl!("Fixed"), "fixed")];
            if own.is_some() {
                options.push((tl!("Hug contents"), "hug"));
            }
            options.push((tl!("Fill container"), "fill"));
            let current = match (sizing, own_sizing) {
                (ItemSizing::Fill, _) => tl!("Fill container"),
                (_, Some(Sizing::Hug)) => tl!("Hug contents"),
                _ if is_text_point => tl!("Hug contents"),
                _ => tl!("Fixed"),
            };
            let labels: Vec<&str> = options.iter().map(|(l, _)| *l).collect();
            if !item.absolute
                && let Some(i) = widgets::dropdown(ui, ("al-item", key), current, &labels, 140.0)
                && let Some((_, v)) = options.get(i)
            {
                run(app, "layout.setItem", json!({ key: v }));
            }
        });
    }
    let mut absolute = item.absolute;
    if ui.checkbox(&mut absolute, tl!("Absolute position")).on_hover_text(tl!("Out of the flow: stays where you put it")).changed() {
        run(app, "layout.setItem", json!({ "absolute": absolute }));
    }
    if !item.absolute {
        limits_rows(app, ui, n, "al-item-limits");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_names_are_the_commands() {
        assert_eq!(grid_name(Align::Start, Align::Start), "top-left");
        assert_eq!(grid_name(Align::Center, Align::End), "bottom");
        assert_eq!(grid_name(Align::End, Align::Center), "right");
    }

    #[test]
    fn properties_shows_the_frame_and_its_children() {
        use super::super::tests_appearance::run;
        let mut app = LogocraftApp::new(logocraft_engine::Session::new(), Default::default());
        run(&mut app, "file.new", json!({"width": 300, "height": 300}));
        let a = run(&mut app, "shape.rectangle", json!({"x": 10, "y": 10, "width": 40, "height": 20}))["id"].clone();
        let b = run(&mut app, "shape.rectangle", json!({"x": 80, "y": 10, "width": 40, "height": 20}))["id"].clone();
        run(&mut app, "select.set", json!({"ids": [a, b]}));
        let text = crate::tests_labels::painted_text(&mut app, super::super::properties::show);
        assert!(text.lines().any(|l| l == "Add Auto Layout"), "a quick action: {text}");
        run(&mut app, "layout.add", json!({}));
        let text = crate::tests_labels::painted_text(&mut app, super::super::properties::show);
        for s in ["Frame", "Auto layout", "Hug contents", "Gap", "Padding", "Fill", "Radius", "Clip content", "Max"] {
            assert!(text.lines().any(|l| l == s), "{s}: {text}");
        }
        assert!(!text.lines().any(|l| l == "Add Auto Layout"));
        run(&mut app, "select.set", json!({"ids": [a]}));
        let text = crate::tests_labels::painted_text(&mut app, super::super::properties::show);
        assert!(text.lines().any(|l| l == "Auto layout child") && text.contains("Absolute position"), "{text}");
    }
}
