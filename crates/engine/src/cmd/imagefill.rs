//! Image fills (the fill picker's Image tab, as in Figma): a picture painted inside a shape.
//!
//! Each filled object gets its own pattern swatch named `Image Fill N` ([`IMAGE_FILL_PREFIX`])
//! holding the embedded picture, so rendering, hit testing and every export draw it as they draw
//! patterns (vectors, the picture embedded once). Its tile depends on the mode:
//!
//! - **Fill**: the picture scaled to cover the shape's bounds, centred (the edges cropped);
//! - **Fit**: the picture scaled to fit inside the bounds, centred (the tile is the bounds, so the
//!   repeats fall outside the shape);
//! - **Tile**: the picture repeated at `scale` % of its size, from the bounds' top-left corner.
//!
//! Image fills move, scale and rotate with their object whatever Transform Patterns says
//! (`logocraft_doc::pattern::transform_image_fills`).

use std::sync::Arc;

use logocraft_color::Paint;
use logocraft_doc::pattern::IMAGE_FILL_PREFIX;
use logocraft_doc::{ImageObject, Node, NodeId, NodeKind, PatternDef};
use logocraft_geom::{Affine, Rect};
use serde_json::{Value, json};

use super::*;
use crate::EngineError;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "paint.imageFill",
        "Image Fill",
        [],
        None,
        "{path | dataBase64 (neither: lay out each object's current image fill again, e.g. in another mode), mode?: \"fill\" (cover the shape, cropped; default) | \"fit\" (inside the shape, whole) | \"tile\" (repeated), scale?: % (tile: of the picture's size at its resolution; 100), stroke?: false (true: the stroke), ids?} paint the picture inside each selected object: each gets its own `Image Fill N` pattern swatch, which moves and scales with it → {ids, patterns}",
        has_selection,
        image_fill
    )]
}

/// How an image fill sits in its shape.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Fill,
    Fit,
    Tile(f64),
}

/// The pattern tile and the art for a `size` (pt) picture in an object with bounds `b`, and the
/// paint transform placing the tile there.
fn layout(mode: Mode, (iw, ih): (f64, f64), b: Rect) -> (Rect, Affine, Affine) {
    let (bw, bh) = (b.width().max(1e-6), b.height().max(1e-6));
    match mode {
        Mode::Fill => {
            let k = (bw / iw).max(bh / ih);
            let (w, h) = (iw * k, ih * k);
            (Rect::new(0.0, 0.0, w, h), Affine::scale(k), Affine::translate((b.center().x - w / 2.0, b.center().y - h / 2.0)))
        }
        Mode::Fit => {
            let k = (bw / iw).min(bh / ih);
            let (w, h) = (iw * k, ih * k);
            (Rect::new(0.0, 0.0, bw, bh), Affine::translate(((bw - w) / 2.0, (bh - h) / 2.0)) * Affine::scale(k), Affine::translate((b.x0, b.y0)))
        }
        Mode::Tile(s) => {
            let (w, h) = (iw * s, ih * s);
            (Rect::new(0.0, 0.0, w, h), Affine::scale(s), Affine::translate((b.x0, b.y0)))
        }
    }
}

/// A free `Image Fill N` name.
fn next_name(d: &logocraft_doc::Document) -> String {
    (1..).map(|i| format!("{IMAGE_FILL_PREFIX} {i}")).find(|n| d.pattern(n).is_none()).unwrap_or_else(|| IMAGE_FILL_PREFIX.to_string())
}

/// A picture to lay out: its `Document::images` key, pixel size and points per pixel.
#[derive(Clone, Debug)]
struct Pic {
    key: String,
    width: u32,
    height: u32,
    pt_per_px: (f64, f64),
}

/// The picture of `n`'s current image fill (`stroke`: its stroke's) and that pattern's name.
fn current_pic(d: &logocraft_doc::Document, n: &Node, stroke: bool) -> Option<(Pic, String)> {
    let paint = if stroke { n.appearance.stroke_paint() } else { n.appearance.fill_paint() };
    let Paint::Pattern { pattern, .. } = paint else { return None };
    if !logocraft_doc::pattern::is_image_fill(&pattern) {
        return None;
    }
    let def = d.pattern(&pattern)?;
    let mut found = None;
    for art in &def.art {
        art.walk(&mut |m| {
            if found.is_none()
                && let NodeKind::Image(im) = &m.kind
            {
                found = Some(Pic { key: im.key.clone(), width: im.width, height: im.height, pt_per_px: super::place::pt_per_px(None) });
            }
        });
    }
    Some((found?, pattern))
}

fn image_fill(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "paint.imageFill";
    // A new picture, or (neither given) each object's current image fill laid out again.
    let new = match (str_param(p, "path"), str_param(p, "dataBase64")) {
        (Some(path), _) => Some(super::fileio::raster_image(&super::fileio::read_file(path)?)?),
        (None, Some(b64)) => Some(super::fileio::raster_image(&logocraft_format::base64_decode(b64).ok_or_else(|| bad(C, "bad dataBase64"))?)?),
        _ => None,
    };
    let mode = match str_param(p, "mode").unwrap_or("fill") {
        "fill" => Mode::Fill,
        "fit" => Mode::Fit,
        "tile" => {
            let pct = f64_or(p, "scale", 100.0);
            if !(pct.is_finite() && (1.0..=10_000.0).contains(&pct)) {
                return Err(bad(C, "scale must be 1–10000 (%)"));
            }
            Mode::Tile(pct / 100.0)
        }
        other => return Err(bad(C, format!("mode must be fill, fit or tile, not `{other}`"))),
    };
    let stroke = bool_or(p, "stroke", false);
    let ids: Vec<NodeId> = match ids_param(p, "ids") {
        Some(v) => v,
        None => super::edit::selected_roots(s)?,
    };
    let d = &s.doc()?.doc;
    // The objects that can be painted (paths, compound paths and type, each with bounds), with
    // their picture and, when it is laid out again, its pattern.
    let new_pic =
        new.as_ref().map(|img| Pic { key: img.key.clone(), width: img.width, height: img.height, pt_per_px: super::place::pt_per_px(img.ppi) });
    let targets: Vec<(NodeId, Rect, Pic, Option<String>)> = ids
        .iter()
        .filter_map(|id| {
            d.node(*id).filter(|n| matches!(n.kind, NodeKind::Path { .. } | NodeKind::Compound { .. } | NodeKind::Text(_))).map(|n| (*id, n))
        })
        .filter_map(|(id, n)| {
            let b = n.geometric_bounds()?;
            match &new_pic {
                Some(pic) => Some((id, b, pic.clone(), None)),
                None => current_pic(d, n, stroke).map(|(pic, name)| (id, b, pic, Some(name))),
            }
        })
        .collect();
    if targets.is_empty() {
        return Err(EngineError::Other(if new.is_some() {
            "select a path, compound path or type object".into()
        } else {
            "give `path` or `dataBase64`: the selection has no image fill to lay out again".into()
        }));
    }
    let (out, names) = s.edit("Image Fill", |d, sel| {
        if let Some(img) = &new {
            d.images.entry(img.key.clone()).or_insert_with(|| img.blob.clone());
        }
        let mut names = vec![];
        for (id, b, pic, reuse) in &targets {
            let (sx, sy) = pic.pt_per_px;
            let size = (pic.width as f64 * sx, pic.height as f64 * sy);
            let (tile, art_xf, paint_xf) = layout(mode, size, *b);
            let im = ImageObject {
                key: pic.key.clone(),
                width: pic.width,
                height: pic.height,
                xf: art_xf * Affine::scale_non_uniform(sx, sy),
                link: None,
                placement: Default::default(),
            };
            let art = Node::new(d.alloc_id(), NodeKind::Image(Box::new(im)));
            let name = reuse.clone().unwrap_or_else(|| next_name(d));
            let mut def = PatternDef::new(&name, vec![Arc::new(art)]);
            def.tile = tile;
            match d.patterns.iter_mut().find(|x| x.name == name) {
                Some(old) => *old = def,
                None => d.patterns.push(def),
            }
            let paint = Paint::Pattern { pattern: name.clone(), xf: paint_xf };
            if let Some(n) = d.node_mut(*id) {
                if stroke {
                    n.appearance.set_stroke(paint);
                } else {
                    n.appearance.set_fill(paint);
                }
            }
            names.push(name);
        }
        let out: Vec<NodeId> = targets.iter().map(|(id, ..)| *id).collect();
        sel.set(out.iter().copied());
        Ok((out, names))
    })?;
    Ok(json!({ "ids": out.iter().map(|i| i.0).collect::<Vec<_>>(), "patterns": names }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_covers_fit_fits_and_tile_repeats_from_the_corner() {
        let b = Rect::new(100.0, 50.0, 300.0, 150.0); // 200 × 100
        // A 100 × 100 picture: Fill scales it ×2 to 200 × 200, centred (cropped top and bottom).
        let (tile, art, at) = layout(Mode::Fill, (100.0, 100.0), b);
        assert_eq!(tile, Rect::new(0.0, 0.0, 200.0, 200.0));
        assert_eq!(art, Affine::scale(2.0));
        assert_eq!(at * logocraft_geom::Point::ZERO, logocraft_geom::Point::new(100.0, 0.0));
        // Fit: ×1, centred inside a tile the size of the bounds.
        let (tile, art, at) = layout(Mode::Fit, (100.0, 100.0), b);
        assert_eq!(tile, Rect::new(0.0, 0.0, 200.0, 100.0));
        assert_eq!(art * logocraft_geom::Point::ZERO, logocraft_geom::Point::new(50.0, 0.0));
        assert_eq!(at * logocraft_geom::Point::ZERO, logocraft_geom::Point::new(100.0, 50.0));
        // Tile at 50%: 50 × 50 tiles from the corner.
        let (tile, _, at) = layout(Mode::Tile(0.5), (100.0, 100.0), b);
        assert_eq!(tile, Rect::new(0.0, 0.0, 50.0, 50.0));
        assert_eq!(at * logocraft_geom::Point::ZERO, logocraft_geom::Point::new(100.0, 50.0));
    }
}
