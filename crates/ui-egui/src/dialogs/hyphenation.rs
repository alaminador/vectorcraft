//! Paragraph panel menu › Hyphenation…: Hyphenation on or off, Words Longer Than, After First,
//! Before Last, Hyphen Limit, Hyphenation Zone and Hyphenate Capitalized Words of the selected
//! paragraphs. OK runs `text.setFormat` with `hyphenate` and `hyphenation`.
//!
//! Fields: `hyphenate`, `wordsLongerThan`, `afterFirst`, `beforeLast`, `limit`, `zone` (points),
//! `capitalized`.

use serde_json::{Value, json};
use vectorcraft_doc::ParaStyle;

use super::DialogSpec;
use super::swatch_options::{grid, label};
use crate::state::Dialog;
use crate::{VectorcraftApp, widgets};

/// The dialog kind of Hyphenation.
pub const KIND: &str = "hyphenation";

pub(super) const SPEC: DialogSpec = DialogSpec { heading: |_| tl!("Hyphenation").into(), body, confirm, min_width: 340.0, ..DialogSpec::FORM };

/// The dialog's fields for paragraph `p`.
pub fn fields(p: &ParaStyle) -> Value {
    let h = &p.hyphenation;
    json!({
        "hyphenate": p.hyphenate,
        "wordsLongerThan": h.words_longer_than,
        "afterFirst": h.after_first,
        "beforeLast": h.before_last,
        "limit": h.limit,
        "zone": h.zone,
        "capitalized": h.capitalized,
    })
}

/// Open Hyphenation on the selected paragraphs (the first one's values).
pub fn open(app: &mut VectorcraftApp) -> Result<Value, String> {
    let (_, para) = crate::panels::character::text_style(app).ok_or("select some type first")?;
    app.ui.dialog = Some(Dialog::new(KIND, fields(&para)));
    Ok(Value::Null)
}

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    let on = d.bool("hyphenate");
    if widgets::check(ui, tl!("Hyphenation"), on, true) {
        d.fields.insert("hyphenate".into(), json!(!on));
    }
    ui.add_space(6.0);
    let unit = app.session.general_unit();
    ui.add_enabled_ui(on, |ui| {
        grid(ui, |ui| {
            for (name, key, suffix, lo, hi) in [
                (tl!("Words Longer Than:"), "wordsLongerThan", tl!("letters"), 1.0, 25.0),
                (tl!("After First:"), "afterFirst", tl!("letters"), 1.0, 15.0),
                (tl!("Before Last:"), "beforeLast", tl!("letters"), 1.0, 15.0),
                (tl!("Hyphen Limit:"), "limit", tl!("hyphens"), 0.0, 25.0),
            ] {
                label(ui, name);
                ui.horizontal(|ui| {
                    if let Some(v) = widgets::spin_plain(ui, key, Some(d.f64(key, lo)), "", 0, 70.0, 1.0, lo, &[]) {
                        d.fields.insert(key.into(), json!(v.round().clamp(lo, hi) as u64));
                    }
                    ui.label(suffix);
                });
                ui.end_row();
            }
            label(ui, tl!("Hyphenation Zone:"));
            if let Some(v) = widgets::spin_field(ui, "hy-zone", Some(d.f64("zone", 0.0)), unit, 100.0, 1.0, 0.0, &[]) {
                d.fields.insert("zone".into(), json!(v.clamp(0.0, 1296.0)));
            }
            ui.end_row();
        });
        ui.add_space(4.0);
        let caps = d.bool("capitalized");
        if widgets::check(ui, tl!("Hyphenate Capitalized Words"), caps, true) {
            d.fields.insert("capitalized".into(), json!(!caps));
        }
    });
    false
}

/// The `text.setFormat` params of the dialog's fields.
pub fn params(d: &Dialog) -> Value {
    let n = |k: &str, d0: f64| d.f64(k, d0).round().max(0.0) as u64;
    json!({
        "hyphenate": d.bool("hyphenate"),
        "hyphenation": {
            "wordsLongerThan": n("wordsLongerThan", 5.0).clamp(1, 25),
            "afterFirst": n("afterFirst", 3.0).clamp(1, 15),
            "beforeLast": n("beforeLast", 3.0).clamp(1, 15),
            "limit": n("limit", 0.0).min(25),
            "zone": d.f64("zone", 0.0).clamp(0.0, 1296.0),
            "capitalized": d.bool("capitalized"),
        },
    })
}

fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    crate::panels::paragraph::para_cmd(app, "text.setFormat", params(d))?;
    app.ui.dialog = None;
    Ok(Value::Null)
}
