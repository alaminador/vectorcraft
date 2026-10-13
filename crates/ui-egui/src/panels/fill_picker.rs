//! The Fill/Stroke chip popover, laid out like Figma's fill picker (public documentation:
//! https://help.figma.com/hc/en-us/articles/360041003694): a Custom | Swatches switch, then three
//! kinds of paint, each a click away.
//!
//! - **Color**: a saturation/brightness square, a hue rail and an opacity rail (round handles), the
//!   eyedropper, the value as Hex, RGB, HSL or HSB with the opacity beside it, the contrast against
//!   white with its WCAG level, and the solid colours already used in the document.
//! - **Gradient**: Linear or Radial, Flip and Rotate 90°, a ramp whose stops drag along it (a click
//!   on the ramp adds one), the stop list (position, colour, opacity, remove) and the selected
//!   stop's colour controls.
//! - **Image**: Fill, Fit or Tile (with its scale) and Upload from computer (`paint.imageFill`);
//!   changing the mode of an image fill lays the same picture out again.
//!
//! Every change is a command; a drag is one undo step ([`live_run`]). The opacity of a solid
//! paint is its fill (or stroke) item's opacity, as in the Appearance panel.

use egui::{Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use logocraft_color::gradient::{MIN_STOPS, insert_stop, move_stop, remove_stop};
use logocraft_color::{Color, Gradient, GradientKind, GradientPaint, GradientStop, Paint, cms, keep_model};
use serde_json::{Value, json};

use super::{active_paint, c32, gradient, live_run, pstate, set_pstate};
use crate::theme::Tokens;
use crate::widgets::{self, Live};
use crate::{FilePick, LogocraftApp};

/// The popover's width.
pub(crate) const WIDTH: f32 = 248.0;
/// Panel state: the Swatches side is showing.
const SWATCHES: &str = "fill-picker-swatches";
/// Panel state: the paint kind the user picked (none: the current paint's).
const MODE: &str = "fill-picker-mode";
/// Panel state: the last colour's hue, saturation and brightness, with its hex (a grey keeps the
/// hue it was dragged from).
const HSV: &str = "fill-picker-hsv";
/// Panel state: the value format (an index into [`FORMATS`]).
const FORMAT: &str = "fill-picker-format";
/// Panel state: the Image tab's mode (an index into [`IMAGE_MODES`]) and tile scale.
const IMAGE_MODE: &str = "fill-picker-image-mode";
const IMAGE_SCALE: &str = "fill-picker-image-scale";
/// Panel state: a stop drag's starting stops and stop.
const STOP_DRAG: &str = "fill-picker-stop-drag";
/// Panel state: the document's solid colours, per document revision.
const PAGE_COLORS: &str = "fill-picker-page-colors";

const FORMATS: [&str; 5] = ["Hex", "RGB", "HSL", "HSB", "CMYK"];
/// The CMYK entry of [`FORMATS`].
const CMYK: usize = 4;
/// Total ink coverage above this (%) is flagged: most presses limit it to 280–320%.
const INK_LIMIT: f32 = 300.0;
const IMAGE_MODES: [&str; 3] = ["Fill", "Fit", "Tile"];
/// The grey a new paint starts from when there is no colour to start from (Figma's default fill).
const DEFAULT_GREY: Color = Color::Rgb { r: 0xd9 as f32 / 255.0, g: 0xd9 as f32 / 255.0, b: 0xd9 as f32 / 255.0 };
/// The most document colours listed.
const MAX_PAGE_COLORS: usize = 24;

/// The kinds of paint the picker makes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Color,
    Gradient,
    Image,
}

/// The tab a paint opens on: its own kind (None and other patterns open on Color).
pub(crate) fn mode_of(p: &Paint) -> Mode {
    match p {
        Paint::Gradient(_) => Mode::Gradient,
        Paint::Pattern { pattern, .. } if logocraft_doc::pattern::is_image_fill(pattern) => Mode::Image,
        _ => Mode::Color,
    }
}

/// Forget the kind the user picked: the picker opens on the current paint's (a chip click).
pub(crate) fn reset(ctx: &egui::Context) {
    set_pstate(ctx, MODE, None::<Mode>);
}

/// The command that paints the active proxy.
fn paint_cmd(app: &LogocraftApp) -> &'static str {
    if app.session.fill_active { "paint.setFill" } else { "paint.setStroke" }
}

/// The popover body.
pub(crate) fn show(app: &mut LogocraftApp, ui: &mut Ui) {
    ui.set_width(WIDTH);
    let ctx = ui.ctx().clone();
    let swatches: bool = pstate(&ctx, SWATCHES);
    if let Some(i) = widgets::tab_bar(ui, &[tl!("Custom"), tl!("Swatches")], usize::from(swatches)) {
        set_pstate(&ctx, SWATCHES, i == 1);
    }
    widgets::divider(ui);
    if swatches {
        super::swatches::popover(app, ui);
        return;
    }
    let paint = active_paint(app);
    let mode = pstate::<Option<Mode>>(&ctx, MODE).unwrap_or_else(|| mode_of(&paint));
    ui.horizontal(|ui| {
        for (m, icon, tip) in [(Mode::Color, "square", "Color"), (Mode::Gradient, "dc-gradient", "Gradient"), (Mode::Image, "image", "Image")] {
            if widgets::icon_button(ui, icon, tl!(tip), mode == m, 24.0).clicked() && mode != m {
                switch_mode(app, &ctx, &paint, m);
            }
        }
    });
    ui.add_space(6.0);
    match (mode, &paint) {
        (Mode::Gradient, Paint::Gradient(g)) => gradient_mode(app, ui, g),
        (Mode::Image, _) => image_mode(app, ui, &paint),
        _ => color_mode(app, ui, &paint),
    }
}

/// Switch the active proxy's paint to kind `m`, starting from the colour it has.
fn switch_mode(app: &mut LogocraftApp, ctx: &egui::Context, paint: &Paint, m: Mode) {
    let base = match paint {
        Paint::Solid { color, .. } => *color,
        Paint::Gradient(g) => {
            let i = app.session.selected_stop().unwrap_or(0);
            g.gradient.stops.get(i).or(g.gradient.stops.first()).map(|s| s.color).unwrap_or(DEFAULT_GREY)
        }
        _ => remembered(ctx).map(|(hex, _)| hex).and_then(|h| Color::from_hex(&h)).unwrap_or(DEFAULT_GREY),
    };
    let cmd = paint_cmd(app);
    match m {
        Mode::Color => {
            set_pstate(ctx, MODE, None::<Mode>);
            app.run(cmd, json!({ "color": base.to_hex(), "focus": true })).ok();
        }
        Mode::Gradient => {
            set_pstate(ctx, MODE, None::<Mode>);
            let mut params = super::paint_params(&Paint::Gradient(Box::new(GradientPaint::new(default_gradient(base)))));
            params["focus"] = json!(true);
            app.run(cmd, params).ok();
        }
        // An image fill needs a picture: the tab shows the upload button until there is one.
        Mode::Image => set_pstate(ctx, MODE, Some(Mode::Image)),
    }
}

/// A new gradient from colour `c`: `c` fading out, as Figma starts one.
pub(crate) fn default_gradient(c: Color) -> Gradient {
    let mut end = GradientStop::new(1.0, c);
    end.opacity = 0.0;
    Gradient { kind: GradientKind::Linear, stops: vec![GradientStop::new(0.0, c), end] }
}

// ---------- Color ----------

/// The index of the active proxy's item (the topmost fill, or stroke) in the first selected
/// object's appearance stack, for its opacity.
fn item_index(app: &LogocraftApp) -> Option<usize> {
    let stroke = !app.session.fill_active;
    super::first_node(app)?.appearance.items.iter().rposition(|i| i.is_fill() != stroke)
}

/// That item's opacity (0..1).
fn item_opacity(app: &LogocraftApp) -> Option<f32> {
    let i = item_index(app)?;
    super::first_node(app)?.appearance.items.get(i).map(|it| it.opacity())
}

fn color_mode(app: &mut LogocraftApp, ui: &mut Ui, paint: &Paint) {
    let c = match paint {
        Paint::Solid { color, .. } => *color,
        _ => DEFAULT_GREY,
    };
    let alpha = item_opacity(app);
    let e = color_controls(ui, "fill", c, alpha);
    if e.eyedropper {
        let stroke = paint_cmd(app) == "paint.setStroke";
        start_pick(app, "paint.sampleColor", json!({ "stroke": stroke }), &["color"]);
    }
    if let Some((nc, phase)) = e.color {
        let cmd = paint_cmd(app);
        live_run(app, "Color", cmd, json!({ "color": super::color_json(&nc), "focus": true }), phase);
    }
    if let (Some((a, phase)), Some(index)) = (e.alpha, item_index(app)) {
        live_run(app, "Opacity", "appearance.setItem", json!({ "index": index, "opacity": (a * 100.0).round() }), phase);
    }
    ui.add_space(6.0);
    contrast_row(ui, c);
    let colors = page_colors(app, ui.ctx());
    if !colors.is_empty() {
        ui.add_space(6.0);
        widgets::subheader(ui, tl!("On this page"));
        if let Some(pick) = color_chips(ui, &colors) {
            let cmd = paint_cmd(app);
            app.run(cmd, json!({ "color": super::color_json(&pick), "focus": true })).ok();
        }
    }
}

/// What the colour controls changed this frame.
#[derive(Default)]
pub(crate) struct ColorEdit {
    pub(crate) color: Option<(Color, Live)>,
    pub(crate) alpha: Option<(f32, Live)>,
    pub(crate) eyedropper: bool,
}

/// The last colour's HSB and its hex.
fn remembered(ctx: &egui::Context) -> Option<(String, [f32; 3])> {
    pstate::<Option<(String, [f32; 3])>>(ctx, HSV)
}

fn remember(ctx: &egui::Context, c: Color, hsv: [f32; 3]) {
    set_pstate(ctx, HSV, Some((c.to_hex(), hsv)));
}

/// `c` as hue, saturation and brightness, keeping the remembered hue (and saturation) where `c`
/// has none: a grey or black dragged from a colour keeps that colour's hue on the rail.
/// The eyedropper: the next click on the canvas samples the colour there (what shows, art and
/// artboard fill alike) and runs `cmd` with `params`, the colour at `key`, as Figma's eyedropper
/// does; Escape cancels.
pub(crate) fn start_pick(app: &mut LogocraftApp, cmd: &str, params: Value, key: &[&str]) {
    app.ui.color_pick = Some(crate::state::ColorPick { cmd: cmd.into(), params, key: key.iter().map(|k| k.to_string()).collect() });
    app.status(tl!("Click the canvas to pick a colour (Esc cancels)"));
}

/// `pick`'s params with colour `hex` at its key.
pub(crate) fn picked_params(pick: &crate::state::ColorPick, hex: &str) -> Value {
    let mut p = pick.params.clone();
    if !p.is_object() {
        p = json!({});
    }
    let mut at = &mut p;
    for (n, k) in pick.key.iter().enumerate() {
        let Some(o) = at.as_object_mut() else { break };
        if n + 1 == pick.key.len() {
            o.insert(k.clone(), json!(hex));
            break;
        }
        at = o.entry(k.clone()).or_insert_with(|| json!({}));
    }
    p
}

pub(crate) fn hsv_of(c: Color, last: Option<(String, [f32; 3])>) -> [f32; 3] {
    let hex = c.to_hex();
    if let Some((h, hsv)) = &last
        && *h == hex
    {
        return *hsv;
    }
    let [h, s, v] = c.to_hsb();
    match last {
        Some((_, [lh, ls, _])) if v <= 0.0 => [lh, ls, 0.0],
        Some((_, [lh, _, _])) if s <= 0.0 => [lh, 0.0, v],
        _ => [h, s, v],
    }
}

/// The square, the rails and the value fields for colour `c` (with opacity `alpha`, when it
/// applies): the picker every colour in the app is edited with. `id` keeps two sets apart. A
/// CMYK colour stays CMYK whatever is dragged (its numbers follow); the CMYK format shows its
/// inks, their total coverage, the gamut warning for a colour that won't print as it shows, and
/// rich black.
pub(crate) fn color_controls(ui: &mut Ui, id: &str, c: Color, alpha: Option<f32>) -> ColorEdit {
    let ctx = ui.ctx().clone();
    let [h, s, v] = hsv_of(c, remembered(&ctx));
    let width = ui.available_width();
    let mut e = ColorEdit::default();
    let (pos, phase) = widgets::color_field(ui, ("fp-sv", id), vec2(width, 180.0), (s, 1.0 - v), &|x, y| c32(&Color::from_hsb(h, x, 1.0 - y)));
    if let Some((x, y)) = pos {
        let nc = Color::from_hsb(h, x, 1.0 - y);
        remember(&ctx, nc, [h, x, 1.0 - y]);
        e.color = Some((nc, phase));
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        e.eyedropper = widgets::icon_button(ui, "pipette", tl!("Pick a color from the canvas"), false, 26.0).clicked();
        ui.vertical(|ui| {
            let w = ui.available_width();
            let (t, phase) = rail(ui, ("fp-hue", id), h / 360.0, w, &|t| c32(&Color::from_hsb(t * 360.0, 1.0, 1.0)), false);
            if let Some(t) = t {
                let nh = (t * 360.0).min(359.9);
                let nc = Color::from_hsb(nh, s, v);
                remember(&ctx, nc, [nh, s, v]);
                e.color = Some((nc, phase));
            }
            if let Some(a) = alpha {
                let [r, g, b, _] = c.to_rgba8(1.0);
                let (t, phase) = rail(ui, ("fp-alpha", id), a, w, &|t| Color32::from_rgba_unmultiplied(r, g, b, (t * 255.0).round() as u8), true);
                if let Some(t) = t {
                    e.alpha = Some((t, phase));
                }
            }
        });
    });
    ui.add_space(6.0);
    // The format: the one picked, else CMYK for a CMYK colour, else Hex.
    let f = pstate::<Option<usize>>(&ctx, FORMAT).unwrap_or(if c.model() == cms::Model::Cmyk { CMYK } else { 0 }).min(FORMATS.len() - 1);
    let mut typed_cmyk = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if let Some(i) = widgets::dropdown(ui, ("fp-format", id), FORMATS[f], &FORMATS, 56.0) {
            set_pstate(&ctx, FORMAT, Some(i));
        }
        if f == CMYK {
            if let Some(nc) = cmyk_fields(ui, id, c) {
                remember(&ctx, nc, hsv_of(nc, None));
                e.color = Some((nc, Live::Released));
                typed_cmyk = true;
            }
        } else {
            if let Some(nc) = value_fields(ui, id, f, c, [h, s, v]) {
                remember(&ctx, nc, hsv_of(nc, None));
                e.color = Some((nc, Live::Released));
            }
            if let Some(a) = alpha
                && let Some(pct) = widgets::plain_field(ui, ("fp-alpha-field", id), f64::from(a * 100.0).round(), "%", 0, 46.0)
            {
                e.alpha = Some(((pct.clamp(0.0, 100.0) / 100.0) as f32, Live::Released));
            }
        }
    });
    if f == CMYK
        && let Some(nc) = cmyk_tools(ui, id, c, alpha, &mut e)
    {
        e.color = Some((nc, Live::Released));
        typed_cmyk = true;
    }
    // A CMYK colour stays CMYK (the square and the rails work in RGB).
    if !typed_cmyk
        && c.model() == cms::Model::Cmyk
        && let Some((nc, _)) = e.color.as_mut()
    {
        *nc = keep_model(c, *nc);
    }
    e
}

/// The ink of `c` (0..1 each): its own numbers when it is CMYK, else the working CMYK space's.
pub(crate) fn inks(c: Color) -> [f32; 4] {
    match c {
        Color::Cmyk { c, m, y, k } => [c, m, y, k],
        _ => c.to_cmyk_managed(cms::Intent::default()),
    }
}

/// C, M, Y and K fields (%); returns the CMYK colour typed.
fn cmyk_fields(ui: &mut Ui, id: &str, c: Color) -> Option<Color> {
    let mut v = inks(c);
    let mut changed = false;
    for (i, k) in ["C", "M", "Y", "K"].iter().enumerate() {
        let Some(slot) = v.get_mut(i) else { continue };
        let resp = widgets::plain_field(ui, ("fp-cmyk", id, *k), f64::from(*slot * 100.0).round(), "", 0, 34.0);
        if let Some(n) = resp {
            *slot = (n.clamp(0.0, 100.0) / 100.0) as f32;
            changed = true;
        }
    }
    changed.then(|| Color::cmyk(v[0], v[1], v[2], v[3]))
}

/// Below the CMYK fields: their labels, the total ink coverage, opacity, the gamut warning (snap
/// to CMYK) and rich black. Returns a colour a button made.
fn cmyk_tools(ui: &mut Ui, id: &str, c: Color, alpha: Option<f32>, e: &mut ColorEdit) -> Option<Color> {
    let t = Tokens::get(ui.ctx());
    let mut out = None;
    let [cc, mm, yy, kk] = inks(c);
    let total = (cc + mm + yy + kk) * 100.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.add_space(60.0);
        for k in ["C", "M", "Y", "K"] {
            ui.add_sized([34.0, 14.0], egui::Label::new(egui::RichText::new(k).size(10.5).color(t.text_dim)));
        }
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let over = total > INK_LIMIT;
        let color = if over { t.warning } else { t.text_dim };
        ui.label(egui::RichText::new(format!("{} {total:.0}%", tl!("Ink"))).size(11.5).color(color))
            .on_hover_text(tl!("Total ink coverage (C + M + Y + K). Most presses want 300% or less; above it, ink can smear or dry badly."));
        if let Some(a) = alpha
            && let Some(pct) = widgets::plain_field(ui, ("fp-alpha-field", id), f64::from(a * 100.0).round(), "%", 0, 46.0)
        {
            e.alpha = Some(((pct.clamp(0.0, 100.0) / 100.0) as f32, Live::Released));
        }
    });
    if c.model() != cms::Model::Cmyk && c.out_of_gamut() {
        let resp = ui.add(egui::Button::new(
            egui::RichText::new(format!("⚠ {}", tl!("Out of CMYK gamut: use the closest printable colour"))).size(11.0).color(t.warning),
        ));
        if resp.on_hover_text(tl!("This colour can't be printed as it shows. Click to replace it with its CMYK equivalent.")).clicked() {
            let [c2, m2, y2, k2] = c.to_cmyk_managed(cms::Intent::default());
            out = Some(Color::cmyk(c2, m2, y2, k2));
        }
    } else if c.model() != cms::Model::Cmyk && widgets::flat_button(ui, tl!("Convert to CMYK"), ui.available_width()).clicked() {
        let [c2, m2, y2, k2] = inks(c);
        out = Some(Color::cmyk(c2, m2, y2, k2));
    }
    // Plain 100% K prints greyish in big areas: rich black adds the other inks under it.
    if kk >= 0.99 && cc + mm + yy < 0.15 && widgets::flat_button(ui, tl!("Rich black (C60 M40 Y40 K100)"), ui.available_width()).clicked() {
        out = Some(Color::cmyk(0.6, 0.4, 0.4, 1.0));
    }
    out
}

/// The value fields of format `f` (an index into [`FORMATS`]); returns the colour typed.
fn value_fields(ui: &mut Ui, id: &str, f: usize, c: Color, [h, s, v]: [f32; 3]) -> Option<Color> {
    let field = |ui: &mut Ui, k: &str, value: f32, suffix: &str| {
        widgets::plain_field(ui, ("fp-val", id, k.to_string()), f64::from(value).round(), suffix, 0, 32.0)
    };
    match f {
        1 => {
            let [r, g, b, _] = c.to_rgba8(1.0);
            let mut rgb = [r, g, b];
            let mut changed = false;
            for (i, k) in ["r", "g", "b"].iter().enumerate() {
                if let Some(n) = field(ui, k, f32::from(rgb[i]), "") {
                    rgb[i] = n.clamp(0.0, 255.0).round() as u8;
                    changed = true;
                }
            }
            changed.then(|| Color::rgb8(rgb[0], rgb[1], rgb[2]))
        }
        2 => {
            let [hh, ss, ll] = hsl_of(c, h);
            let mut hsl = [hh, ss * 100.0, ll * 100.0];
            let mut changed = false;
            for (i, (k, suffix)) in [("h", "°"), ("s", "%"), ("l", "%")].iter().enumerate() {
                if let Some(n) = field(ui, k, hsl[i], suffix) {
                    hsl[i] = n as f32;
                    changed = true;
                }
            }
            changed.then(|| from_hsl(hsl[0].rem_euclid(360.0), (hsl[1] / 100.0).clamp(0.0, 1.0), (hsl[2] / 100.0).clamp(0.0, 1.0)))
        }
        3 => {
            let mut hsb = [h, s * 100.0, v * 100.0];
            let mut changed = false;
            for (i, (k, suffix)) in [("h", "°"), ("s", "%"), ("b", "%")].iter().enumerate() {
                if let Some(n) = field(ui, k, hsb[i], suffix) {
                    hsb[i] = n as f32;
                    changed = true;
                }
            }
            changed.then(|| Color::from_hsb(hsb[0].rem_euclid(360.0), (hsb[1] / 100.0).clamp(0.0, 1.0), (hsb[2] / 100.0).clamp(0.0, 1.0)))
        }
        _ => {
            let hex = c.to_hex().trim_start_matches('#').to_ascii_uppercase();
            widgets::hex_field(ui, ("fp-hex", id), &hex).and_then(|t| Color::from_hex(&t))
        }
    }
}

/// `c` as hue (degrees; `hue` where `c` is grey), saturation and lightness (0..1).
pub(crate) fn hsl_of(c: Color, hue: f32) -> [f32; 3] {
    let [r, g, b] = c.to_rgb();
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d <= 1e-6 {
        return [hue, 0.0, l];
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6);
    let [h, _, _] = c.to_hsb();
    [h, s.clamp(0.0, 1.0), l]
}

/// The colour of hue `h` (degrees), saturation `s` and lightness `l` (0..1).
pub(crate) fn from_hsl(h: f32, s: f32, l: f32) -> Color {
    let v = l + s * l.min(1.0 - l);
    let sv = if v <= 0.0 { 0.0 } else { 2.0 * (1.0 - l / v) };
    Color::from_hsb(h, sv, v)
}

/// A rounded rail with a round handle at `value` (0..1): `track(t)` gives its colour, over a
/// checkerboard when `checker` (an opacity rail). Returns the new value and the phase.
fn rail(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: f32,
    width: f32,
    track: &dyn Fn(f32) -> Color32,
    checker: bool,
) -> (Option<f32>, Live) {
    const H: f32 = 12.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, H + 4.0), Sense::hover());
    let resp = ui.interact(rect, ui.id().with(id), Sense::click_and_drag());
    let bar = Rect::from_center_size(rect.center(), vec2(width, H));
    let r = H / 2.0;
    let inner = bar.shrink2(vec2(r, 0.0));
    let painter = ui.painter_at(rect.expand(4.0));
    if checker {
        painter.rect_filled(bar, r, Color32::WHITE);
        let cell = H / 2.0;
        let (cols, rows) = ((inner.width() / cell).ceil() as i32, 2);
        for j in 0..rows {
            for i in 0..cols {
                if (i + j) % 2 == 0 {
                    let min = pos2(inner.left() + i as f32 * cell, inner.top() + j as f32 * cell);
                    let cr = Rect::from_min_size(min, vec2(cell, cell)).intersect(inner);
                    painter.rect_filled(cr, 0.0, Color32::from_gray(204));
                }
            }
        }
    }
    painter.circle_filled(pos2(inner.left(), bar.center().y), r, track(0.0));
    painter.circle_filled(pos2(inner.right(), bar.center().y), r, track(1.0));
    painter.add(widgets::color_mesh(inner, (48, 1), &|x, _| track(x)));
    let v = value.clamp(0.0, 1.0);
    let c = pos2(inner.left() + v * inner.width(), bar.center().y);
    let shown = track(v);
    let opaque = Color32::from_rgb(shown.r(), shown.g(), shown.b());
    painter.circle_filled(c, r + 1.0, if checker { Color32::WHITE } else { opaque });
    if checker {
        painter.circle_filled(c, r - 1.0, shown);
    }
    painter.circle_stroke(c, r + 1.0, Stroke::new(2.0, Color32::WHITE));
    painter.circle_stroke(c, r + 2.0, Stroke::new(1.0, Color32::from_black_alpha(90)));
    match widgets::pointer_phase(&resp) {
        Some((p, phase)) => (Some(((p.x - inner.left()) / inner.width().max(1.0)).clamp(0.0, 1.0)), phase),
        None => (None, Live::Idle),
    }
}

/// WCAG relative luminance of `c` (0..1).
fn luminance(c: Color) -> f32 {
    let lin = |v: f32| if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    let [r, g, b] = c.to_rgb();
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// The WCAG contrast ratio of `a` against `b` (1..21).
pub(crate) fn contrast_ratio(a: Color, b: Color) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The WCAG 2 level a contrast ratio passes for normal text, if any.
pub(crate) fn wcag_level(ratio: f32) -> Option<&'static str> {
    if ratio >= 7.0 {
        Some("AAA")
    } else if ratio >= 4.5 {
        Some("AA")
    } else if ratio >= 3.0 {
        Some("AA Large")
    } else {
        None
    }
}

/// "Contrast 4.6:1 on white" and its WCAG badge.
fn contrast_row(ui: &mut Ui, c: Color) {
    let t = Tokens::get(ui.ctx());
    // Shown rounded down, so 2.99:1 doesn't read "3.0:1" next to a failing badge.
    let ratio = (contrast_ratio(c, Color::WHITE) * 10.0).floor() / 10.0;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{} {ratio:.1}:1 {}", tl!("Contrast"), tl!("on white"))).size(11.5).color(t.text_dim));
        let (text, color) = match wcag_level(ratio) {
            Some(level) => (level, t.accent),
            None => ("Fails", t.warning),
        };
        egui::Frame::NONE
            .stroke(Stroke::new(1.0, color))
            .corner_radius(3)
            .inner_margin(egui::Margin::symmetric(4, 1))
            .show(ui, |ui| ui.label(egui::RichText::new(tl!(text)).size(10.5).color(color)));
    })
    .response
    .on_hover_text(tl!("WCAG 2 contrast for normal text: AA needs 4.5:1, AAA 7:1, large text 3:1"));
}

/// The solid colours the document uses (fills, strokes and gradient stops), cached per revision.
fn page_colors(app: &LogocraftApp, ctx: &egui::Context) -> Vec<Color> {
    let Some(st) = app.session.active() else { return vec![] };
    let key = (st.uid, st.revision);
    if let Some((k, v)) = pstate::<Option<((u64, u64), Vec<Color>)>>(ctx, PAGE_COLORS)
        && k == key
    {
        return v;
    }
    let mut out: Vec<Color> = vec![];
    let mut add = |c: Color| {
        if out.len() < MAX_PAGE_COLORS && !out.iter().any(|o| o.to_hex() == c.to_hex()) {
            out.push(c);
        }
    };
    st.doc.walk(|n| {
        for it in &n.appearance.items {
            let paint = match it {
                logocraft_doc::AppearanceItem::Fill(f) => &f.paint,
                logocraft_doc::AppearanceItem::Stroke(s) => &s.paint,
            };
            match paint {
                Paint::Solid { color, .. } => add(*color),
                Paint::Gradient(g) => g.gradient.stops.iter().for_each(|s| add(s.color)),
                _ => {}
            }
        }
    });
    set_pstate(ctx, PAGE_COLORS, Some((key, out.clone())));
    out
}

/// A wrapping row of colour chips; returns the one clicked.
fn color_chips(ui: &mut Ui, colors: &[Color]) -> Option<Color> {
    let mut chosen = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for c in colors {
            let (rect, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
            let resp = resp.on_hover_text(c.to_hex().to_ascii_uppercase());
            widgets::swatch_tile(ui, rect, &Paint::solid(*c), false, resp.hovered());
            if resp.clicked() {
                chosen = Some(*c);
            }
        }
    });
    chosen
}

// ---------- Gradient ----------

/// Edit the active proxy's gradient (`paint.editGradient` params), live while dragging.
fn edit_gradient(app: &mut LogocraftApp, mut params: Value, phase: Live) {
    params["stroke"] = json!(!app.session.fill_active);
    live_run(app, "Gradient", "paint.editGradient", params, phase);
}

fn gradient_mode(app: &mut LogocraftApp, ui: &mut Ui, gp: &GradientPaint) {
    let g = &gp.gradient;
    let selected = app.session.selected_stop().unwrap_or(0).min(g.stops.len().saturating_sub(1));
    ui.horizontal(|ui| {
        let kinds = ["Linear", "Radial"];
        let current = if g.kind == GradientKind::Radial { "Radial" } else { "Linear" };
        if let Some(i) = widgets::dropdown(ui, "fp-grad-kind", current, &kinds, 96.0) {
            edit_gradient(app, json!({ "kind": kinds[i].to_ascii_lowercase() }), Live::Released);
        }
        if widgets::icon_button(ui, "arrow-left-right", tl!("Flip"), false, 24.0).clicked() {
            edit_gradient(app, json!({ "reverse": true }), Live::Released);
        }
        if widgets::icon_button(ui, "rotate-cw", tl!("Rotate 90°"), false, 24.0).clicked() {
            edit_gradient(app, json!({ "angle": rotate_90(gp.angle) }), Live::Released);
        }
    });
    ui.add_space(8.0);
    ramp(app, ui, g, selected);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        widgets::subheader(ui, tl!("Stops"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, "plus", tl!("Add a stop"), false, 22.0).clicked() {
                let (stops, i) = insert_stop(g, widest_gap(&g.stops));
                gradient::set_stops(app, &stops, Some(i), Live::Released);
            }
        });
    });
    for i in 0..g.stops.len() {
        stop_row(app, ui, g, i, i == selected);
    }
    if let Some(stop) = g.stops.get(selected) {
        ui.add_space(8.0);
        let e = color_controls(ui, "stop", stop.color, Some(stop.opacity));
        if e.eyedropper {
            let stroke = paint_cmd(app) == "paint.setStroke";
            start_pick(app, "paint.sampleColor", json!({ "stroke": stroke, "stop": selected }), &["color"]);
        }
        let mut stops = g.stops.clone();
        let mut phase = None;
        if let (Some((c, p)), Some(s)) = (e.color, stops.get_mut(selected)) {
            s.color = c;
            s.swatch = None;
            phase = Some(p);
        }
        if let (Some((a, p)), Some(s)) = (e.alpha, stops.get_mut(selected)) {
            s.opacity = a;
            phase = Some(p);
        }
        if let Some(p) = phase {
            gradient::set_stops(app, &stops, Some(selected), p);
        }
    }
}

/// An angle turned 90° clockwise, kept within −180..180.
pub(crate) fn rotate_90(angle: f64) -> f64 {
    let a = (angle - 90.0).rem_euclid(360.0);
    if a > 180.0 { a - 360.0 } else { a }
}

/// The middle of the widest gap between stops (where a new stop goes).
pub(crate) fn widest_gap(stops: &[GradientStop]) -> f32 {
    let mut best = (0.0f32, 0.5f32);
    for w in stops.windows(2) {
        if let [a, b] = w {
            let gap = b.offset - a.offset;
            if gap > best.0 {
                best = (gap, (a.offset + b.offset) / 2.0);
            }
        }
    }
    best.1
}

/// The gradient ramp with its stop handles: drag a handle to move its stop, click the ramp to
/// add one.
fn ramp(app: &mut LogocraftApp, ui: &mut Ui, g: &Gradient, selected: usize) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
    let bar = rect.shrink2(vec2(8.0, 6.0));
    let bar_resp = ui.interact(bar, ui.id().with("fp-ramp"), Sense::click());
    let painter = ui.painter_at(rect.expand(2.0));
    let cell = bar.height() / 2.0;
    painter.rect_filled(bar, 3.0, Color32::WHITE);
    for j in 0..2 {
        for i in 0..((bar.width() / cell).ceil() as i32) {
            if (i + j) % 2 == 0 {
                let cr = Rect::from_min_size(pos2(bar.left() + i as f32 * cell, bar.top() + j as f32 * cell), vec2(cell, cell)).intersect(bar);
                painter.rect_filled(cr, 0.0, Color32::from_gray(204));
            }
        }
    }
    painter.add(widgets::color_mesh(bar, (64, 1), &|x, _| {
        let (c, a) = g.sample(x);
        let [r, gg, b, _] = c.to_rgba8(1.0);
        Color32::from_rgba_unmultiplied(r, gg, b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
    }));
    painter.rect_stroke(bar, 3.0, Stroke::new(1.0, t.border), StrokeKind::Outside);
    let off = |x: f32| gradient::x_to_offset(x, bar.left(), bar.width());
    let mut handled = false;
    for (i, stop) in g.stops.iter().enumerate() {
        let c = pos2(bar.left() + stop.offset.clamp(0.0, 1.0) * bar.width(), bar.center().y);
        let hr = Rect::from_center_size(c, vec2(14.0, 14.0));
        let resp = ui.interact(hr, ui.id().with(("fp-stop", i)), Sense::click_and_drag()).on_hover_text(format!("{:.0}%", stop.offset * 100.0));
        painter.rect_filled(hr, 3.0, Color32::WHITE);
        painter.rect_filled(hr.shrink(2.0), 2.0, c32(&stop.color));
        let ring = if i == selected { t.accent } else { Color32::from_black_alpha(110) };
        painter.rect_stroke(hr, 3.0, Stroke::new(if i == selected { 2.0 } else { 1.0 }, ring), StrokeKind::Outside);
        if resp.drag_started() {
            set_pstate(&ctx, STOP_DRAG, Some((g.stops.clone(), i)));
        }
        if resp.clicked() {
            handled = true;
            app.run("gradient.selectStop", json!({ "index": i })).ok();
        } else if let Some((p, phase)) = widgets::pointer_phase(&resp) {
            handled = true;
            let (origin, k) = pstate::<Option<(Vec<GradientStop>, usize)>>(&ctx, STOP_DRAG).unwrap_or_else(|| (g.stops.clone(), i));
            let (stops, j) = move_stop(&origin, k, off(p.x));
            gradient::set_stops(app, &stops, Some(j), phase);
            if phase == Live::Released {
                set_pstate(&ctx, STOP_DRAG, None::<(Vec<GradientStop>, usize)>);
            }
        }
    }
    if !handled
        && bar_resp.clicked()
        && let Some(p) = bar_resp.interact_pointer_pos()
    {
        let (stops, i) = insert_stop(g, off(p.x));
        gradient::set_stops(app, &stops, Some(i), Live::Released);
    }
}

/// One row of the stop list: position, colour, opacity and remove.
fn stop_row(app: &mut LogocraftApp, ui: &mut Ui, g: &Gradient, i: usize, selected: bool) {
    let Some(stop) = g.stops.get(i) else { return };
    let t = Tokens::get(ui.ctx());
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if let Some(pct) = widgets::plain_field(ui, ("fp-stop-pos", i), f64::from(stop.offset * 100.0).round(), "%", 0, 44.0) {
            let (stops, j) = move_stop(&g.stops, i, (pct.clamp(0.0, 100.0) / 100.0) as f32);
            gradient::set_stops(app, &stops, Some(j), Live::Released);
        }
        let (chip, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
        widgets::swatch_tile(ui, chip, &Paint::solid(stop.color), selected, resp.hovered());
        if resp.clicked() {
            app.run("gradient.selectStop", json!({ "index": i })).ok();
        }
        let hex = stop.color.to_hex().trim_start_matches('#').to_ascii_uppercase();
        if let Some(c) = widgets::hex_field(ui, ("fp-stop-hex", i), &hex).and_then(|h| Color::from_hex(&h)) {
            let mut stops = g.stops.clone();
            if let Some(s) = stops.get_mut(i) {
                s.color = c;
                s.swatch = None;
            }
            gradient::set_stops(app, &stops, Some(i), Live::Released);
        }
        if let Some(pct) = widgets::plain_field(ui, ("fp-stop-alpha", i), f64::from(stop.opacity * 100.0).round(), "%", 0, 46.0) {
            let mut stops = g.stops.clone();
            if let Some(s) = stops.get_mut(i) {
                s.opacity = (pct.clamp(0.0, 100.0) / 100.0) as f32;
            }
            gradient::set_stops(app, &stops, Some(i), Live::Released);
        }
        if g.stops.len() > MIN_STOPS
            && widgets::icon_button(ui, "minus", tl!("Remove stop"), false, 20.0).clicked()
            && let Some(stops) = remove_stop(&g.stops, i)
        {
            gradient::set_stops(app, &stops, Some(i.min(stops.len().saturating_sub(1))), Live::Released);
        }
    });
    if selected {
        ui.painter().rect_stroke(row.response.rect.expand(1.0), 2.0, Stroke::new(1.0, t.accent.gamma_multiply(0.5)), StrokeKind::Outside);
    }
}

// ---------- Image ----------

fn image_mode(app: &mut LogocraftApp, ui: &mut Ui, paint: &Paint) {
    let ctx = ui.ctx().clone();
    let t = Tokens::get(&ctx);
    let current = match paint {
        Paint::Pattern { pattern, .. } if logocraft_doc::pattern::is_image_fill(pattern) => Some(pattern.clone()),
        _ => None,
    };
    let stroke = !app.session.fill_active;
    // An image fill shows how it is laid out; before there is one, the mode the next upload uses.
    let shown = current.as_deref().and_then(|name| {
        let st = app.session.active()?;
        let bounds = super::first_node(app)?.geometric_bounds()?;
        infer_layout(st.doc.pattern(name)?, bounds)
    });
    let (mode, scale) = shown.unwrap_or_else(|| {
        let scale = match pstate::<f64>(&ctx, IMAGE_SCALE) {
            s if s > 0.0 => s,
            _ => 100.0,
        };
        (pstate::<usize>(&ctx, IMAGE_MODE).min(IMAGE_MODES.len() - 1), scale)
    });
    let mut relayout = false;
    let mut mode_now = mode;
    let mut scale_now = scale;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if let Some(i) = widgets::dropdown(ui, "fp-image-mode", IMAGE_MODES[mode], &IMAGE_MODES, 96.0) {
            set_pstate(&ctx, IMAGE_MODE, i);
            mode_now = i;
            relayout = true;
        }
        if mode_now == 2
            && let Some(s) = widgets::plain_field(ui, "fp-image-scale", scale, "%", 0, 52.0)
        {
            let s = s.clamp(1.0, 10_000.0);
            set_pstate(&ctx, IMAGE_SCALE, s);
            scale_now = s;
            relayout = true;
        }
    });
    let mode_name = IMAGE_MODES[mode_now].to_ascii_lowercase();
    if relayout && current.is_some() {
        app.run("paint.imageFill", json!({ "mode": mode_name, "scale": scale_now, "stroke": stroke })).ok();
    }
    ui.add_space(8.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 96.0), Sense::hover());
    ui.painter().rect_filled(rect, 4.0, t.panel_darker);
    ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, t.border), StrokeKind::Inside);
    let caption = match &current {
        Some(name) => format!("{name} · {}", tl!(IMAGE_MODES[mode_now])),
        None => tl!("No image yet").to_string(),
    };
    crate::icons::paint(ui, "image", Rect::from_center_size(rect.center() - vec2(0.0, 10.0), vec2(26.0, 26.0)), t.text_disabled);
    ui.painter().text(rect.center() + vec2(0.0, 18.0), egui::Align2::CENTER_CENTER, caption, egui::FontId::proportional(11.5), t.text_dim);
    ui.add_space(8.0);
    if widgets::flat_button(ui, tl!("Upload from computer…"), ui.available_width()).clicked() {
        let mode: &'static str = match mode_now {
            1 => "fit",
            2 => "tile",
            _ => "fill",
        };
        crate::picks::button(app, move |app| {
            let pick = FilePick { filters: vec![("Images", &["png", "jpg", "jpeg", "gif", "webp", "tif", "tiff", "bmp"])], ..Default::default() };
            if let Some(path) = crate::picks::open(app, &pick) {
                app.run("paint.imageFill", json!({ "path": path, "mode": mode, "scale": scale_now, "stroke": stroke })).ok();
            }
        });
    }
}

/// How an image fill's pattern `def` sits in an object with bounds `b` (`paint.imageFill`'s
/// layouts): Fit has a tile the size of the bounds with the picture inside it; Fill a tile the
/// size of the picture just covering the bounds; anything else is Tile, at the picture's scale
/// (% of its size at 72 ppi). An index into [`IMAGE_MODES`] and the scale.
pub(crate) fn infer_layout(def: &logocraft_doc::PatternDef, b: logocraft_geom::Rect) -> Option<(usize, f64)> {
    let im = def.art.iter().find_map(|n| match &n.kind {
        logocraft_doc::NodeKind::Image(im) => Some(im),
        _ => None,
    })?;
    let c = im.xf.as_coeffs();
    let (iw, ih) = (c[0].abs() * f64::from(im.width), c[3].abs() * f64::from(im.height));
    let (tw, th) = (def.tile.width(), def.tile.height());
    let (bw, bh) = (b.width(), b.height());
    let near = |a: f64, b: f64| (a - b).abs() <= 1e-3 * a.abs().max(b.abs()).max(1.0);
    if !(iw.is_finite() && ih.is_finite() && iw > 0.0 && ih > 0.0) {
        return None;
    }
    let picture_is_tile = near(iw, tw) && near(ih, th);
    if near(tw, bw) && near(th, bh) && !picture_is_tile {
        return Some((1, 100.0));
    }
    if picture_is_tile && tw >= bw - 1e-3 && th >= bh - 1e-3 && (near(tw, bw) || near(th, bh)) {
        return Some((0, 100.0));
    }
    Some((2, (c[0].abs() * 100.0 * 100.0).round() / 100.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_follow_the_paint() {
        assert_eq!(mode_of(&Paint::None), Mode::Color);
        assert_eq!(mode_of(&Paint::solid(Color::BLACK)), Mode::Color);
        assert_eq!(mode_of(&Paint::Gradient(Box::new(GradientPaint::new(Gradient::default())))), Mode::Gradient);
        assert_eq!(mode_of(&logocraft_doc::pattern::pattern_paint("Image Fill 3")), Mode::Image);
        assert_eq!(mode_of(&logocraft_doc::pattern::pattern_paint("Dots")), Mode::Color);
    }

    #[test]
    fn a_grey_keeps_the_hue_it_came_from() {
        let red = Color::from_hsb(10.0, 1.0, 1.0);
        let last = Some((red.to_hex(), [10.0, 1.0, 1.0]));
        assert_eq!(hsv_of(red, last.clone()), [10.0, 1.0, 1.0]);
        assert_eq!(hsv_of(Color::gray(0.5), last.clone())[0], 10.0);
        assert_eq!(hsv_of(Color::BLACK, last)[..2], [10.0, 1.0]);
    }

    #[test]
    fn hsl_round_trips() {
        for c in [Color::rgb8(230, 120, 40), Color::rgb8(20, 200, 180), Color::rgb8(90, 90, 90)] {
            let [h, s, l] = hsl_of(c, 0.0);
            assert_eq!(from_hsl(h, s, l).to_hex(), c.to_hex());
        }
    }

    #[test]
    fn contrast_levels_are_wcags() {
        assert!((contrast_ratio(Color::BLACK, Color::WHITE) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(Color::WHITE, Color::WHITE) - 1.0).abs() < 1e-6);
        assert_eq!(wcag_level(21.0), Some("AAA"));
        assert_eq!(wcag_level(4.6), Some("AA"));
        assert_eq!(wcag_level(3.2), Some("AA Large"));
        assert_eq!(wcag_level(2.0), None);
        // #767676 is the classic lightest grey passing AA on white.
        assert_eq!(wcag_level(contrast_ratio(Color::rgb8(0x76, 0x76, 0x76), Color::WHITE)), Some("AA"));
    }

    #[test]
    fn cmyk_colours_keep_their_inks() {
        let c = Color::cmyk(0.1, 0.2, 0.3, 0.4);
        assert_eq!(inks(c), [0.1, 0.2, 0.3, 0.4]);
        let dragged = keep_model(c, Color::rgb(1.0, 0.0, 0.0));
        assert_eq!(dragged.model(), cms::Model::Cmyk, "a CMYK colour stays CMYK");
        assert_eq!(super::super::color_json(&dragged)["k"].as_f64().map(|k| k < 0.01), Some(true));
    }

    #[test]
    fn gradient_helpers() {
        assert_eq!(rotate_90(0.0), -90.0);
        assert_eq!(rotate_90(-90.0), 180.0);
        assert_eq!(rotate_90(180.0), 90.0);
        let g = default_gradient(Color::rgb8(255, 0, 0));
        assert_eq!(g.stops.len(), 2);
        assert_eq!((g.stops[0].opacity, g.stops[1].opacity), (1.0, 0.0));
        let mut stops = Gradient::default().stops;
        stops.insert(1, GradientStop::new(0.2, Color::BLACK));
        assert!((widest_gap(&stops) - 0.6).abs() < 1e-6);
    }

    /// The texts one headless frame of the picker draws, for the app's active proxy.
    fn picker_texts(app: &mut LogocraftApp, ctx: &egui::Context) -> String {
        let texts = super::super::tests_appearance::frame_events(ctx, app, vec![], show);
        texts.into_iter().map(|(s, _)| s).collect::<Vec<_>>().join("|")
    }

    #[test]
    fn each_tab_shows_its_controls_and_paints_through_commands() {
        use super::super::tests_appearance::{app_with_rect, run};
        let mut app = app_with_rect();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        run(&mut app, "paint.setFill", json!({"color": "#000000"}));
        let t = picker_texts(&mut app, &ctx);
        assert!(t.contains("Custom") && t.contains("Swatches"), "{t}");
        assert!(t.contains("Contrast") && t.contains("AAA") && t.contains("On this page"), "{t}");
        // Gradient: switching starts from the colour, fading out.
        let paint = active_paint(&app);
        switch_mode(&mut app, &ctx, &paint, Mode::Gradient);
        let Paint::Gradient(g) = active_paint(&app) else { panic!("not a gradient") };
        assert_eq!(g.gradient.stops[1].opacity, 0.0);
        let t = picker_texts(&mut app, &ctx);
        assert!(t.contains("Linear") && t.contains("Stops"), "{t}");
        // Image: the upload button until there is a picture.
        let paint = active_paint(&app);
        switch_mode(&mut app, &ctx, &paint, Mode::Image);
        let t = picker_texts(&mut app, &ctx);
        assert!(t.contains("Upload from computer") && t.contains("No image yet"), "{t}");
        // Back to Color: a solid paint again; a chip click forgets the picked tab.
        let paint = active_paint(&app);
        switch_mode(&mut app, &ctx, &paint, Mode::Color);
        assert!(matches!(active_paint(&app), Paint::Solid { .. }));
        set_pstate(&ctx, MODE, Some(Mode::Image));
        reset(&ctx);
        assert_eq!(pstate::<Option<Mode>>(&ctx, MODE), None);
    }

    #[test]
    fn an_image_fill_reads_back_as_the_mode_that_laid_it_out() {
        use super::super::tests_appearance::{app_with_rect, run};
        let mut app = app_with_rect();
        let img = image::RgbaImage::from_pixel(50, 25, image::Rgba([10, 20, 30, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let data = logocraft_format::base64_encode(&png.into_inner());
        run(&mut app, "paint.imageFill", json!({"dataBase64": data, "mode": "fill"}));
        let read = |app: &LogocraftApp| {
            let st = app.session.active().unwrap();
            let n = super::super::first_node(app).unwrap();
            let Paint::Pattern { pattern, .. } = n.appearance.fill_paint() else { panic!("no image fill") };
            infer_layout(st.doc.pattern(&pattern).unwrap(), n.geometric_bounds().unwrap()).unwrap()
        };
        assert_eq!(read(&app), (0, 100.0));
        run(&mut app, "paint.imageFill", json!({"mode": "fit"}));
        assert_eq!(read(&app), (1, 100.0));
        run(&mut app, "paint.imageFill", json!({"mode": "tile", "scale": 40}));
        assert_eq!(read(&app), (2, 40.0));
    }
}
