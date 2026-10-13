//! Figma's Noise and Texture effects (public documentation:
//! https://help.figma.com/hc/en-us/articles/360041488473), original implementations: grains fixed
//! to the page (document space), so they don't swim as the view pans or zooms.

use super::PixelSpace;
use logocraft_geom::Point;

/// A well-mixed hash of a grain cell (`x`, `y`) and a `salt`, 0..1.
fn hash(x: i64, y: i64, salt: u64) -> f64 {
    let mut z =
        (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ salt.wrapping_mul(0x1656_67B1_9E37_79F9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// The grain cell of pixel (`x`, `y`) for grains `size` across.
fn cell(space: &PixelSpace, x: usize, y: usize, size: f64) -> Option<(i64, i64)> {
    let d = space.to_doc * Point::new(x as f64 + 0.5, y as f64 + 0.5);
    let (cx, cy) = ((d.x / size).floor(), (d.y / size).floor());
    (cx.is_finite() && cy.is_finite() && cx.abs() < 1e15 && cy.abs() < 1e15).then_some((cx as i64, cy as i64))
}

/// A bright colour for a multi-colour grain.
fn hue_rgb(t: f64) -> [f64; 3] {
    let h = (t * 6.0).rem_euclid(6.0);
    let x = 1.0 - ((h % 2.0) - 1.0).abs();
    match h as u32 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    }
}

/// Grains over the object's pixels (premultiplied RGBA): each grain cell, with probability
/// `density`, mixes `color` (or a random colour with `multi`) in at `opacity`, as much as the
/// object covers the pixel.
#[allow(clippy::too_many_arguments)]
pub(super) fn noise(px: &mut [[u8; 4]], w: usize, h: usize, space: &PixelSpace, size: f64, density: f64, opacity: f64, color: [u8; 3], multi: bool) {
    if !(size.is_finite() && size > 0.0) || density <= 0.0 || opacity <= 0.0 {
        return;
    }
    for y in 0..h {
        for x in 0..w {
            let Some(p) = px.get_mut(y * w + x) else { continue };
            let a = p[3] as f64 / 255.0;
            if a <= 0.0 {
                continue;
            }
            let Some((cx, cy)) = cell(space, x, y, size) else { continue };
            if hash(cx, cy, 1) >= density {
                continue;
            }
            let rgb = if multi { hue_rgb(hash(cx, cy, 2)) } else { color.map(|c| c as f64 / 255.0) };
            let o = opacity;
            for (ch, v) in p.iter_mut().take(3).zip(rgb) {
                let mixed = *ch as f64 * (1.0 - o) + v * 255.0 * a * o;
                *ch = mixed.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// The object's pixels jittered by up to `radius` (document units), one random shift per grain
/// cell `size` across; with `clip`, what lands outside the object's shape is cut away.
pub(super) fn texture(px: &mut [[u8; 4]], w: usize, h: usize, space: &PixelSpace, size: f64, radius: f64, clip: bool) {
    let r = radius / space.px.max(1e-9);
    if !(size.is_finite() && size > 0.0 && r.is_finite() && r >= 0.5) {
        return;
    }
    let src = px.to_vec();
    for y in 0..h {
        for x in 0..w {
            let Some((cx, cy)) = cell(space, x, y, size) else { continue };
            let dx = ((hash(cx, cy, 3) - 0.5) * 2.0 * r).round() as i64;
            let dy = ((hash(cx, cy, 4) - 0.5) * 2.0 * r).round() as i64;
            let (sx, sy) = (x as i64 + dx, y as i64 + dy);
            let sample =
                if sx >= 0 && sy >= 0 && (sx as usize) < w && (sy as usize) < h { src.get(sy as usize * w + sx as usize).copied() } else { None };
            let mut out = sample.unwrap_or([0; 4]);
            if clip {
                let keep = src.get(y * w + x).map_or(0.0, |o| o[3] as f64 / 255.0);
                out = out.map(|c| (c as f64 * keep).round() as u8);
            }
            if let Some(p) = px.get_mut(y * w + x) {
                *p = out;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logocraft_geom::Affine;

    fn space() -> PixelSpace {
        PixelSpace { to_doc: Affine::IDENTITY, px: 1.0, center: Point::ZERO, channels: Default::default(), line: 1.0, extent: 10.0 }
    }

    #[test]
    fn noise_marks_some_pixels_and_keeps_transparency() {
        let (w, h) = (32, 32);
        let mut px = vec![[255u8, 255, 255, 255]; w * h];
        px[0] = [0, 0, 0, 0];
        noise(&mut px, w, h, &space(), 1.0, 0.5, 1.0, [0, 0, 0], false);
        let dark = px.iter().filter(|p| p[0] == 0 && p[3] == 255).count();
        assert!(dark > 300 && dark < 700, "about half the grains: {dark}");
        assert_eq!(px[0], [0, 0, 0, 0], "transparent stays transparent");
        // The same page position gives the same grain.
        let mut again = vec![[255u8, 255, 255, 255]; w * h];
        noise(&mut again, w, h, &space(), 1.0, 0.5, 1.0, [0, 0, 0], false);
        assert_eq!(px[5..], again[5..]);
    }

    #[test]
    fn texture_roughens_edges_and_clips() {
        let (w, h) = (40, 40);
        let mut px = vec![[0u8; 4]; w * h];
        for y in 10..30 {
            for x in 10..30 {
                px[y * w + x] = [255, 0, 0, 255];
            }
        }
        let before = px.clone();
        texture(&mut px, w, h, &space(), 2.0, 3.0, false);
        assert_ne!(px, before, "the edges moved");
        let mut clipped = before.clone();
        texture(&mut clipped, w, h, &space(), 2.0, 3.0, true);
        assert!(clipped.iter().zip(&before).all(|(c, b)| b[3] > 0 || c[3] == 0), "nothing outside the shape");
    }
}
