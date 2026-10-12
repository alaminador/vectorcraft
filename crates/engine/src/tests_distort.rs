//! Width points: commands and the Width tool.

use logocraft_tools::{Mods, PointerEvent, PointerKind, ToolKey};
use serde_json::json;

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
    s
}

fn rect(s: &mut Session, x: f64, y: f64, w: f64, h: f64) -> NodeId {
    let r = s.execute("shape.rectangle", &json!({"x": x, "y": y, "width": w, "height": h})).unwrap();
    NodeId(r["id"].as_u64().unwrap())
}

fn line(s: &mut Session) -> NodeId {
    let r = s.execute("shape.line", &json!({"x1": 100, "y1": 300, "x2": 500, "y2": 300})).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    s.execute("stroke.set", &json!({"ids": [id.0], "weight": 10})).unwrap();
    id
}

fn profile(s: &Session, id: NodeId) -> Option<Vec<(f64, f64, f64)>> {
    s.doc().unwrap().doc.node(id).unwrap().appearance.stroke().unwrap().profile.as_ref().map(|p| p.points.clone())
}

fn undo_len(s: &Session) -> usize {
    s.doc().unwrap().history.undo.len()
}

/// How many steps there are to undo and to redo, and whether the document has unsaved changes.
fn history(s: &Session) -> (usize, usize, bool) {
    let st = s.doc().unwrap();
    (st.history.undo.len(), st.history.redo.len(), st.is_dirty())
}

/// Leave a step to redo (a rectangle drawn and undone, which keeps the selection) and mark the
/// document saved.
fn with_a_step_to_redo(s: &mut Session) {
    rect(s, 700.0, 500.0, 10.0, 10.0);
    s.execute("edit.undo", &json!({})).unwrap();
    s.doc_mut().unwrap().mark_saved();
}

fn gesture(s: &mut Session, tool: &str, pts: &[(f64, f64)], mods: Mods) {
    let v = ViewInfo::default();
    s.select_tool(tool, v).unwrap();
    for (i, (x, y)) in pts.iter().enumerate() {
        let kind = if i == 0 {
            PointerKind::Down
        } else if i == pts.len() - 1 {
            PointerKind::Up
        } else {
            PointerKind::Drag
        };
        s.pointer(&PointerEvent::new(kind, *x, *y).with_mods(mods), v).unwrap();
    }
}

// ---------- width ----------

#[test]
fn width_point_set_creates_profile_and_undoes() {
    let mut s = session();
    let id = line(&mut s);
    let n = undo_len(&s);
    let r = s.execute("stroke.widthPoint.set", &json!({"id": id.0, "t": 0.5, "left": 10, "right": 5})).unwrap();
    assert_eq!(r["index"], json!(1));
    assert_eq!(profile(&s, id).unwrap(), vec![(0.0, 1.0, 1.0), (0.5, 2.0, 1.0), (1.0, 1.0, 1.0)]);
    assert_eq!(undo_len(&s), n + 1);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(profile(&s, id), None);
}

#[test]
fn width_point_move_and_remove() {
    let mut s = session();
    let id = line(&mut s);
    s.execute("stroke.widthPoint.set", &json!({"id": id.0, "t": 0.5, "left": 10, "right": 10})).unwrap();
    let r = s.execute("stroke.widthPoint.set", &json!({"id": id.0, "t": 0.25, "left": 10, "right": 10, "index": 1})).unwrap();
    assert_eq!(r["index"], json!(1));
    assert_eq!(profile(&s, id).unwrap()[1], (0.25, 2.0, 2.0));
    assert_eq!(profile(&s, id).unwrap().len(), 3);
    s.execute("stroke.widthPoint.remove", &json!({"id": id.0, "index": 1})).unwrap();
    assert_eq!(profile(&s, id).unwrap().len(), 2);
    assert!(s.execute("stroke.widthPoint.remove", &json!({"id": id.0, "index": 7})).is_err());
    assert!(s.execute("stroke.widthPoint.set", &json!({"id": id.0, "t": 0.5, "left": -1, "right": 1})).is_err());
}

#[test]
fn width_profile_set_and_clear() {
    let mut s = session();
    let id = line(&mut s);
    s.execute("stroke.widthProfile.set", &json!({"ids": [id.0], "points": [[1, 0, 0], [0, 0.5, 0.5]]})).unwrap();
    assert_eq!(profile(&s, id).unwrap(), vec![(0.0, 0.5, 0.5), (1.0, 0.0, 0.0)]);
    s.execute("stroke.widthProfile.set", &json!({"ids": [id.0], "points": null})).unwrap();
    assert_eq!(profile(&s, id), None);
}

#[test]
fn width_tool_gesture_is_one_undo_step() {
    let mut s = session();
    let id = line(&mut s);
    let n = undo_len(&s);
    gesture(&mut s, "width", &[(300.0, 301.0), (300.0, 310.0), (300.0, 320.0)], Mods::default());
    let p = profile(&s, id).unwrap();
    assert_eq!(p.len(), 3);
    // The last drag sample (10 pt from the path) sets both sides: factor 10 / (10 / 2) = 2.
    assert!((p[1].0 - 0.5).abs() < 1e-3 && (p[1].1 - 2.0).abs() < 1e-6 && (p[1].2 - 2.0).abs() < 1e-6, "{p:?}");
    assert_eq!(undo_len(&s), n + 1);
    // Delete removes the selected width point.
    s.tool_key(ToolKey::Delete, Mods::default(), ViewInfo::default()).unwrap();
    assert_eq!(profile(&s, id).unwrap().len(), 2);
}

/// Width Point with the place and widths a point has already (given its index, or at its place, with
/// Adjust Adjoining Width Points, or as Width Point Edit gives back the widths it shows), Copy Width
/// Point onto the side of a discontinuous point that has its widths already, and Width Profile with
/// the profile the strokes have already (or null for a uniform stroke and for art with no stroke)
/// record no undo step: the step to redo stays and the document stays unmodified.
#[test]
fn width_commands_that_change_nothing_record_no_undo_step() {
    let mut s = session();
    let id = line(&mut s);
    let uniform = line(&mut s);
    let r = rect(&mut s, 100.0, 400.0, 20.0, 20.0);
    s.execute("select.set", &json!({"ids": [r.0]})).unwrap();
    let group = NodeId(s.execute("object.group", &json!({})).unwrap()["id"].as_u64().unwrap());
    // Points 1 and 2 make a discontinuous point at 0.5 whose sides are both 2 pt and 3 pt wide
    // (point 1 copied onto its own place), and point 3 is at 0.75.
    s.execute("stroke.widthPoint.set", &json!({"id": id.0, "t": 0.5, "left": 2, "right": 3})).unwrap();
    s.execute("stroke.widthPoint.copy", &json!({"id": id.0, "index": 1, "t": 0.5})).unwrap();
    s.execute("stroke.widthPoint.set", &json!({"id": id.0, "t": 0.75, "left": 4, "right": 4})).unwrap();
    let points = profile(&s, id).unwrap();
    assert_eq!(points, vec![(0.0, 1.0, 1.0), (0.5, 0.4, 0.6), (0.5, 0.4, 0.6), (0.75, 0.8, 0.8), (1.0, 1.0, 1.0)]);
    // Width Point Edit shows a width as its factor times half the weight (5 pt) and gives that back.
    // For this factor the round trip changes the last bit.
    let dragged = line(&mut s);
    let f = 1.8822996672167678;
    assert_ne!(f * 5.0 / 5.0, f);
    s.execute("stroke.widthProfile.set", &json!({"ids": [dragged.0], "points": [[0, 1, 1], [0.5, f, f], [1, 1, 1]]})).unwrap();
    with_a_step_to_redo(&mut s);
    let before = history(&s);
    assert_eq!((before.1, before.2), (1, false), "a step to redo, and nothing unsaved");
    let calls = [
        ("stroke.widthPoint.set", json!({"id": id.0, "index": 1, "t": 0.5, "left": 2, "right": 3}), json!({"index": 1})),
        ("stroke.widthPoint.set", json!({"id": id.0, "t": 0.75, "left": 4, "right": 4}), json!({"index": 3})),
        // Adjust Adjoining multiplies point 2's 0.4 by 0.8 and divides the product by 0.8, which
        // gives 0.4000000000000001.
        ("stroke.widthPoint.set", json!({"id": id.0, "index": 3, "t": 0.75, "left": 4, "right": 4, "adjustAdjoining": true}), json!({"index": 3})),
        ("stroke.widthPoint.set", json!({"id": dragged.0, "index": 1, "t": 0.5, "left": f * 5.0, "right": f * 5.0}), json!({"index": 1})),
        ("stroke.widthPoint.copy", json!({"id": id.0, "index": 2, "t": 0.5}), json!({"index": 2})),
        ("stroke.widthProfile.set", json!({"ids": [id.0], "points": points}), json!(null)),
        ("stroke.widthProfile.set", json!({"ids": [uniform.0, group.0], "points": null}), json!(null)),
    ];
    for (c, p, reply) in calls {
        assert_eq!(s.execute(c, &p).unwrap(), reply, "{c} {p}");
        assert_eq!(history(&s), before, "{c} {p}");
        assert_eq!(profile(&s, id).unwrap(), points, "{c} {p}");
        assert_eq!(s.journal.last().unwrap().0, c);
    }
    assert_eq!(profile(&s, dragged).unwrap()[1], (0.5, f, f));
    // Other widths are one undo step, and they clear Redo.
    s.execute("stroke.widthPoint.set", &json!({"id": id.0, "index": 1, "t": 0.5, "left": 3, "right": 3})).unwrap();
    assert_eq!(history(&s), (before.0 + 1, 0, true));
}

// ---------- liquify ----------
