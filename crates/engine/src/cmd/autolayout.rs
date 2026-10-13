//! Auto layout, the Figma way ([`logocraft_doc::layout`]): Add Auto Layout (Shift+A) makes the
//! selection a frame that lays its children out in a row or a column, with a gap, padding,
//! alignment, Hug or Fixed sizing and wrapping; each child keeps its size, fills the frame or is
//! positioned absolutely.
//!
//! [`sync`] runs after every edit, so frames follow whatever changes inside them:
//! - every frame is laid out again, innermost first (a child that fills a frame is resized);
//! - a child dragged within its frame takes the place it was dropped at; dragged out of the
//!   frame, it leaves it;
//! - a frame resized as a whole keeps its children's sizes and becomes Fixed along the axes that
//!   changed, as in Figma.

use std::sync::Arc;

use logocraft_color::{Color, Paint};
use logocraft_doc::layout::{Align, Arranged, AutoLayout, Direction, ItemSizing, LayoutItem, Sizing, Slot, arrange};
use logocraft_doc::{Document, LiveShape, Node, NodeId, NodeKind, Scaling, TextKind};
use logocraft_geom::{Affine, Point, Rect};
use serde_json::{Value, json};

use super::*;
use crate::EngineError;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "layout.add",
            "Add Auto Layout",
            ["Object", "Auto Layout"],
            Some("Shift+A"),
            "{ids?, direction?: horizontal|vertical (default: how the objects are arranged), gap?: pt (default: the space between them)} make the selection an auto layout frame: a group laid out in a row or a column (a group, or several objects, become its children in their order on the page; a frame is wrapped in a new one) → {id}",
            has_selection,
            add
        ),
        cmd!(
            "layout.remove",
            "Remove Auto Layout",
            ["Object", "Auto Layout"],
            Some("Alt+Shift+A"),
            "{ids?} turn the selected frames back into plain groups, their children where they are (a background that paints nothing goes) → {ids}",
            has_selection,
            remove
        ),
        cmd!(
            "layout.set",
            "Auto Layout Settings",
            [],
            None,
            "{ids? (default: the selected frames, or the frames of the selected children), direction?: horizontal|vertical, wrap?: bool, gap?: pt | \"auto\" (space between), crossGap?: pt (between wrapped rows), padding?: pt | [top, right, bottom, left], paddingH?, paddingV?, paddingTop?, paddingRight?, paddingBottom?, paddingLeft?, align?: top-left|top|top-right|left|center|right|bottom-left|bottom|bottom-right, alignMain?, alignCross?: start|center|end, width?, height?: hug | fixed | pt (fixed at that size), fill?: \"#hex\" | null (none), stroke?: \"#hex\" | null, strokeWidth?: pt, radius?: pt} change auto layout frames → {ids}",
            has_selection,
            set
        ),
        cmd!(
            "layout.setItem",
            "Auto Layout Child",
            [],
            None,
            "{ids? (default: the selection), width?, height?: fixed | hug | fill (hug and fixed set a child frame's own sizing), absolute?: bool} how children of auto layout frames size and whether they flow → {ids}",
            has_selection,
            set_item
        ),
        cmd!(query "layout.get", "Auto Layout", [], None, "{id?} the selected frame's settings (or the frame of the selected child) → {id, direction, gap, gapAuto, padding, alignMain, alignCross, width, height, wrap, crossGap, frame: [x, y, w, h], children: [{id, width, height, absolute}]}", has_doc, get),
    ]
}

// ---------- reading a frame ----------

/// The auto layout of `n`, if it is a frame.
fn layout_of(n: &Node) -> Option<&AutoLayout> {
    n.extras().layout.as_deref()
}

/// A child's settings (defaults when it has none).
fn item_of(n: &Node) -> LayoutItem {
    n.extras().layout_item.unwrap_or_default()
}

/// Does `n` take part in its frame's flow?
fn flows(n: &Node) -> bool {
    n.visible && item_of(n).flows()
}

/// The bounds a frame lays a child out by: a frame's own bounds, else its geometric bounds.
fn item_bounds(n: &Node) -> Option<Rect> {
    match layout_of(n) {
        Some(l) => Some(l.frame),
        None => n.geometric_bounds(),
    }
    .filter(|r| r.x0.is_finite() && r.y0.is_finite() && r.x1.is_finite() && r.y1.is_finite())
}

/// Point type can't change its size by its frame: it always hugs.
fn can_fill(n: &Node) -> bool {
    !matches!(&n.kind, NodeKind::Text(t) if !matches!(t.kind, TextKind::Area { .. }))
}

// ---------- laying out ----------

/// Move `n` by `(dx, dy)`.
fn translate(n: &mut Node, dx: f64, dy: f64) {
    if (dx.abs() > 1e-9 || dy.abs() > 1e-9) && dx.is_finite() && dy.is_finite() {
        n.transform(Affine::translate((dx, dy)), false);
    }
}

/// Resize `n` to `w` × `h` (either kept when `None`), keeping its top-left corner: a frame lays
/// itself out at that size, area type resizes its frame, other art scales.
fn resize(n: &mut Node, w: Option<f64>, h: Option<f64>) {
    if w.is_none() && h.is_none() {
        return;
    }
    if layout_of(n).is_some() {
        layout_node(n, (w, h));
        return;
    }
    let Some(b) = n.geometric_bounds() else { return };
    let sx = w.filter(|_| b.width() > 1e-9).map_or(1.0, |w| w / b.width());
    let sy = h.filter(|_| b.height() > 1e-9).map_or(1.0, |h| h / b.height());
    if !(sx.is_finite() && sy.is_finite()) || ((sx - 1.0).abs() < 1e-9 && (sy - 1.0).abs() < 1e-9) {
        return;
    }
    let a = Affine::translate(b.origin().to_vec2()) * Affine::scale_non_uniform(sx, sy) * Affine::translate(-b.origin().to_vec2());
    if let NodeKind::Text(t) = &mut n.kind {
        // Area type: its frame resizes and the type reflows (type that is rotated or skewed stays).
        if let TextKind::Area { frame } = &mut t.kind {
            let [_, b1, c1, _, _, _] = t.xf.as_coeffs();
            if b1.abs() < 1e-9 && c1.abs() < 1e-9 {
                let inv = t.xf.inverse();
                frame.transform(inv * a * t.xf);
            }
        }
        return;
    }
    n.transform(a, Scaling { keep_corners: true, ..Default::default() });
}

/// Keep a frame's background child at its bounds.
fn fit_background(n: &mut Node, r: Rect) {
    if let NodeKind::Path { path, live: Some(l), .. } = &mut n.kind
        && let LiveShape::Rectangle { w, h, xf, .. } = &mut **l
    {
        if (*w - r.width()).abs() > 1e-9 || (*h - r.height()).abs() > 1e-9 || (*xf * Point::ZERO - r.origin()).hypot() > 1e-9 {
            *w = r.width();
            *h = r.height();
            *xf = Affine::translate(r.origin().to_vec2());
            *path = l.to_path();
        }
        return;
    }
    // A background that is no longer a rectangle is stretched to the bounds.
    resize(n, Some(r.width()), Some(r.height()));
    if let Some(b) = n.geometric_bounds() {
        translate(n, r.x0 - b.x0, r.y0 - b.y0);
    }
}

/// Lay out frame `n` (its children, its bounds and its background); `forced` sizes it along
/// either axis (a parent's Fill container), whatever its own sizing.
fn layout_node(n: &mut Node, forced: (Option<f64>, Option<f64>)) {
    let Some(stored) = layout_of(n).cloned() else { return };
    let mut l = stored.clone();
    if let Some(w) = forced.0.filter(|w| w.is_finite() && *w >= 0.0) {
        l.width = Sizing::Fixed;
        l.frame.x1 = l.frame.x0 + w;
    }
    if let Some(h) = forced.1.filter(|h| h.is_finite() && *h >= 0.0) {
        l.height = Sizing::Fixed;
        l.frame.y1 = l.frame.y0 + h;
    }
    let Some(children) = n.children_mut() else { return };
    // The children in the flow, with their bounds.
    let flow: Vec<(usize, Rect)> =
        children.iter().enumerate().filter(|(_, c)| flows(c)).filter_map(|(i, c)| item_bounds(c).map(|b| (i, b))).collect();
    let slots: Vec<Slot> = flow
        .iter()
        .filter_map(|(i, b)| {
            let c = children.get(*i)?;
            let it = item_of(c);
            let fill = can_fill(c);
            Some(Slot { w: b.width(), h: b.height(), fill_w: fill && it.width == ItemSizing::Fill, fill_h: fill && it.height == ItemSizing::Fill })
        })
        .collect();
    let Arranged { frame, items } = arrange(&l, &slots);
    for ((i, b), (target, slot)) in flow.iter().zip(items.iter().zip(&slots)) {
        let Some(c) = children.get_mut(*i) else { continue };
        let want_w = (slot.fill_w && (target.width() - b.width()).abs() > 1e-6).then_some(target.width());
        let want_h = (slot.fill_h && (target.height() - b.height()).abs() > 1e-6).then_some(target.height());
        let moved = (target.x0 - b.x0).abs() > 1e-9 || (target.y0 - b.y0).abs() > 1e-9;
        if want_w.is_none() && want_h.is_none() && !moved {
            continue;
        }
        let c = Arc::make_mut(c);
        resize(c, want_w, want_h);
        if let Some(now) = item_bounds(c) {
            translate(c, target.x0 - now.x0, target.y0 - now.y0);
        }
    }
    if let Some(bg) = children.iter_mut().find(|c| item_of(c).background)
        && item_bounds(bg) != Some(frame)
    {
        fit_background(Arc::make_mut(bg), frame);
    }
    if stored.frame != frame
        && let Some(e) = n.extras_if_any()
        && let Some(l) = e.layout.as_mut()
    {
        l.frame = frame;
    }
}

/// The frames in `d`, innermost first.
fn frames(d: &Document) -> Vec<NodeId> {
    fn visit(n: &Node, out: &mut Vec<NodeId>) {
        if let Some(ch) = n.children() {
            for c in ch {
                visit(c, out);
            }
        }
        if layout_of(n).is_some() {
            out.push(n.id);
        }
    }
    let mut out = vec![];
    for l in &d.layers {
        visit(l, &mut out);
    }
    out
}

/// Lay out every frame of `d`, innermost first.
pub(crate) fn reflow(d: &mut Document) {
    for id in frames(d) {
        if let Some(n) = d.node_mut(id) {
            layout_node(n, (None, None));
        }
    }
}

/// Keep auto layout frames laid out after an edit (`before`: the document before it).
pub(crate) fn sync(before: &Document, d: &mut Document) {
    let ids = frames(d);
    if ids.is_empty() {
        return;
    }
    for id in &ids {
        follow_edit(before, d, *id);
    }
    reflow(d);
}

/// Read what the user did to frame `id`: resized it as a whole, or dragged a child.
fn follow_edit(before: &Document, d: &mut Document, id: NodeId) {
    let (Some(old), Some(new)) = (before.node(id), d.node(id)) else { return };
    let (Some(lo), Some(ln)) = (layout_of(old), layout_of(new)) else { return };
    if !ln.same_settings(lo) {
        return;
    }
    let (ow, oh, nw, nh) = (lo.frame.width(), lo.frame.height(), ln.frame.width(), ln.frame.height());
    if (ow - nw).abs() > 1e-6 || (oh - nh).abs() > 1e-6 {
        resized(old, d, id, (nw - ow).abs() > 1e-6, (nh - oh).abs() > 1e-6);
        return;
    }
    dragged(old, d, id);
}

/// Frame `id` was resized as a whole (its children scaled with it): its children get back their
/// sizes, and it becomes Fixed along the axes that changed.
fn resized(old: &Node, d: &mut Document, id: NodeId, w: bool, h: bool) {
    let Some(old_children) = old.children() else { return };
    let Some(n) = d.node_mut(id) else { return };
    let Some(frame) = layout_of(n).map(|l| l.frame) else { return };
    let origin = layout_of(old).map(|l| l.frame.origin()).unwrap_or(frame.origin());
    if let Some(children) = n.children_mut() {
        for c in children.iter_mut() {
            if let Some(o) = old_children.iter().find(|o| o.id == c.id) {
                let mut back = (**o).clone();
                translate(&mut back, frame.x0 - origin.x, frame.y0 - origin.y);
                *c = Arc::new(back);
            }
        }
    }
    if let Some(l) = n.extras_if_any().and_then(|e| e.layout.as_mut()) {
        if w {
            l.width = Sizing::Fixed;
        }
        if h {
            l.height = Sizing::Fixed;
        }
    }
}

/// Children of frame `id` moved without changing size (dragged): each takes the place it was
/// dropped at, or leaves the frame when dropped outside it.
fn dragged(old: &Node, d: &mut Document, id: NodeId) {
    let Some(new) = d.node(id) else { return };
    let (Some(l), Some(children), Some(old_children)) = (layout_of(new), new.children(), old.children()) else { return };
    let horizontal = l.direction == Direction::Horizontal;
    let frame = l.frame;
    let mut moved: Vec<NodeId> = vec![];
    for c in children.iter().filter(|c| flows(c)) {
        let Some(o) = old_children.iter().find(|o| o.id == c.id) else { continue };
        let (Some(a), Some(b)) = (item_bounds(o), item_bounds(c)) else { continue };
        let same_size = (a.width() - b.width()).abs() < 1e-6 && (a.height() - b.height()).abs() < 1e-6;
        if same_size && (a.origin() - b.origin()).hypot() > 1e-6 {
            moved.push(c.id);
        }
    }
    if moved.is_empty() {
        return;
    }
    // Dropped outside the frame: out of it, into the frame's parent above it.
    let (out, inside): (Vec<NodeId>, Vec<NodeId>) = moved
        .into_iter()
        .partition(|m| children.iter().find(|c| c.id == *m).and_then(|c| item_bounds(c)).is_some_and(|b| !frame.contains(b.center())));
    if !out.is_empty()
        && let Some((parent, idx, _)) = d.position(id)
    {
        for (k, m) in out.iter().enumerate() {
            if let Ok(node) = d.remove(*m) {
                let mut node = (*node).clone();
                node.extras_mut().layout_item = None;
                // Failing to put it back would lose it: put it back in the frame instead.
                if let Err(e) = d.insert(parent, idx + 1 + k, node.clone()) {
                    log::warn!("auto layout: couldn't move a child out of its frame: {e}");
                    let _ = d.insert(Some(id), usize::MAX, node);
                }
            }
        }
    }
    if inside.is_empty() {
        return;
    }
    // Dropped inside: the flow children in the order of their centres along the direction,
    // the moved ones placed among the others where they were dropped.
    let Some(n) = d.node_mut(id) else { return };
    let Some(children) = n.children_mut() else { return };
    let slots: Vec<usize> = children.iter().enumerate().filter(|(_, c)| flows(c)).map(|(i, _)| i).collect();
    let key = |c: &Node| item_bounds(c).map(|b| if horizontal { b.center().x } else { b.center().y }).unwrap_or(0.0);
    let mut order: Vec<Arc<Node>> = slots.iter().filter_map(|i| children.get(*i).cloned()).filter(|c| !inside.contains(&c.id)).collect();
    for m in &inside {
        let Some(c) = children.iter().find(|c| c.id == *m).cloned() else { continue };
        let k = key(&c);
        let at = order.iter().position(|o| key(o) > k).unwrap_or(order.len());
        order.insert(at, c);
    }
    for (slot, c) in slots.iter().zip(order) {
        if let Some(s) = children.get_mut(*slot) {
            *s = c;
        }
    }
}

// ---------- commands ----------

/// A new frame background: an unpainted rectangle at `r`.
fn background(d: &mut Document, r: Rect) -> Node {
    let live =
        LiveShape::Rectangle { w: r.width(), h: r.height(), radii: [0.0; 4], kinds: Default::default(), xf: Affine::translate(r.origin().to_vec2()) };
    let mut bg = Node::new(
        d.alloc_id(),
        NodeKind::Path { path: live.to_path(), rule: Default::default(), live: Some(Box::new(live)), clipping: false, guide: false },
    );
    bg.name = Some("Background".into());
    bg.extras_mut().layout_item = Some(LayoutItem { background: true, ..Default::default() });
    bg
}

/// A free "Frame N" name.
fn frame_name(d: &Document) -> String {
    let mut used = std::collections::HashSet::new();
    d.walk(|n| {
        if let Some(name) = &n.name {
            used.insert(name.clone());
        }
    });
    (1..).map(|i| format!("Frame {i}")).find(|n| !used.contains(n)).unwrap_or_else(|| "Frame".into())
}

/// The direction and gap objects with bounds `bs` are arranged in: along the axis their centres
/// spread most, with the mean space between neighbours (never below 0).
pub(crate) fn infer(bs: &[Rect]) -> (Direction, f64) {
    let span = |f: &dyn Fn(&Rect) -> f64| {
        let (lo, hi) = bs.iter().map(f).fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));
        if hi >= lo { hi - lo } else { 0.0 }
    };
    let dir = if span(&|r| r.center().x) >= span(&|r| r.center().y) { Direction::Horizontal } else { Direction::Vertical };
    let mut sorted: Vec<Rect> = bs.to_vec();
    sorted.sort_by(|a, b| if dir == Direction::Horizontal { a.x0.total_cmp(&b.x0) } else { a.y0.total_cmp(&b.y0) });
    let gaps: Vec<f64> = sorted
        .windows(2)
        .filter_map(|w| match w {
            [a, b] => Some(if dir == Direction::Horizontal { b.x0 - a.x1 } else { b.y0 - a.y1 }),
            _ => None,
        })
        .collect();
    let gap = if gaps.is_empty() { 10.0 } else { (gaps.iter().sum::<f64>() / gaps.len() as f64).max(0.0).round() };
    (dir, gap)
}

fn direction_param(p: &Value, cmd: &str) -> Result<Option<Direction>> {
    match str_param(p, "direction") {
        None => Ok(None),
        Some("horizontal" | "row" | "h") => Ok(Some(Direction::Horizontal)),
        Some("vertical" | "column" | "v") => Ok(Some(Direction::Vertical)),
        Some(o) => Err(bad(cmd, format!("direction must be horizontal or vertical, not `{o}`"))),
    }
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "layout.add";
    let ids = super::brushsym::roots(s, p)?;
    if ids.is_empty() {
        return Err(EngineError::Other("nothing selected".into()));
    }
    let direction = direction_param(p, C)?;
    let gap = match p.get("gap") {
        Some(_) => Some(f64_req(p, "gap", C)?).filter(|g| g.is_finite()),
        None => None,
    };
    let d = &s.doc()?.doc;
    // One plain group becomes the frame; anything else is wrapped in a new one.
    let convert = match ids.as_slice() {
        [one] => d.node(*one).filter(|n| matches!(n.kind, NodeKind::Group { clip: false, .. }) && layout_of(n).is_none()).map(|n| n.id),
        _ => None,
    };
    let members: Vec<(NodeId, Rect)> = match convert {
        Some(g) => d
            .node(g)
            .and_then(|n| n.children())
            .map(|ch| ch.iter().filter_map(|c| c.geometric_bounds().map(|b| (c.id, b))).collect())
            .unwrap_or_default(),
        None => ids.iter().filter_map(|id| d.node(*id).and_then(item_bounds).map(|b| (*id, b))).collect(),
    };
    if members.is_empty() {
        return Err(EngineError::Other("the selection has nothing to lay out".into()));
    }
    let bounds: Vec<Rect> = members.iter().map(|(_, b)| *b).collect();
    let (dir0, gap0) = infer(&bounds);
    let dir = direction.unwrap_or(dir0);
    let gap = gap.unwrap_or(gap0);
    let union = bounds.iter().skip(1).fold(bounds.first().copied().unwrap_or(Rect::ZERO), |a, b| a.union(*b));
    // Children in the order they stand along the direction.
    let mut order = members.clone();
    order.sort_by(|a, b| if dir == Direction::Horizontal { a.1.x0.total_cmp(&b.1.x0) } else { a.1.y0.total_cmp(&b.1.y0) });
    let name = frame_name(d);
    let id = s.edit("Add Auto Layout", |d, sel| {
        let layout = AutoLayout::new(dir, gap, union);
        let id = match convert {
            Some(g) => {
                let bg = background(d, union);
                let n = d.node_mut(g).ok_or(EngineError::NoNode(g))?;
                let Some(children) = n.children_mut() else { return Err(EngineError::Other("not a group".into())) };
                let mut sorted: Vec<Arc<Node>> = order.iter().filter_map(|(id, _)| children.iter().find(|c| c.id == *id).cloned()).collect();
                let rest: Vec<Arc<Node>> = children.iter().filter(|c| !order.iter().any(|(id, _)| *id == c.id)).cloned().collect();
                sorted.extend(rest);
                sorted.insert(0, Arc::new(bg));
                *children = sorted;
                n.extras_mut().layout = Some(Box::new(layout));
                if n.name.is_none() {
                    n.name = Some(name.clone());
                }
                g
            }
            None => {
                let top = *ids.last().ok_or_else(|| EngineError::Other("nothing selected".into()))?;
                let (par, idx, _) = d.position(top).ok_or(EngineError::NoNode(top))?;
                let mut children: Vec<Arc<Node>> = vec![Arc::new(background(d, union))];
                children.extend(order.iter().filter_map(|(id, _)| d.node(*id).cloned().map(Arc::new)));
                let gid = d.alloc_id();
                let mut group = Node::group(gid, children);
                group.name = Some(name.clone());
                group.extras_mut().layout = Some(Box::new(layout));
                d.insert(par, idx + 1, group)?;
                for (r, _) in &order {
                    d.remove(*r)?;
                }
                gid
            }
        };
        sel.set([id]);
        Ok(id)
    })?;
    Ok(json!({ "id": id.0 }))
}

/// The frames a command acts on: the given or selected frames, or the frames of the given or
/// selected children.
fn target_frames(s: &Session, p: &Value) -> Result<Vec<NodeId>> {
    let ids = super::brushsym::roots(s, p)?;
    let d = &s.doc()?.doc;
    let mut out = vec![];
    for id in ids {
        let f =
            if d.node(id).and_then(layout_of).is_some() { Some(id) } else { d.parent_of(id).filter(|p| d.node(*p).and_then(layout_of).is_some()) };
        if let Some(f) = f
            && !out.contains(&f)
        {
            out.push(f);
        }
    }
    if out.is_empty() {
        return Err(EngineError::Other("select an auto layout frame".into()));
    }
    Ok(out)
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = target_frames(s, p)?;
    s.edit("Remove Auto Layout", |d, sel| {
        for id in &ids {
            let Some(n) = d.node_mut(*id) else { continue };
            n.extras_mut().layout = None;
            if let Some(children) = n.children_mut() {
                // A background that paints nothing goes; one that paints stays as a rectangle.
                children.retain(|c| !(item_of(c).background && c.appearance.items.iter().all(|i| i.paint().is_none())));
                for c in children.iter_mut() {
                    if c.extras().layout_item.is_some() {
                        Arc::make_mut(c).extras_mut().layout_item = None;
                    }
                }
            }
        }
        sel.set(ids.iter().copied());
        Ok(())
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

/// A 9-grid alignment name as (x, y).
fn grid_align(s: &str) -> Option<(Align, Align)> {
    let (y, x) = match s.to_ascii_lowercase().as_str() {
        "top-left" => ("start", "start"),
        "top" | "top-center" => ("start", "center"),
        "top-right" => ("start", "end"),
        "left" | "center-left" => ("center", "start"),
        "center" | "middle" => ("center", "center"),
        "right" | "center-right" => ("center", "end"),
        "bottom-left" => ("end", "start"),
        "bottom" | "bottom-center" => ("end", "center"),
        "bottom-right" => ("end", "end"),
        _ => return None,
    };
    Some((Align::parse(x)?, Align::parse(y)?))
}

/// A sizing param: `hug`, `fixed`, or a size (fixed at it).
fn sizing_param(p: &Value, key: &str, cmd: &str) -> Result<Option<(Sizing, Option<f64>)>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s == "hug" => Ok(Some((Sizing::Hug, None))),
        Some(Value::String(s)) if s == "fixed" => Ok(Some((Sizing::Fixed, None))),
        Some(v) => match v.as_f64().filter(|v| v.is_finite() && *v >= 0.0 && *v <= 1e7) {
            Some(n) => Ok(Some((Sizing::Fixed, Some(n)))),
            None => Err(bad(cmd, format!("{key} must be hug, fixed or a size in pt"))),
        },
    }
}

/// A colour param: `#hex`, or null / "none" for none.
fn paint_param(p: &Value, key: &str, cmd: &str) -> Result<Option<Paint>> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(Paint::None)),
        Some(Value::String(s)) if s == "none" => Ok(Some(Paint::None)),
        Some(Value::String(s)) => Color::from_hex(s).map(|c| Some(Paint::solid(c))).ok_or_else(|| bad(cmd, format!("{key} must be #rrggbb or null"))),
        Some(_) => Err(bad(cmd, format!("{key} must be #rrggbb or null"))),
    }
}

fn finite_param(p: &Value, key: &str, cmd: &str) -> Result<Option<f64>> {
    match p.get(key) {
        None => Ok(None),
        Some(v) => v.as_f64().filter(|v| v.is_finite() && v.abs() <= 1e6).map(Some).ok_or_else(|| bad(cmd, format!("{key} must be a number"))),
    }
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "layout.set";
    let ids = target_frames(s, p)?;
    let direction = direction_param(p, C)?;
    let wrap = p.get("wrap").and_then(Value::as_bool);
    let (gap, gap_auto) = match p.get("gap") {
        Some(Value::String(g)) if g == "auto" => (None, Some(true)),
        Some(_) => (finite_param(p, "gap", C)?, Some(false)),
        None => (None, None),
    };
    let cross_gap = finite_param(p, "crossGap", C)?;
    let mut padding: [Option<f64>; 4] = [None; 4];
    match p.get("padding") {
        Some(Value::Array(a)) => {
            let v: Vec<f64> = a.iter().filter_map(Value::as_f64).filter(|v| v.is_finite()).collect();
            let [t, r, b, l] = v.as_slice() else { return Err(bad(C, "padding must be a number or [top, right, bottom, left]")) };
            padding = [Some(*t), Some(*r), Some(*b), Some(*l)];
        }
        Some(_) => padding = [finite_param(p, "padding", C)?; 4],
        None => {}
    }
    if let Some(h) = finite_param(p, "paddingH", C)? {
        padding[1] = Some(h);
        padding[3] = Some(h);
    }
    if let Some(v) = finite_param(p, "paddingV", C)? {
        padding[0] = Some(v);
        padding[2] = Some(v);
    }
    for (i, k) in ["paddingTop", "paddingRight", "paddingBottom", "paddingLeft"].iter().enumerate() {
        if let Some(v) = finite_param(p, k, C)?
            && let Some(slot) = padding.get_mut(i)
        {
            *slot = Some(v);
        }
    }
    let grid = match str_param(p, "align") {
        Some(a) => Some(grid_align(a).ok_or_else(|| bad(C, format!("align must be top-left … bottom-right, not `{a}`")))?),
        None => None,
    };
    let align_param = |k: &str| -> Result<Option<Align>> {
        match str_param(p, k) {
            Some(a) => Align::parse(a).map(Some).ok_or_else(|| bad(C, format!("{k} must be start, center or end"))),
            None => Ok(None),
        }
    };
    let (align_main, align_cross) = (align_param("alignMain")?, align_param("alignCross")?);
    let width = sizing_param(p, "width", C)?;
    let height = sizing_param(p, "height", C)?;
    let fill = paint_param(p, "fill", C)?;
    let stroke = paint_param(p, "stroke", C)?;
    let stroke_width = finite_param(p, "strokeWidth", C)?.map(|w| w.max(0.0));
    let radius = finite_param(p, "radius", C)?.map(|r| r.max(0.0));
    s.edit("Auto Layout", |d, _| {
        for id in &ids {
            let bg_rect;
            {
                let Some(n) = d.node_mut(*id) else { continue };
                let Some(l) = n.extras_if_any().and_then(|e| e.layout.as_mut()) else { continue };
                if let Some(v) = direction {
                    l.direction = v;
                }
                if let Some(v) = wrap {
                    l.wrap = v;
                }
                if let Some(v) = gap {
                    l.gap = v;
                }
                if let Some(v) = gap_auto {
                    l.gap_auto = v;
                }
                if let Some(v) = cross_gap {
                    l.cross_gap = v;
                }
                for (slot, v) in l.padding.iter_mut().zip(padding) {
                    if let Some(v) = v {
                        *slot = v.max(0.0);
                    }
                }
                if let Some((x, y)) = grid {
                    (l.align_main, l.align_cross) = if l.direction == Direction::Horizontal { (x, y) } else { (y, x) };
                }
                if let Some(v) = align_main {
                    l.align_main = v;
                }
                if let Some(v) = align_cross {
                    l.align_cross = v;
                }
                if let Some((sz, size)) = width {
                    l.width = sz;
                    if let Some(w) = size {
                        l.frame.x1 = l.frame.x0 + w;
                    }
                }
                if let Some((sz, size)) = height {
                    l.height = sz;
                    if let Some(h) = size {
                        l.frame.y1 = l.frame.y0 + h;
                    }
                }
                bg_rect = l.frame;
            }
            if fill.is_none() && stroke.is_none() && stroke_width.is_none() && radius.is_none() {
                continue;
            }
            // Fill, stroke and corners belong to the background (made when the frame lost it).
            let has_bg = d.node(*id).and_then(|n| n.children()).is_some_and(|ch| ch.iter().any(|c| item_of(c).background));
            if !has_bg {
                let bg = background(d, bg_rect);
                d.insert(Some(*id), 0, bg)?;
            }
            let Some(children) = d.node_mut(*id).and_then(|n| n.children_mut()) else { continue };
            let Some(bg) = children.iter_mut().find(|c| item_of(c).background) else { continue };
            let bg = Arc::make_mut(bg);
            if let Some(f) = &fill {
                bg.appearance.set_fill(f.clone());
            }
            if let Some(st) = &stroke {
                bg.appearance.set_stroke(st.clone());
            }
            if let Some(w) = stroke_width
                && let Some(st) = bg.appearance.stroke_mut()
            {
                st.width = w;
            }
            if let Some(r) = radius
                && let NodeKind::Path { path, live: Some(l), .. } = &mut bg.kind
                && let LiveShape::Rectangle { radii, .. } = &mut **l
            {
                *radii = [r; 4];
                *path = l.to_path();
            }
        }
        Ok(())
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

fn item_sizing(p: &Value, key: &str, cmd: &str) -> Result<Option<(ItemSizing, Option<Sizing>)>> {
    match str_param(p, key) {
        None => Ok(None),
        Some("fill") => Ok(Some((ItemSizing::Fill, None))),
        Some("fixed" | "auto") => Ok(Some((ItemSizing::Auto, Some(Sizing::Fixed)))),
        Some("hug") => Ok(Some((ItemSizing::Auto, Some(Sizing::Hug)))),
        Some(o) => Err(bad(cmd, format!("{key} must be fixed, hug or fill, not `{o}`"))),
    }
}

fn set_item(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "layout.setItem";
    let ids = match ids_param(p, "ids") {
        Some(v) => v,
        None => s.doc()?.selection.objects.clone(),
    };
    let width = item_sizing(p, "width", C)?;
    let height = item_sizing(p, "height", C)?;
    let absolute = p.get("absolute").and_then(Value::as_bool);
    let d = &s.doc()?.doc;
    let children: Vec<NodeId> = ids.into_iter().filter(|id| d.parent_of(*id).and_then(|p| d.node(p)).and_then(layout_of).is_some()).collect();
    if children.is_empty() {
        return Err(EngineError::Other("select objects inside an auto layout frame".into()));
    }
    s.edit("Auto Layout Child", |d, _| {
        for id in &children {
            let Some(n) = d.node_mut(*id) else { continue };
            let mut it = item_of(n);
            if let Some((w, own)) = width {
                it.width = w;
                if let (Some(own), Some(l)) = (own, n.extras_if_any().and_then(|e| e.layout.as_mut())) {
                    l.width = own;
                }
            }
            if let Some((h, own)) = height {
                it.height = h;
                if let (Some(own), Some(l)) = (own, n.extras_if_any().and_then(|e| e.layout.as_mut())) {
                    l.height = own;
                }
            }
            if let Some(a) = absolute {
                it.absolute = a;
            }
            n.extras_mut().layout_item = (it != LayoutItem::default()).then_some(it);
        }
        Ok(())
    })?;
    Ok(json!({ "ids": children.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

fn get(s: &mut Session, p: &Value) -> Result<Value> {
    let id = *target_frames(s, p)?.first().ok_or_else(|| EngineError::Other("select an auto layout frame".into()))?;
    let d = &s.doc()?.doc;
    let n = d.node(id).ok_or(EngineError::NoNode(id))?;
    let l = layout_of(n).ok_or_else(|| EngineError::Other("not an auto layout frame".into()))?;
    let sizing = |s: Sizing| if s == Sizing::Hug { "hug" } else { "fixed" };
    let item = |s: ItemSizing, own: Option<Sizing>| match (s, own) {
        (ItemSizing::Fill, _) => "fill",
        (_, Some(Sizing::Hug)) => "hug",
        _ => "fixed",
    };
    let children: Vec<Value> = n
        .children()
        .map(|ch| {
            ch.iter()
                .filter(|c| !item_of(c).background)
                .map(|c| {
                    let it = item_of(c);
                    let own = layout_of(c);
                    json!({ "id": c.id.0, "width": item(it.width, own.map(|l| l.width)), "height": item(it.height, own.map(|l| l.height)), "absolute": it.absolute })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(json!({
        "id": id.0,
        "direction": if l.direction == Direction::Horizontal { "horizontal" } else { "vertical" },
        "gap": l.gap,
        "gapAuto": l.gap_auto,
        "padding": l.padding,
        "alignMain": l.align_main.name(),
        "alignCross": l.align_cross.name(),
        "width": sizing(l.width),
        "height": sizing(l.height),
        "wrap": l.wrap,
        "crossGap": l.cross_gap,
        "frame": [l.frame.x0, l.frame.y0, l.frame.width(), l.frame.height()],
        "children": children,
    }))
}
