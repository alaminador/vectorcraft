//! Paragraph › Justification… (word and letter spacing, glyph scaling, Auto Leading, Single Word
//! Justification), Hyphenation… (Hyphen Limit, Words Longer Than), Character › No Break and
//! Kerning › Optical.

use super::*;
use kurbo::{Affine, Shape};
use vectorcraft_doc::{CharStyle, Justify, SingleWord, TextKind};
use vectorcraft_geom::PathData;

fn db() -> &'static FontDb {
    FontDb::global()
}

fn style(size: f64) -> CharStyle {
    CharStyle { size, ..CharStyle::default() }
}

fn area(text: &str, st: CharStyle, frame: Rect, justify: Justify) -> TextObject {
    let mut t = TextObject::point(Point::ZERO, text, st);
    t.kind = TextKind::Area { frame: PathData::from_bezpath(&frame.to_path(0.1)) };
    t.xf = Affine::IDENTITY;
    t.para.justify = justify;
    t
}

fn point(text: &str, st: CharStyle) -> TextObject {
    let mut t = TextObject::point(Point::ZERO, text, st);
    t.xf = Affine::IDENTITY;
    t
}

/// The x of the glyph of the character at `byte`.
fn x_at(l: &TextLayout, byte: usize) -> f64 {
    l.glyphs.iter().find(|g| g.byte == byte).map(|g| g.origin.x).unwrap()
}

fn width(l: &TextLayout) -> f64 {
    l.glyphs.last().map(|g| g.origin.x + g.advance).unwrap() - l.glyphs[0].origin.x
}

const COPY: &str = "Typography is the craft of arranging type to make written language legible, readable and appealing when displayed. \
The arrangement of type involves selecting typefaces, point sizes, line lengths, line spacing and letter spacing, and adjusting the space \
between pairs of letters. Designers balance hyphenation against justification to produce even texture across the paragraph.";

#[test]
fn desired_word_and_letter_spacing_and_glyph_scaling_set_every_line() {
    let base = layout(db(), &point("ab cd", style(20.0)));
    let space = x_at(&base, 3) - x_at(&base, 2);
    // Desired word spacing 200%: the space is twice as wide.
    let mut t = point("ab cd", style(20.0));
    t.para.justification.word_spacing = [80.0, 200.0, 250.0];
    let wide = layout(db(), &t);
    assert!(((x_at(&wide, 3) - x_at(&wide, 2)) - 2.0 * space).abs() < 1e-6);
    // Desired letter spacing 40%: each letter is followed by 40% of a quarter em (2 pt at 20 pt).
    let mut t = point("ab cd", style(20.0));
    t.para.justification.letter_spacing = [0.0, 40.0, 40.0];
    let spaced = layout(db(), &t);
    let ab = |l: &TextLayout| x_at(l, 1) - x_at(l, 0);
    assert!((ab(&spaced) - ab(&base) - 2.0).abs() < 1e-6, "{} vs {}", ab(&spaced), ab(&base));
    // Desired glyph scaling 150%: letters are half as wide again.
    let mut t = point("ab cd", style(20.0));
    t.para.justification.glyph_scaling = [100.0, 150.0, 150.0];
    let scaled = layout(db(), &t);
    assert!((ab(&scaled) - ab(&base) * 1.5).abs() < 1e-6);
    assert!(width(&scaled) > width(&base));
}

#[test]
fn auto_leading_is_the_paragraphs_percentage_of_the_size() {
    let frame = Rect::new(0.0, 0.0, 200.0, 400.0);
    let lead = |pct: f64, st: CharStyle| {
        let mut t = area("one\ntwo", st, frame, Justify::Left);
        t.para.justification.auto_leading = pct;
        let l = layout(db(), &t);
        l.lines[1].baseline - l.lines[0].baseline
    };
    assert!((lead(120.0, style(10.0)) - 12.0).abs() < 1e-6);
    assert!((lead(200.0, style(10.0)) - 20.0).abs() < 1e-6);
    // A fixed leading ignores it.
    let fixed = CharStyle { leading: Some(15.0), ..style(10.0) };
    assert!((lead(200.0, fixed) - 15.0).abs() < 1e-6);
}

#[test]
fn justified_spaces_stop_at_their_maximum_and_letters_take_the_rest() {
    let frame = Rect::new(0.0, 0.0, 300.0, 100.0);
    let mut t = area("ab cd", style(20.0), frame, Justify::JustifyAll);
    let spaces_only = layout(db(), &t);
    t.para.justification.letter_spacing = [0.0, 0.0, 50.0];
    let letters = layout(db(), &t);
    for l in [&spaces_only, &letters] {
        assert!((l.lines[0].x1 - l.lines[0].x0 - 300.0).abs() < 1e-6, "still full: {:?}", l.lines[0]);
    }
    let ab = |l: &TextLayout| x_at(l, 1) - x_at(l, 0);
    // Letters may grow by up to 50% of a quarter em (2.5 pt).
    assert!((ab(&letters) - ab(&spaces_only) - 2.5).abs() < 1e-6, "{} {}", ab(&letters), ab(&spaces_only));
    assert!(x_at(&letters, 3) - x_at(&letters, 2) < x_at(&spaces_only, 3) - x_at(&spaces_only, 2), "the space is narrower");
}

#[test]
fn single_word_justification_spreads_or_aligns_a_lone_word() {
    let frame = Rect::new(0.0, 0.0, 300.0, 100.0);
    let mut t = area("Hello", style(20.0), frame, Justify::JustifyAll);
    let full = layout(db(), &t);
    assert!((width(&full) - 300.0).abs() < 1e-6, "Full Justify fills the line");
    t.para.justification.single_word = SingleWord::AlignCenter;
    let centred = layout(db(), &t);
    let w = width(&centred);
    assert!(w < 200.0);
    assert!((centred.glyphs[0].origin.x - (300.0 - w) / 2.0).abs() < 1e-6);
    t.para.justification.single_word = SingleWord::AlignRight;
    let right = layout(db(), &t);
    assert!((right.glyphs[0].origin.x + width(&right) - 300.0).abs() < 1e-6);
    t.para.justification.single_word = SingleWord::AlignLeft;
    assert!(layout(db(), &t).glyphs[0].origin.x.abs() < 1e-6);
}

/// Lines ending in a hyphen, in a row at most.
fn hyphen_runs(l: &TextLayout) -> usize {
    let ends: Vec<bool> = l.lines.iter().map(|li| li.glyph_end > li.glyph_start && l.glyphs[li.glyph_end - 1].len == 0).collect();
    let (mut best, mut run) = (0, 0);
    for e in ends {
        run = if e { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

#[test]
fn hyphen_limit_caps_consecutive_hyphenated_lines() {
    for composer in [Composer::EveryLine, Composer::SingleLine] {
        let mut t = area(&COPY.repeat(3), style(12.0), Rect::new(0.0, 0.0, 90.0, 5000.0), Justify::Left);
        t.para.hyphenate = true;
        t.para.hyphenation.words_longer_than = 3;
        t.para.hyphenation.after_first = 2;
        t.para.hyphenation.before_last = 2;
        let opts = LayoutOptions { composer: Some(composer), ..Default::default() };
        let free = layout_with(db(), &t, &opts);
        assert!(hyphen_runs(&free) >= 2, "{composer:?}: the copy hyphenates lines in a row");
        t.para.hyphenation.limit = 1;
        let limited = layout_with(db(), &t, &opts);
        assert_eq!(hyphen_runs(&limited), 1, "{composer:?}");
    }
}

#[test]
fn words_longer_than_keeps_short_words_whole() {
    let mut t = area(&COPY.repeat(2), style(12.0), Rect::new(0.0, 0.0, 90.0, 5000.0), Justify::Left);
    t.para.hyphenate = true;
    let hyphens = |l: &TextLayout| l.glyphs.iter().filter(|g| g.len == 0).count();
    let some = hyphens(&layout(db(), &t));
    assert!(some > 0);
    t.para.hyphenation.words_longer_than = 25;
    assert_eq!(hyphens(&layout(db(), &t)), 0, "no word of the copy is that long");
}

#[test]
fn no_break_keeps_its_characters_on_one_line() {
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa";
    let frame = Rect::new(0.0, 0.0, 110.0, 1000.0);
    let t = area(text, style(12.0), frame, Justify::Left);
    let plain = layout(db(), &t);
    // A line breaks somewhere inside "gamma delta epsilon" without No Break…
    let (a, b) = (text.find("gamma").unwrap(), text.find("epsilon").unwrap() + "epsilon".len());
    let breaks_inside = |l: &TextLayout| l.lines.iter().any(|li| li.start > a && li.start < b);
    assert!(breaks_inside(&plain), "{:?}", plain.lines.iter().map(|li| &text[li.start..li.end]).collect::<Vec<_>>());
    // …and not with it.
    let mut t = t;
    edit::style_range(&mut t.runs, a, b, |s| s.no_break = true);
    let kept = layout(db(), &t);
    assert!(!breaks_inside(&kept), "{:?}", kept.lines.iter().map(|li| &text[li.start..li.end]).collect::<Vec<_>>());
}

#[test]
fn optical_kerning_spaces_pairs_from_their_outlines() {
    let auto = layout(db(), &point("AVATAR", style(40.0)));
    if auto.glyphs.iter().all(|g| g.outline.elements().is_empty()) {
        return; // No font with outlines on this machine.
    }
    let optical = layout(db(), &point("AVATAR", CharStyle { optical_kerning: true, ..style(40.0) }));
    // The open A-V, V-A and T-A pairs close up; the font's own pairs are off.
    let pair = |l: &TextLayout, i: usize| x_at(l, i + 1) - x_at(l, i);
    let plain = layout(db(), &point("AVATAR", CharStyle { kerning: Some(0.0), ..style(40.0) }));
    for i in [0, 1, 3] {
        assert!(pair(&optical, i) < pair(&plain, i) - 0.4, "pair {i}: optical {} vs none {}", pair(&optical, i), pair(&plain, i));
    }
    // Straight-sided pairs keep their spacing.
    let hh = |st: CharStyle| {
        let l = layout(db(), &point("HH", st));
        x_at(&l, 1) - x_at(&l, 0)
    };
    assert!((hh(CharStyle { optical_kerning: true, ..style(40.0) }) - hh(CharStyle { kerning: Some(0.0), ..style(40.0) })).abs() < 0.05);
    assert_ne!(width(&optical), width(&auto));
    // Manual kerning wins over Optical.
    let manual = layout(db(), &point("AVATAR", CharStyle { kerning: Some(0.0), optical_kerning: true, ..style(40.0) }));
    assert!((width(&manual) - width(&plain)).abs() < 1e-9);
}
