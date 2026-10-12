//! Paragraph panel menu › Justification…: Word Spacing, Letter Spacing and Glyph Scaling
//! (minimum, desired, maximum), Auto Leading and Single Word Justification of the selected
//! paragraphs. OK runs `text.setFormat` with `justification`.
//!
//! Fields: `wordSpacing`, `letterSpacing`, `glyphScaling` (each `[min, desired, max]`, percent),
//! `autoLeading` (percent), `singleWord` (`text.setFormat` names).

use serde_json::{Value, json};
use vectorcraft_doc::{Justification, SingleWord};

use super::DialogSpec;
use super::swatch_options::label;
use crate::state::Dialog;
use crate::{VectorcraftApp, widgets};

/// The dialog kind of Justification.
pub const KIND: &str = "justification";

pub(super) const SPEC: DialogSpec = DialogSpec { heading: |_| tl!("Justification").into(), body, confirm, min_width: 380.0, ..DialogSpec::FORM };

/// Single Word Justification: (label, `text.setFormat` name, value).
pub const SINGLE_WORD: [(&str, &str, SingleWord); 4] = [
    ("Full Justify", "fullJustify", SingleWord::FullJustify),
    ("Align Left", "alignLeft", SingleWord::AlignLeft),
    ("Align Center", "alignCenter", SingleWord::AlignCenter),
    ("Align Right", "alignRight", SingleWord::AlignRight),
];

/// The dialog's fields for `j`.
pub fn fields(j: &Justification) -> Value {
    let single = SINGLE_WORD.iter().find(|s| s.2 == j.single_word).map_or("fullJustify", |s| s.1);
    json!({
        "wordSpacing": j.word_spacing,
        "letterSpacing": j.letter_spacing,
        "glyphScaling": j.glyph_scaling,
        "autoLeading": j.auto_leading,
        "singleWord": single,
    })
}

/// Open Justification on the selected paragraphs (the first one's values).
pub fn open(app: &mut VectorcraftApp) -> Result<Value, String> {
    let (_, para) = crate::panels::character::text_style(app).ok_or("select some type first")?;
    app.ui.dialog = Some(Dialog::new(KIND, fields(&para.justification)));
    Ok(Value::Null)
}

fn triple(d: &Dialog, k: &str) -> [f64; 3] {
    let a = d.fields.get(k).and_then(Value::as_array);
    let at = |i: usize| a.and_then(|a| a.get(i)).and_then(Value::as_f64).unwrap_or(0.0);
    [at(0), at(1), at(2)]
}

fn body(_app: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    let rows = [
        (tl!("Word Spacing:"), "wordSpacing", 0.0, 1000.0),
        (tl!("Letter Spacing:"), "letterSpacing", -100.0, 500.0),
        (tl!("Glyph Scaling:"), "glyphScaling", 50.0, 200.0),
    ];
    egui::Grid::new("justification").num_columns(4).spacing([8.0, 8.0]).show(ui, |ui| {
        ui.label("");
        for h in [tl!("Minimum"), tl!("Desired"), tl!("Maximum")] {
            label(ui, h);
        }
        ui.end_row();
        for (name, key, lo, hi) in rows {
            label(ui, name);
            let mut t = triple(d, key);
            let mut changed = false;
            for (i, slot) in t.clone().iter().enumerate() {
                if let Some(v) = widgets::spin_plain(ui, (key, i), Some(*slot), "%", 0, 80.0, 1.0, lo, &[]) {
                    t[i] = v.clamp(lo, hi);
                    changed = true;
                }
            }
            if changed {
                // Keep minimum ≤ desired ≤ maximum, moving the others with the one typed.
                let [a, b, c] = t;
                let b = b.clamp(a.min(c), a.max(c));
                d.fields.insert(key.into(), json!([a.min(b), b, c.max(b)]));
            }
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    egui::Grid::new("justification-more").num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
        label(ui, tl!("Auto Leading:"));
        if let Some(v) = widgets::spin_plain(ui, "jf-auto", Some(d.f64("autoLeading", 120.0)), "%", 0, 80.0, 1.0, 0.0, &[]) {
            d.fields.insert("autoLeading".into(), json!(v.clamp(0.0, 500.0)));
        }
        ui.end_row();
        label(ui, tl!("Single Word Justification:"));
        let names: Vec<&str> = SINGLE_WORD.iter().map(|s| tl!(s.0)).collect();
        let key = d.str("singleWord");
        let current = SINGLE_WORD.iter().zip(&names).find(|(s, _)| s.1 == key).map_or(names[0], |(_, n)| *n);
        if let Some(i) = widgets::dropdown_names(ui, "jf-single", current, &names, 150.0)
            && let Some(s) = SINGLE_WORD.get(i)
        {
            d.fields.insert("singleWord".into(), json!(s.1));
        }
        ui.end_row();
    });
    false
}

/// The `justification` param of the dialog's fields.
pub fn params(d: &Dialog) -> Value {
    json!({
        "wordSpacing": triple(d, "wordSpacing"),
        "letterSpacing": triple(d, "letterSpacing"),
        "glyphScaling": triple(d, "glyphScaling"),
        "autoLeading": d.f64("autoLeading", 120.0),
        "singleWord": d.str("singleWord"),
    })
}

fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    crate::panels::paragraph::para_cmd(app, "text.setFormat", json!({"justification": params(d)}))?;
    app.ui.dialog = None;
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_engine::Session;

    fn app() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
        app.run("text.create", json!({"x": 10, "y": 10, "text": "one two", "area": {"width": 200, "height": 100}})).unwrap();
        app
    }

    #[test]
    fn the_paragraph_menu_opens_justification_and_ok_sets_it() {
        let mut app = app();
        open(&mut app).unwrap();
        let d = app.ui.dialog.as_mut().expect("Justification");
        assert_eq!((d.kind.as_str(), triple(d, "wordSpacing")), (KIND, [80.0, 100.0, 133.0]));
        d.fields.insert("letterSpacing".into(), json!([-5, 0, 10]));
        d.fields.insert("autoLeading".into(), json!(150));
        d.fields.insert("singleWord".into(), json!("alignRight"));
        crate::dialogs::confirm(&mut app).unwrap();
        assert!(app.ui.dialog.is_none());
        let (_, para) = crate::panels::character::text_style(&app).unwrap();
        assert_eq!(para.justification.letter_spacing, [-5.0, 0.0, 10.0]);
        assert_eq!((para.justification.auto_leading, para.justification.single_word), (150.0, SingleWord::AlignRight));
        // Hyphenation… the same way.
        crate::dialogs::hyphenation::open(&mut app).unwrap();
        let d = app.ui.dialog.as_mut().expect("Hyphenation");
        d.fields.insert("hyphenate".into(), json!(true));
        d.fields.insert("limit".into(), json!(3));
        d.fields.insert("beforeLast".into(), json!(99));
        crate::dialogs::confirm(&mut app).unwrap();
        let (_, para) = crate::panels::character::text_style(&app).unwrap();
        assert!(para.hyphenate);
        assert_eq!((para.hyphenation.limit, para.hyphenation.before_last), (3, 15), "clamped to the range");
    }

    #[test]
    fn both_dialogs_paint_their_fields() {
        let mut app = app();
        for (open, labels) in [
            (
                open as fn(&mut VectorcraftApp) -> Result<Value, String>,
                &[
                    "Justification",
                    "Word Spacing:",
                    "Letter Spacing:",
                    "Glyph Scaling:",
                    "Auto Leading:",
                    "Single Word Justification:",
                    "Minimum",
                    "Desired",
                    "Maximum",
                ][..],
            ),
            (
                crate::dialogs::hyphenation::open,
                &[
                    "Hyphenation",
                    "Words Longer Than:",
                    "After First:",
                    "Before Last:",
                    "Hyphen Limit:",
                    "Hyphenation Zone:",
                    "Hyphenate Capitalized Words",
                ][..],
            ),
        ] {
            open(&mut app).unwrap();
            let text = crate::tests_labels::painted_text(&mut app, |app, ui| crate::dialogs::show(app, ui.ctx()));
            for l in labels {
                assert!(text.contains(l), "{l} in {text}");
            }
            app.ui.dialog = None;
        }
    }
}
