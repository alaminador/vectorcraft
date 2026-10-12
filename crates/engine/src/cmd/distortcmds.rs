//! Commands behind the Width tool: width points and width profiles. The geometry kernels live in
//! `vectorcraft_tools::distort` (shared with the tool).

use serde_json::{Value, json};
use vectorcraft_doc::{Document, NodeId, WidthProfile};
use vectorcraft_tools::distort::stroke_owner;

use super::*;
use crate::EngineError;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "stroke.widthPoint.set",
            "Width Point",
            [],
            None,
            "{id, t: 0..1 (fraction of the path length), left, right: side widths in pt, index?: width point to edit or move, adjustAdjoining?: bool (with index: the nearest points either side change their widths in proportion)} → {index}. Without index a point already at t is replaced; a point moved onto another one's t joins it as a discontinuous point (on the side it came from), so the width steps there. Creates a uniform profile first if needed",
            has_doc,
            width_point_set
        ),
        cmd!(
            "stroke.widthPoint.remove",
            "Delete Width Point",
            [],
            None,
            "{id, index | indices: [..]} remove width points (removing the last one leaves a uniform stroke)",
            has_doc,
            width_point_remove
        ),
        cmd!(
            "stroke.widthPoint.copy",
            "Copy Width Point",
            [],
            None,
            "{id, index: the width point to copy, t: 0..1 where the copy goes} → {index} (Alt-drag with the Width tool). Landing on another point makes a discontinuous point, as a stroke.widthPoint.set move does",
            has_doc,
            width_point_copy
        ),
        cmd!(
            "stroke.widthProfile.set",
            "Width Profile",
            [],
            None,
            "{ids?|id?, points: [[t, left, right]…] (width factors, 1 = the stroke weight) | null = uniform}",
            has_doc,
            width_profile_set
        ),
    ]
}

// ---------- width points ----------

fn uniform() -> WidthProfile {
    WidthProfile { points: vec![(0.0, 1.0, 1.0), (1.0, 1.0, 1.0)] }
}

/// Width points closer than this (in t) are at the same place: a discontinuous point.
const SAME_T: f64 = 1e-6;

/// The stroke of `id` that width point commands edit (a compound path's for its members).
fn stroke_mut(d: &mut Document, id: NodeId) -> Result<&mut vectorcraft_doc::StrokeLayer> {
    let id = stroke_owner(d, id);
    let n = d.node_mut(id).ok_or(EngineError::NoNode(id))?;
    n.appearance.stroke_mut().ok_or_else(|| EngineError::Other("the object has no stroke".into()))
}

/// [`stroke_mut`], which must have a weight.
fn weighted_stroke(d: &mut Document, id: NodeId) -> Result<&mut vectorcraft_doc::StrokeLayer> {
    let st = stroke_mut(d, id)?;
    if st.width <= 0.0 {
        return Err(EngineError::Other("the stroke has no weight".into()));
    }
    Ok(st)
}

/// The `index` param (none when absent or null), checked against the points of `prof`: a malformed
/// one is refused like one out of range, never wrapped round or taken as missing.
fn point_index(p: &Value, prof: &WidthProfile, cmd: &str) -> Result<Option<usize>> {
    let Some(v) = p.get("index").filter(|v| !v.is_null()) else { return Ok(None) };
    match point_indices(&json!([v])).and_then(|i| i.first().copied()) {
        Some(i) if i < prof.points.len() => Ok(Some(i)),
        _ => Err(bad(cmd, "no such width point")),
    }
}

/// Insert width point `pt` in order. Landing on another point's position makes a discontinuous
/// point: `pt` snaps to it and takes the side it came from (`from_below`: before it), replacing
/// that side of a discontinuous point already there. → where it landed.
fn place(points: &mut Vec<(f64, f64, f64)>, mut pt: (f64, f64, f64), from_below: bool) -> usize {
    let at: Vec<usize> = (0..points.len()).filter(|i| (points[*i].0 - pt.0).abs() <= SAME_T).collect();
    let (Some(&first), Some(&last)) = (at.first(), at.last()) else {
        let pos = points.iter().position(|q| q.0 > pt.0).unwrap_or(points.len());
        points.insert(pos, pt);
        return pos;
    };
    pt.0 = points[first].0;
    let pair = at.len() > 1;
    let pos = match (from_below, pair) {
        (true, _) => first,
        (false, true) => last,
        (false, false) => last + 1,
    };
    if pair {
        points.remove(pos);
    }
    points.insert(pos, pt);
    pos
}

/// Adjust Adjoining Width Points: point `i` changes from `old` to `new` widths, and the nearest
/// points at other positions either side change in proportion (by the same amount from zero).
fn adjust_adjoining(points: &mut [(f64, f64, f64)], i: usize, old: (f64, f64, f64), new: (f64, f64, f64)) {
    let t = points[i].0;
    let scale = |v: f64, o: f64, n: f64| if o > 1e-9 { v * n / o } else { v + n - o }.max(0.0);
    let before = (0..i).rev().find(|j| (points[*j].0 - t).abs() > SAME_T);
    let after = (i + 1..points.len()).find(|j| (points[*j].0 - t).abs() > SAME_T);
    for j in before.into_iter().chain(after) {
        let q = &mut points[j];
        (q.1, q.2) = (scale(q.1, old.1, new.1), scale(q.2, old.2, new.2));
    }
}

/// Whether `a` and `b` hold the same width points, each place and width within a rounding error
/// (1e-9).
fn same_points(a: Option<&WidthProfile>, b: Option<&WidthProfile>) -> bool {
    let near = |p: &(f64, f64, f64), q: &(f64, f64, f64)| (p.0 - q.0).abs() <= 1e-9 && (p.1 - q.1).abs() <= 1e-9 && (p.2 - q.2).abs() <= 1e-9;
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.points.len() == b.points.len() && a.points.iter().zip(&b.points).all(|(p, q)| near(p, q)),
        _ => false,
    }
}

fn width_point_set(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stroke.widthPoint.set";
    let id = id_param(p, "id").ok_or_else(|| bad(C, "missing id"))?;
    let t = f64_req(p, "t", C)?.clamp(0.0, 1.0);
    let (left, right) = (f64_req(p, "left", C)?, f64_req(p, "right", C)?);
    if !(left.is_finite() && right.is_finite()) || left < 0.0 || right < 0.0 || left.max(right) > 1.0e5 {
        return Err(bad(C, "left/right must be non-negative widths"));
    }
    let adjust = bool_or(p, "adjustAdjoining", false);
    // When the profile comes out as it was, within a rounding error (1e-9), the document and its undo
    // history stay as they are.
    let mut pos = 0;
    super::overprint::edit_counted(s, "Width Point", |d| {
        let st = weighted_stroke(d, id)?;
        let before = st.profile.clone();
        let half = st.width / 2.0;
        let mut prof = st.profile.take().unwrap_or_else(uniform);
        let pt = (t, left / half, right / half);
        pos = match point_index(p, &prof, C)? {
            Some(i) => {
                let old = prof.points[i];
                if adjust {
                    adjust_adjoining(&mut prof.points, i, old, pt);
                }
                if (old.0 - t).abs() <= SAME_T {
                    // Widths only: it keeps its place (and its side of a discontinuous point).
                    prof.points[i] = (old.0, pt.1, pt.2);
                    i
                } else {
                    prof.points.remove(i);
                    place(&mut prof.points, pt, old.0 < t)
                }
            }
            None => {
                prof.points.retain(|q| (q.0 - t).abs() > SAME_T);
                place(&mut prof.points, pt, true)
            }
        };
        st.profile = Some(prof);
        Ok(usize::from(!same_points(st.profile.as_ref(), before.as_ref())))
    })?;
    Ok(json!({ "index": pos }))
}

fn width_point_copy(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stroke.widthPoint.copy";
    let id = id_param(p, "id").ok_or_else(|| bad(C, "missing id"))?;
    let t = f64_req(p, "t", C)?.clamp(0.0, 1.0);
    // When the profile comes out as it was (the copy replaces a side of a discontinuous point that has
    // its widths already), the document and its undo history stay as they are.
    let mut pos = 0;
    super::overprint::edit_counted(s, "Copy Width Point", |d| {
        let st = weighted_stroke(d, id)?;
        let before = st.profile.clone();
        let mut prof = st.profile.take().unwrap_or_else(uniform);
        let i = point_index(p, &prof, C)?.ok_or_else(|| bad(C, "missing index"))?;
        let (from, l, r) = prof.points[i];
        pos = place(&mut prof.points, (t, l, r), from < t);
        st.profile = Some(prof);
        Ok(usize::from(st.profile != before))
    })?;
    Ok(json!({ "index": pos }))
}

fn width_point_remove(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stroke.widthPoint.remove";
    let id = id_param(p, "id").ok_or_else(|| bad(C, "missing id"))?;
    let indices = match (p.get("indices"), p.get("index")) {
        (Some(a), _) => point_indices(a),
        (None, Some(i)) => point_indices(&json!([i])),
        (None, None) => return Err(bad(C, "missing index")),
    };
    // A malformed index fails the whole delete instead of being dropped while the others go.
    let mut indices = indices.ok_or_else(|| bad(C, "no such width point"))?;
    indices.sort_unstable();
    indices.dedup();
    s.edit("Delete Width Point", |d, _| {
        let st = stroke_mut(d, id)?;
        let fits = |pr: &&mut WidthProfile| indices.last().is_some_and(|i| *i < pr.points.len());
        let prof = st.profile.as_mut().filter(fits).ok_or_else(|| bad(C, "no such width point"))?;
        for i in indices.iter().rev() {
            prof.points.remove(*i);
        }
        if prof.points.is_empty() {
            st.profile = None;
        }
        Ok(())
    })?;
    ok()
}

fn width_profile_set(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stroke.widthProfile.set";
    let profile = match p.get("points") {
        None => return Err(bad(C, "missing points")),
        Some(Value::Null) => None,
        Some(Value::Array(a)) if a.is_empty() => None,
        Some(Value::Array(a)) => {
            let mut pts = vec![];
            for q in a {
                let v: Vec<f64> = q.as_array().map(|x| x.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
                if v.len() != 3 || v.iter().any(|x| !x.is_finite()) || v[1] < 0.0 || v[2] < 0.0 {
                    return Err(bad(C, "points must be [t, left, right] with non-negative widths"));
                }
                pts.push((v[0].clamp(0.0, 1.0), v[1], v[2]));
            }
            pts.sort_by(|a, b| a.0.total_cmp(&b.0));
            Some(WidthProfile { points: pts })
        }
        _ => return Err(bad(C, "points must be an array or null")),
    };
    let doc = &s.doc()?.doc;
    let mut ids: Vec<NodeId> = targets(s, p)?.into_iter().map(|id| stroke_owner(doc, id)).collect();
    ids.sort_unstable();
    ids.dedup();
    // When every stroke has the profile already, or there is no stroke, the document and its undo
    // history stay as they are.
    if ids.iter().all(|id| doc.node(*id).is_some_and(|n| n.appearance.stroke().is_none_or(|st| st.profile == profile))) {
        return ok();
    }
    s.edit("Width Profile", |d, _| {
        for id in &ids {
            let n = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?;
            if let Some(st) = n.appearance.stroke_mut() {
                st.profile = profile.clone();
            }
        }
        Ok(())
    })?;
    ok()
}
