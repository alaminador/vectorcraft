//! The Color Picker (double-click a Fill/Stroke proxy, or `ui.colorPicker`): the app's colour
//! picker ([`crate::panels::fill_picker::color_controls`]: square, hue rail, Hex/RGB/HSL/HSB/CMYK
//! with ink coverage and the CMYK gamut warning); Only Web Colors and the out-of-web warning
//! (click to correct); New/Original chips; and a Color Swatches list. With art selected the colour shows on it live as it is picked; OK keeps it as one
//! undo step through `paint.setFill` / `paint.setStroke`, Cancel puts the art back.
//!
//! Fields agents set with `ui.dialog.set`: `hex` (`"00FF00"`) or `color` (any colour the paint
//! commands take), `webOnly` and `swatches`. `stroke` and `original` come from the opener.

use egui::{Rect, Sense, Ui, vec2};
use logocraft_color::cms;
use logocraft_color::{Color, Paint};
use logocraft_doc::ColorMode;
use logocraft_engine::cmd::color_value;
use serde_json::{Value, json};

use super::DialogSpec;
use super::form::PREVIEWED;
use crate::panels::color::{WEB_WARNING, hex_digits, is_web_safe, parse_hex, warning_chip, web_safe};
use crate::panels::color_json;
use crate::state::Dialog;
use crate::theme::Tokens;
use crate::{LogocraftApp, widgets};

pub(super) const KIND: &str = "colorPicker";

pub(super) const SPEC: DialogSpec = DialogSpec {
    heading: |_| tl!("Color Picker").into(),
    body,
    confirm,
    preview: true,
    min_width: 460.0,
    max_width: Some(560.0),
    ..DialogSpec::FORM
};

/// Height of the swatch list.
const FIELD: f32 = 240.0;

/// The picked colour with its HSB (hue in degrees), kept so the hue survives grey colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Picked {
    pub color: Color,
    pub hsb: [f32; 3],
}

impl Picked {
    pub fn of(color: Color) -> Self {
        Self { color, hsb: color.to_hsb() }
    }
    fn from_hsb(hsb: [f32; 3]) -> Self {
        Self { color: Color::from_hsb(hsb[0], hsb[1], hsb[2]), hsb }
    }
    /// The nearest web colour when Only Web Colors is on.
    pub fn snapped(self, web: bool) -> Self {
        if web && !is_web_safe(&self.color) { Self::of(web_safe(&self.color)) } else { self }
    }
}

/// The colour the dialog holds: an edited `hex` wins over `color` (agents set either one).
fn picked(d: &Dialog) -> Picked {
    let hex = d.str("hex");
    let color = match parse_hex(&hex) {
        Some(c) if hex != d.str("__hex") => c,
        _ => d.fields.get("color").and_then(color_value).unwrap_or(Color::WHITE),
    };
    let hsb = d.fields.get("__hsb").and_then(|v| serde_json::from_value::<[f32; 3]>(v.clone()).ok());
    let p = match hsb {
        Some(h) if Picked::from_hsb(h).color.to_hex() == color.to_hex() => Picked { color, hsb: h },
        _ => Picked::of(color),
    };
    p.snapped(d.bool("webOnly"))
}

fn store(d: &mut Dialog, p: &Picked) {
    let hex = hex_digits(&p.color);
    d.fields.insert("color".into(), color_json(&p.color));
    d.fields.insert("hex".into(), json!(hex));
    d.fields.insert("__hex".into(), json!(hex));
    d.fields.insert("__hsb".into(), json!(p.hsb));
}

/// The colour the proxy shows (a gradient's first stop; the last colour for None and patterns).
fn proxy_color(app: &LogocraftApp, stroke: bool) -> Color {
    let (f, s) = crate::panels::current_paints(app);
    match if stroke { s } else { f } {
        Paint::Solid { color, .. } => color,
        Paint::Gradient(g) => g.gradient.stops.first().map_or(app.session.last_solid, |s| s.color),
        Paint::None | Paint::Pattern { .. } => app.session.last_solid,
    }
}

/// `ui.colorPicker {stroke?, color?}`: open the picker for the fill (or stroke) proxy, starting
/// from `color` or the proxy's colour.
pub fn open(app: &mut LogocraftApp, p: &Value) -> Result<Value, String> {
    let stroke = p.get("stroke").and_then(Value::as_bool).unwrap_or(!app.session.fill_active);
    let color = match p.get("color") {
        Some(v) => color_value(v).ok_or_else(|| format!("bad color {v}"))?,
        None => proxy_color(app, stroke),
    };
    let mut d = Dialog::new(KIND, json!({"stroke": stroke, "original": color_json(&color), "webOnly": false, "swatches": false}));
    store(&mut d, &Picked::of(color));
    app.ui.dialog = Some(d);
    Ok(Value::Null)
}

fn body(app: &mut LogocraftApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    let web = d.bool("webOnly");
    let swatches = d.bool("swatches");
    let p = picked(d);
    let original = d.fields.get("original").and_then(color_value).unwrap_or(p.color);
    let mut pick: Option<Picked> = None;
    let mut toggle_swatches = false;
    ui.horizontal_top(|ui| {
        // The picker every colour in the app is edited with (CMYK included).
        ui.vertical(|ui| {
            ui.set_width(260.0);
            let e = crate::panels::fill_picker::color_controls(ui, "cp", p.color, None);
            if let Some((c, _)) = e.color {
                pick = Some(Picked::of(c));
            }
            if e.eyedropper {
                app.select_tool("eyedropper");
            }
        });
        ui.add_space(14.0);
        ui.vertical(|ui| {
            if new_original_chips(ui, &p.color, &original) {
                pick = Some(Picked::of(original));
            }
            ui.add_space(8.0);
            let ws = web_safe(&p.color);
            if !web && !is_web_safe(&p.color) && warning_chip(ui, WEB_WARNING, &ws) {
                pick = Some(Picked::of(ws));
            }
            ui.add_space(8.0);
            toggle_swatches = widgets::flat_button(ui, if swatches { tl!("Hide Swatches") } else { tl!("Color Swatches") }, 150.0).clicked();
            if swatches && let Some(c) = swatch_list(app, ui, &p.color) {
                pick = Some(Picked::of(c));
            }
        });
    });
    ui.add_space(8.0);
    if widgets::check(ui, tl!("Only Web Colors"), web, true) {
        d.fields.insert("webOnly".into(), json!(!web));
    }
    if toggle_swatches {
        d.fields.insert("swatches".into(), json!(!swatches));
    }
    store(d, &pick.unwrap_or(p).snapped(d.bool("webOnly")));
    preview(app, d);
    false
}

/// Show the picked colour on the selected art while the picker is open: each new colour replaces
/// the last preview. Nothing previews until the colour first changes, nor without a selection
/// (the colour then only becomes the default paint, on OK).
fn preview(app: &mut LogocraftApp, d: &mut Dialog) {
    let (cmd, params) = paint_command(app, d);
    let first = !app.session.in_interaction();
    let unchanged = d.fields.get(PREVIEWED) == Some(&params) || first && d.fields.get("original").and_then(color_value) == Some(picked(d).color);
    let selected = app.session.active().is_some_and(|st| !st.selection.objects.is_empty());
    if unchanged || !selected {
        return;
    }
    let _ = app.session.begin_interaction(if d.bool("stroke") { "Stroke Color" } else { "Fill Color" });
    if let Err(e) = app.session.preview(cmd, &params) {
        app.status(e.to_string());
    }
    d.fields.insert(PREVIEWED.into(), params);
}

/// The command that applies the picked colour to the proxy, and its params (as CMYK in a CMYK
/// document).
fn paint_command(app: &LogocraftApp, d: &Dialog) -> (&'static str, Value) {
    let mut c = picked(d).color;
    if matches!(c, Color::Rgb { .. }) && app.session.active().is_some_and(|s| s.doc.color_mode == ColorMode::Cmyk) {
        let [cy, m, y, k] = c.to_cmyk_managed(cms::active().settings().intent);
        c = Color::cmyk(cy, m, y, k);
    }
    (if d.bool("stroke") { "paint.setStroke" } else { "paint.setFill" }, json!({ "color": color_json(&c) }))
}

/// The New (top) and Original (bottom) chips. Returns true when Original is clicked.
fn new_original_chips(ui: &mut Ui, new: &Color, original: &Color) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(64.0, 60.0), Sense::hover());
    let top = Rect::from_min_max(r.min, egui::pos2(r.right(), r.center().y));
    let bottom = Rect::from_min_max(egui::pos2(r.left(), r.center().y), r.max);
    widgets::paint_chip(ui, top, &Paint::solid(*new));
    widgets::paint_chip(ui, bottom, &Paint::solid(*original));
    ui.painter().rect_stroke(r, 0.0, egui::Stroke::new(1.0, t.border), egui::StrokeKind::Outside);
    ui.interact(top, ui.id().with("cp-new"), Sense::hover()).on_hover_text(tl!("New"));
    ui.interact(bottom, ui.id().with("cp-original"), Sense::click()).on_hover_text(tl!("Original: click to restore")).clicked()
}

/// The document's colour swatches; returns the one clicked.
fn swatch_list(app: &LogocraftApp, ui: &mut Ui, current: &Color) -> Option<Color> {
    let t = Tokens::get(ui.ctx());
    let mut chosen = None;
    ui.allocate_ui(vec2(FIELD + 28.0, FIELD), |ui| {
        widgets::list_box(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("cp-swatches").max_height(FIELD).show(ui, |ui| {
                ui.set_width(FIELD + 24.0);
                let Some(st) = app.session.active() else {
                    widgets::dim_label(ui, tl!("Open a document to see its swatches."));
                    return;
                };
                let doc = &st.doc;
                for s in doc.swatches.iter().chain(doc.swatch_groups.iter().flat_map(|g| g.swatches.iter())) {
                    let Paint::Solid { color, .. } = &s.paint else { continue };
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, 0.0, t.hover);
                    }
                    let chip = Rect::from_min_size(r.left_center() + vec2(4.0, -7.0), vec2(14.0, 14.0));
                    widgets::swatch_tile(ui, chip, &s.paint, color == current, false);
                    ui.painter().text(
                        egui::pos2(chip.right() + 8.0, r.center().y),
                        egui::Align2::LEFT_CENTER,
                        &s.name,
                        egui::FontId::proportional(12.0),
                        t.text,
                    );
                    if resp.clicked() {
                        chosen = Some(*color);
                    }
                }
            });
        });
    });
    chosen
}

/// OK: apply the colour to the proxy, keeping the live preview as one undo step.
fn confirm(app: &mut LogocraftApp, d: &Dialog) -> Result<Value, String> {
    let (cmd, params) = paint_command(app, d);
    super::form::commit_preview(app, cmd, params)
}

#[cfg(test)]
mod tests {
    use logocraft_engine::Session;

    use super::*;
    use crate::panels::color::{gamut_fix, in_gamut};

    fn app() -> LogocraftApp {
        let mut app = LogocraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 200, "height": 200})).unwrap();
        app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 50, "height": 50})).unwrap();
        app
    }

    fn frame(app: &mut LogocraftApp) {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut out = ctx.run_ui(Default::default(), |ui| super::super::show(app, ui.ctx()));
        out.textures_delta.clear();
    }

    fn fill_hex(app: &LogocraftApp) -> String {
        crate::panels::current_paints(app).0.color().unwrap().to_hex()
    }

    #[test]
    fn only_web_colors_snaps() {
        let p = Picked::of(Color::rgb8(230, 120, 40));
        assert_eq!(p.snapped(false), p);
        assert_eq!(p.snapped(true).color.to_hex(), "#ff6633");
        assert!(is_web_safe(&p.snapped(true).color));
        let mut d = Dialog::new(KIND, json!({"color": "#e67828", "webOnly": true}));
        assert_eq!(picked(&d).color.to_hex(), "#ff6633");
        d.fields.insert("webOnly".into(), json!(false));
        assert_eq!(picked(&d).color.to_hex(), "#e67828");
    }

    #[test]
    fn gamut_correction_gives_a_printable_colour() {
        let mut app = app();
        let blue = Color::rgb(0.0, 0.0, 1.0);
        assert!(blue.out_of_gamut());
        let fix = in_gamut(&mut app, &blue).expect("pure blue can't be printed");
        assert!(!fix.out_of_gamut(), "{fix:?}");
        assert!(matches!(fix, Color::Rgb { .. }));
        assert_eq!(in_gamut(&mut app, &Color::rgb8(128, 128, 128)), None);
        // The dialog and the Color panel ask through a cache.
        let ctx = egui::Context::default();
        assert_eq!(gamut_fix(&mut app, &ctx, "picker-gamut", &blue), Some(fix));
        assert_eq!(gamut_fix(&mut app, &ctx, "picker-gamut", &blue), Some(fix));
        assert_eq!(gamut_fix(&mut app, &ctx, "picker-gamut", &Color::rgb8(128, 128, 128)), None);
    }

    /// The selected art takes each colour as it is picked; OK keeps the last as one undo step and
    /// Cancel puts the art back.
    #[test]
    fn the_selection_shows_the_colour_live() {
        let mut app = app();
        let undo = |app: &LogocraftApp| app.session.doc().unwrap().history.undo.len();
        let before = undo(&app);
        app.run("ui.colorPicker", json!({})).unwrap();
        frame(&mut app);
        assert!(!app.session.in_interaction(), "nothing previews before the colour changes");
        for hex in ["ff0000", "00ff00"] {
            app.ui.dialog.as_mut().unwrap().fields.insert("hex".into(), json!(hex));
            frame(&mut app);
            assert_eq!(fill_hex(&app), format!("#{hex}"), "live, before OK");
        }
        super::super::confirm(&mut app).unwrap();
        assert_eq!(fill_hex(&app), "#00ff00");
        assert_eq!(undo(&app), before + 1, "one step");
        // Cancel: back to the colour it had.
        app.run("ui.colorPicker", json!({})).unwrap();
        app.ui.dialog.as_mut().unwrap().fields.insert("hex".into(), json!("0000ff"));
        frame(&mut app);
        assert_eq!(fill_hex(&app), "#0000ff");
        super::super::cancel(&mut app);
        assert_eq!(fill_hex(&app), "#00ff00");
        assert_eq!(undo(&app), before + 1);
    }

    #[test]
    fn open_set_hex_and_confirm_fills() {
        let mut app = app();
        app.run("ui.colorPicker", json!({})).unwrap();
        let d = app.ui.dialog.as_ref().unwrap();
        assert_eq!(super::super::DialogKind::of(&d.kind), Some(super::super::DialogKind::ColorPicker));
        assert_eq!(d.str("hex"), "FFFFFF", "starts from the fill");
        assert!(!d.bool("stroke"));
        frame(&mut app);
        // `ui.dialog.set {field: "hex"}` then `ui.dialog.confirm`.
        app.ui.dialog.as_mut().unwrap().fields.insert("hex".into(), json!("00ff00"));
        frame(&mut app);
        assert_eq!(app.ui.dialog.as_ref().unwrap().str("hex"), "00FF00");
        super::super::confirm(&mut app).unwrap();
        assert!(app.ui.dialog.is_none());
        assert_eq!(fill_hex(&app), "#00ff00");
        // Without a frame in between, a set hex still wins; `color` works as well.
        app.run("ui.colorPicker", json!({"stroke": true})).unwrap();
        app.ui.dialog.as_mut().unwrap().fields.insert("hex".into(), json!("#3366CC"));
        super::super::confirm(&mut app).unwrap();
        assert_eq!(crate::panels::current_paints(&app).1.color().unwrap().to_hex(), "#3366cc");
        // The stroke proxy is in front now: the opener follows it unless told otherwise.
        app.run("ui.colorPicker", json!({})).unwrap();
        assert!(app.ui.dialog.as_ref().unwrap().bool("stroke"));
        app.run("ui.colorPicker", json!({"stroke": false})).unwrap();
        app.ui.dialog.as_mut().unwrap().fields.insert("color".into(), json!({"c": 0, "m": 1, "y": 1, "k": 0}));
        super::super::confirm(&mut app).unwrap();
        assert!(matches!(crate::panels::current_paints(&app).0.color(), Some(Color::Cmyk { .. })), "CMYK is kept");
        assert!(app.run("ui.colorPicker", json!({"color": "nope"})).is_err());
        // Colours take every form the paint commands take (CMYK in percent too).
        app.run("ui.colorPicker", json!({"stroke": false, "color": {"c": 0, "m": 100, "y": 100, "k": 0}})).unwrap();
        super::super::confirm(&mut app).unwrap();
        assert_eq!(crate::panels::current_paints(&app).0.color(), Some(Color::cmyk(0.0, 1.0, 1.0, 0.0)));
    }

    #[test]
    fn every_mode_draws_and_cmyk_documents_get_cmyk() {
        let mut app = app();
        for fields in
            [json!({"channel": "red"}), json!({"channel": "brightness", "webOnly": true}), json!({"swatches": true}), json!({"color": "#0000ff"})]
        {
            app.run("ui.colorPicker", json!({})).unwrap();
            let d = app.ui.dialog.as_mut().unwrap();
            for (k, v) in fields.as_object().unwrap() {
                d.fields.insert(k.clone(), v.clone());
            }
            frame(&mut app);
            assert!(app.ui.dialog.is_some());
        }
        app.run("file.documentColorMode", json!({"mode": "cmyk"})).unwrap();
        app.run("ui.colorPicker", json!({"color": "#ff0000"})).unwrap();
        super::super::confirm(&mut app).unwrap();
        assert!(matches!(crate::panels::current_paints(&app).0.color(), Some(Color::Cmyk { .. })));
    }
}
