//! Optical kerning (Character panel › Kerning › Optical): the space between two letters set from
//! their outlines instead of the font's kerning pairs.
//!
//! Each glyph's sides are sampled on [`BANDS`] horizontal scanlines between its baseline and its
//! cap height: how far its ink stays from its left edge (the origin) and from its right edge (the
//! advance) on each. On each band where either glyph has ink, the pair's separation is the left
//! glyph's right distance plus the right glyph's left distance (a side without ink counts as
//! [`DEPTH`] em). The pair's space is its nearest separation plus the average of how much more
//! each band leaves, each band's extra capped at [`DEPTH`] em (so a deep opening, the arms of a T,
//! counts as some space rather than a hole). A pair is kerned by the difference between that space
//! and the font's rhythm, measured the same way on `H H` (pairs reaching above the x-height) or
//! `n n` (lowercase pairs), scaled by [`STRENGTH`] and kept within [`MAX_TIGHTEN`]/[`MAX_LOOSEN`]
//! em.
//!
//! Outlines are read in font units, y down (as [`crate::FontDb::outline`] gives them); profiles
//! are cached per face and glyph.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use kurbo::{BezPath, PathEl, Point};
use skrifa::MetadataProvider;

use crate::fontdb::FontFace;

/// Scanlines a glyph's sides are sampled on.
pub(crate) const BANDS: usize = 24;
/// The most one side of a gap counts for, in em.
pub(crate) const DEPTH: f64 = 0.16;
/// How much of the difference from the font's rhythm a pair is kerned by.
pub(crate) const STRENGTH: f64 = 0.6;
/// The most a pair is tightened, in em.
pub(crate) const MAX_TIGHTEN: f64 = 0.2;
/// The most a pair is loosened, in em.
pub(crate) const MAX_LOOSEN: f64 = 0.1;
/// Most profiles kept (a cache, cleared when full).
const CACHE_MAX: usize = 8192;

/// How far a glyph's ink keeps from its left and right edges on each band (font units; `None`
/// where the band holds no ink).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Profile {
    pub left: [Option<f64>; BANDS],
    pub right: [Option<f64>; BANDS],
}

impl Profile {
    /// Does the glyph have ink in the sampled zone?
    pub fn has_ink(&self) -> bool {
        self.left.iter().any(Option::is_some)
    }
}

/// The band centres from the baseline up to `top` (font units, y up).
fn band_centres(top: f64) -> impl Iterator<Item = f64> {
    (0..BANDS).map(move |i| top * (i as f64 + 0.5) / BANDS as f64)
}

/// The profile of `outline` (font units, y down) with advance `adv`, sampled from the baseline up
/// to `top`.
pub(crate) fn profile(outline: &BezPath, adv: f64, top: f64, tolerance: f64) -> Profile {
    let mut segs: Vec<(Point, Point)> = Vec::new();
    let mut start = Point::ZERO;
    let mut last = Point::ZERO;
    kurbo::flatten(outline, tolerance.max(1e-3), |el| match el {
        PathEl::MoveTo(p) => {
            start = p;
            last = p;
        }
        PathEl::LineTo(p) => {
            segs.push((last, p));
            last = p;
        }
        PathEl::ClosePath => {
            segs.push((last, start));
            last = start;
        }
        // `flatten` only gives lines.
        _ => {}
    });
    let mut left = [None; BANDS];
    let mut right = [None; BANDS];
    for (i, yc) in band_centres(top).enumerate() {
        // The outline is y down: the band at height `yc` is the line y = -yc.
        let y = -yc;
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for (a, b) in &segs {
            if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
                let t = (y - a.y) / (b.y - a.y);
                let x = a.x + t * (b.x - a.x);
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
        if lo.is_finite() && hi.is_finite() {
            left[i] = Some(lo);
            right[i] = Some(adv - hi);
        }
    }
    Profile { left, right }
}

/// The space two glyphs leave between them (font units): their nearest separation over the bands
/// where either has ink, plus the average extra space of those bands, each capped at `depth` (a
/// side without ink counts as `depth`). `None` when either has no ink in the zone.
pub(crate) fn gap(a: &Profile, b: &Profile, depth: f64) -> Option<f64> {
    if !a.has_ink() || !b.has_ink() {
        return None;
    }
    let seps: Vec<f64> = (0..BANDS)
        .filter_map(|i| match (a.right[i], b.left[i]) {
            (None, None) => None,
            (r, l) => Some(r.unwrap_or(depth) + l.unwrap_or(depth)),
        })
        .collect();
    let nearest = seps.iter().copied().fold(f64::INFINITY, f64::min);
    if !nearest.is_finite() {
        return None;
    }
    let extra: f64 = seps.iter().map(|d| (d - nearest).min(depth)).sum::<f64>() / seps.len() as f64;
    Some(nearest + extra)
}

/// Does the glyph have ink above `top_band` (the band holding the x-height)?
pub(crate) fn reaches_above(p: &Profile, top_band: usize) -> bool {
    p.left.iter().skip(top_band + 1).any(Option::is_some)
}

/// The kerning (font units) that brings a pair whose space is `gap` to the font's rhythm
/// `reference`, within the limits (`upem`: the font's units per em).
pub(crate) fn kern(gap: f64, reference: f64, upem: f64) -> f64 {
    ((reference - gap) * STRENGTH).clamp(-MAX_TIGHTEN * upem, MAX_LOOSEN * upem)
}

type Key = (u32, u32);

fn cache() -> &'static Mutex<HashMap<Key, Arc<Profile>>> {
    static CACHE: OnceLock<Mutex<HashMap<Key, Arc<Profile>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The top of the sampled zone of `face` (font units): its cap height, else 70% of the em.
fn zone_top(face: &FontFace) -> f64 {
    if face.cap_height > 0.0 { face.cap_height } else { face.upem * 0.7 }
}

/// The profile of glyph `gid` of `face`, whose advance is `adv` (font units), cached.
pub(crate) fn glyph_profile(face: &FontFace, gid: u32, adv: f64) -> Arc<Profile> {
    let key = (face.id(), gid);
    if let Some(p) = cache().lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return p.clone();
    }
    let outline = crate::FontDb::global().outline(face, gid);
    let p = Arc::new(profile(&outline, adv, zone_top(face), face.upem / 500.0));
    let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
    if c.len() >= CACHE_MAX {
        c.clear();
    }
    c.insert(key, p.clone());
    p
}

/// The font's rhythm (font units): the space of `H H` and of `n n` (else `o o`), measured as [`gap`]
/// measures any pair, and the band holding the x-height. `None` when the font has neither.
pub(crate) struct Reference {
    pub caps: Option<f64>,
    pub lower: Option<f64>,
    pub x_band: usize,
}

impl Reference {
    /// The rhythm a pair is measured against: lowercase when neither glyph reaches above the
    /// x-height.
    pub fn for_pair(&self, a: &Profile, b: &Profile) -> Option<f64> {
        let tall = reaches_above(a, self.x_band) || reaches_above(b, self.x_band);
        if tall { self.caps.or(self.lower) } else { self.lower.or(self.caps) }
    }
}

pub(crate) fn reference(face: &FontFace) -> Option<Reference> {
    let f = face.skrifa()?;
    let cmap = f.charmap();
    let metrics = f.glyph_metrics(skrifa::instance::Size::unscaled(), face.location());
    let depth = DEPTH * face.upem;
    let measure = |chars: &[char]| {
        chars.iter().find_map(|&c| {
            let gid = cmap.map(c)?;
            let adv = f64::from(metrics.advance_width(gid)?);
            let p = glyph_profile(face, gid.to_u32(), adv);
            gap(&p, &p, depth)
        })
    };
    let (caps, lower) = (measure(&['H']), measure(&['n', 'o']));
    if caps.is_none() && lower.is_none() {
        return None;
    }
    let top = zone_top(face);
    let x = if face.x_height > 0.0 { face.x_height } else { top * 0.7 };
    let x_band = ((x / top * BANDS as f64).floor().max(0.0) as usize).min(BANDS - 1);
    Some(Reference { caps, lower, x_band })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;

    /// A box glyph from x0 to x1 (font units), baseline to 700 high, in an advance of `adv`.
    fn bar(x0: f64, x1: f64, adv: f64) -> Profile {
        let r = kurbo::Rect::new(x0, -700.0, x1, 0.0);
        profile(&r.to_path(0.1), adv, 700.0, 1.0)
    }

    #[test]
    fn a_box_has_its_side_bearings_on_every_band() {
        let p = bar(50.0, 450.0, 500.0);
        assert!(p.left.iter().all(|v| v.is_some_and(|v| (v - 50.0).abs() < 1e-6)));
        assert!(p.right.iter().all(|v| v.is_some_and(|v| (v - 50.0).abs() < 1e-6)));
    }

    #[test]
    fn the_gap_adds_both_sides_and_caps_deep_openings() {
        let a = bar(50.0, 450.0, 500.0);
        let b = bar(30.0, 470.0, 500.0);
        assert_eq!(gap(&a, &b, 160.0), Some(80.0));
        // A glyph ink only in its lower half: its upper bands count as the full depth.
        let low = kurbo::Rect::new(100.0, -300.0, 400.0, 0.0);
        let half = profile(&low.to_path(0.1), 500.0, 700.0, 1.0);
        let g = gap(&half, &a, 160.0).unwrap();
        assert!(g > 100.0 + 50.0 && g < 160.0 + 50.0, "{g}");
        // Parallel diagonals (A V) are as far apart as their separation, not their capped sides.
        let slant = |lean: f64| Profile {
            left: std::array::from_fn(|i| Some(200.0 - lean * i as f64)),
            right: std::array::from_fn(|i| Some(lean * i as f64)),
        };
        assert!((gap(&slant(9.0), &slant(9.0), 160.0).unwrap() - 200.0).abs() < 1e-9);
        // Bands neither glyph has ink on don't count.
        let mut low = bar(50.0, 450.0, 500.0);
        for i in 12..BANDS {
            low.left[i] = None;
            low.right[i] = None;
        }
        assert_eq!(gap(&low, &low, 160.0), Some(100.0));
        // No ink: no gap to measure.
        let empty = profile(&BezPath::new(), 250.0, 700.0, 1.0);
        assert_eq!(gap(&empty, &a, 160.0), None);
    }

    #[test]
    fn kerning_closes_open_pairs_and_opens_tight_ones_within_limits() {
        assert!((kern(150.0, 100.0, 1000.0) + 50.0 * STRENGTH).abs() < 1e-9, "too open: tightened");
        assert!(kern(60.0, 100.0, 1000.0) > 0.0, "too tight: loosened");
        assert_eq!(kern(100.0, 100.0, 1000.0), 0.0);
        assert_eq!(kern(10_000.0, 100.0, 1000.0), -200.0);
        assert_eq!(kern(-10_000.0, 100.0, 1000.0), 100.0);
    }
}
