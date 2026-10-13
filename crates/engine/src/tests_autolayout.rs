//! Auto layout (`layout.*`): frames lay their children out after every edit, Figma's way.

use logocraft_doc::layout::{Direction, ItemSizing, Sizing};
use logocraft_doc::{Node, NodeId};
use logocraft_geom::Rect;
use serde_json::{Value, json};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 600, "height": 400})).unwrap();
    s
}

fn id_of(v: &Value) -> NodeId {
    NodeId(v["id"].as_u64().unwrap())
}

fn rect(s: &mut Session, x: f64, y: f64, w: f64, h: f64) -> NodeId {
    id_of(&s.execute("shape.rectangle", &json!({"x": x, "y": y, "width": w, "height": h})).unwrap())
}

fn node(s: &Session, id: NodeId) -> Node {
    s.doc().unwrap().doc.node(id).cloned().unwrap()
}

fn bounds(s: &Session, id: NodeId) -> Rect {
    node(s, id).geometric_bounds().unwrap()
}

fn frame_of(s: &Session, id: NodeId) -> Rect {
    node(s, id).extras().layout.as_ref().unwrap().frame
}

/// Three rectangles in a loose row, made a frame.
fn three(s: &mut Session) -> (NodeId, [NodeId; 3]) {
    let a = rect(s, 10.0, 10.0, 40.0, 20.0);
    let b = rect(s, 70.0, 12.0, 30.0, 30.0);
    let c = rect(s, 120.0, 8.0, 20.0, 10.0);
    s.execute("select.set", &json!({"ids": [a.0, b.0, c.0]})).unwrap();
    let f = id_of(&s.execute("layout.add", &json!({})).unwrap());
    (f, [a, b, c])
}

#[test]
fn add_auto_layout_reads_the_arrangement_and_lays_it_out() {
    let mut s = session();
    let (f, [a, b, c]) = three(&mut s);
    let l = node(&s, f).extras().layout.clone().unwrap();
    assert_eq!(l.direction, Direction::Horizontal, "side by side → a row");
    assert_eq!(l.gap, 20.0, "the mean space between them");
    assert_eq!(node(&s, f).name.as_deref(), Some("Frame 1"));
    assert_eq!(s.doc().unwrap().selection.objects, vec![f]);
    // Top aligned in a row, 20 pt apart, from the frame's corner (10, 8).
    assert_eq!(bounds(&s, a), Rect::new(10.0, 8.0, 50.0, 28.0));
    assert_eq!(bounds(&s, b), Rect::new(70.0, 8.0, 100.0, 38.0));
    assert_eq!(bounds(&s, c), Rect::new(120.0, 8.0, 140.0, 18.0));
    assert_eq!(frame_of(&s, f), Rect::new(10.0, 8.0, 140.0, 38.0), "hugs its children");
    // The background (first child) follows the frame and paints nothing.
    let n = node(&s, f);
    let bg = n.children().unwrap()[0].clone();
    assert!(bg.extras().layout_item.unwrap().background);
    assert_eq!(bg.geometric_bounds().unwrap(), frame_of(&s, f));
    assert!(bg.appearance.items.is_empty());
    let r = s.execute("layout.get", &json!({})).unwrap();
    assert_eq!(r["children"].as_array().unwrap().len(), 3);
    // A column of objects reads as a column.
    let x = rect(&mut s, 300.0, 10.0, 20.0, 20.0);
    let y = rect(&mut s, 300.0, 60.0, 20.0, 20.0);
    s.execute("select.set", &json!({"ids": [x.0, y.0]})).unwrap();
    let col = id_of(&s.execute("layout.add", &json!({})).unwrap());
    assert_eq!(node(&s, col).extras().layout.as_ref().unwrap().direction, Direction::Vertical);
}

#[test]
fn settings_gap_padding_alignment_and_fixed_size() {
    let mut s = session();
    let (f, [a, b, _]) = three(&mut s);
    s.execute("layout.set", &json!({"gap": 8, "padding": 12})).unwrap();
    assert_eq!(bounds(&s, a).origin(), logocraft_geom::Point::new(22.0, 20.0));
    assert_eq!(bounds(&s, b).x0, 22.0 + 40.0 + 8.0);
    assert_eq!(frame_of(&s, f).width(), 12.0 + 40.0 + 8.0 + 30.0 + 8.0 + 20.0 + 12.0);
    // Fixed width, centred: the children sit in the middle.
    s.execute("layout.set", &json!({"width": 300, "align": "center"})).unwrap();
    let fr = frame_of(&s, f);
    assert_eq!(fr.width(), 300.0);
    let content = 40.0 + 8.0 + 30.0 + 8.0 + 20.0;
    assert!((bounds(&s, a).x0 - (fr.x0 + 12.0 + (300.0 - 24.0 - content) / 2.0)).abs() < 1e-9);
    // Vertical: a column.
    s.execute("layout.set", &json!({"direction": "vertical", "width": "hug", "align": "top-left"})).unwrap();
    assert_eq!(bounds(&s, b).y0, bounds(&s, a).y1 + 8.0);
    // Space between.
    s.execute("layout.set", &json!({"direction": "horizontal", "width": 300, "gap": "auto", "padding": 0})).unwrap();
    let fr = frame_of(&s, f);
    assert!((bounds(&s, a).x0 - fr.x0).abs() < 1e-9);
    let n = node(&s, f);
    let last = n.children().unwrap().last().unwrap().geometric_bounds().unwrap();
    assert!((last.x1 - fr.x1).abs() < 1e-9, "the last child at the right edge");
    // Fill and radius go to the background.
    s.execute("layout.set", &json!({"fill": "#ff0000", "radius": 8})).unwrap();
    let bg = node(&s, f).children().unwrap()[0].clone();
    assert!(matches!(bg.appearance.fill_paint(), logocraft_color::Paint::Solid { .. }));
    assert!(s.execute("layout.set", &json!({"direction": "diagonal"})).is_err());
}

#[test]
fn children_fill_the_frame_and_a_row_wraps() {
    let mut s = session();
    let (f, [a, b, c]) = three(&mut s);
    s.execute("layout.set", &json!({"width": 200, "height": 60, "gap": 10, "padding": 0})).unwrap();
    s.execute("layout.setItem", &json!({"ids": [b.0], "width": "fill", "height": "fill"})).unwrap();
    let fr = frame_of(&s, f);
    // 200 - 40 - 20 - 2 × 10 = 120 for the filling child, the full height across.
    assert!((bounds(&s, b).width() - 120.0).abs() < 1e-6, "{:?}", bounds(&s, b));
    assert!((bounds(&s, b).height() - 60.0).abs() < 1e-6);
    assert!((bounds(&s, c).x1 - fr.x1).abs() < 1e-6);
    assert_eq!(node(&s, b).extras().layout_item.unwrap().width, ItemSizing::Fill);
    // Wrap: back to fixed sizes, in a narrow frame.
    s.execute("layout.setItem", &json!({"ids": [b.0], "width": "fixed", "height": "fixed"})).unwrap();
    s.execute("layout.set", &json!({"width": 80, "height": "hug", "wrap": true, "crossGap": 5})).unwrap();
    assert!(bounds(&s, b).y0 > bounds(&s, a).y1, "the second child wraps under the first");
    assert!(frame_of(&s, f).height() > 60.0);
}

#[test]
fn absolute_children_stay_put_and_hidden_ones_leave_the_flow() {
    let mut s = session();
    let (f, [a, b, c]) = three(&mut s);
    s.execute("layout.setItem", &json!({"ids": [a.0], "absolute": true})).unwrap();
    // b and c close the gap a left.
    let fr = frame_of(&s, f);
    assert_eq!(bounds(&s, b).x0, fr.x0);
    assert_eq!(bounds(&s, c).x0, bounds(&s, b).x1 + 20.0);
    // Moving the absolute child: it stays where it is put.
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    let before = bounds(&s, a);
    s.execute("object.move", &json!({"dx": 5, "dy": 50})).unwrap();
    assert_eq!(bounds(&s, a).y0, before.y0 + 50.0);
}

#[test]
fn a_dragged_child_takes_its_new_place_or_leaves_the_frame() {
    let mut s = session();
    let (f, [a, b, c]) = three(&mut s);
    // Drag a past c: the order becomes b, c, a.
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    s.execute("object.move", &json!({"dx": 100, "dy": 0})).unwrap();
    let order: Vec<NodeId> = node(&s, f).children().unwrap().iter().skip(1).map(|c| c.id).collect();
    assert_eq!(order, vec![b, c, a]);
    let fr = frame_of(&s, f);
    assert_eq!(bounds(&s, b).x0, fr.x0, "laid out again in the new order");
    // Drag c far away: it leaves the frame, into the layer above it.
    s.execute("select.set", &json!({"ids": [c.0]})).unwrap();
    s.execute("object.move", &json!({"dx": 0, "dy": 200})).unwrap();
    let d = &s.doc().unwrap().doc;
    assert_ne!(d.parent_of(c), Some(f));
    assert_eq!(d.parent_of(c), d.parent_of(f));
    assert!(bounds(&s, c).y0 > 150.0, "left where it was dropped");
    assert!(node(&s, c).extras().layout_item.is_none());
}

#[test]
fn resizing_a_frame_keeps_its_children_and_fixes_its_size() {
    let mut s = session();
    let (f, [a, ..]) = three(&mut s);
    let size = bounds(&s, a);
    s.execute("select.set", &json!({"ids": [f.0]})).unwrap();
    s.execute("object.setBounds", &json!({"width": 300})).unwrap();
    let l = node(&s, f).extras().layout.clone().unwrap();
    assert_eq!(l.width, Sizing::Fixed, "resized across: fixed width");
    assert_eq!(l.height, Sizing::Hug, "its height still hugs");
    assert!((l.frame.width() - 300.0).abs() < 1e-6, "{:?}", l.frame);
    let now = bounds(&s, a);
    assert!((now.width() - size.width()).abs() < 1e-6 && (now.height() - size.height()).abs() < 1e-6, "children keep their size");
}

#[test]
fn edits_inside_reflow_and_undo_restores() {
    let mut s = session();
    let (f, [a, b, _]) = three(&mut s);
    let before = bounds(&s, b);
    // Widen the first child: the others move along.
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    s.execute("object.setBounds", &json!({"width": 80})).unwrap();
    assert!((bounds(&s, b).x0 - (before.x0 + 40.0)).abs() < 1e-6, "{:?}", bounds(&s, b));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(bounds(&s, b), before);
    // Nested: a frame inside a frame hugs its own children and flows in its parent.
    s.execute("select.set", &json!({"ids": [f.0]})).unwrap();
    let r = rect(&mut s, 10.0, 200.0, 50.0, 50.0);
    s.execute("select.set", &json!({"ids": [f.0, r.0]})).unwrap();
    let outer = id_of(&s.execute("layout.add", &json!({"direction": "vertical", "gap": 4})).unwrap());
    assert_eq!(s.doc().unwrap().doc.parent_of(f), Some(outer));
    assert!((bounds(&s, r).y0 - (frame_of(&s, f).y1 + 4.0)).abs() < 1e-6);
    // Fill across: the inner frame takes the outer's width.
    s.execute("layout.set", &json!({"ids": [outer.0], "width": 400})).unwrap();
    s.execute("layout.setItem", &json!({"ids": [f.0], "width": "fill"})).unwrap();
    assert!((frame_of(&s, f).width() - 400.0).abs() < 1e-6, "{:?}", frame_of(&s, f));
}

#[test]
fn remove_and_save() {
    let mut s = session();
    let (f, [a, ..]) = three(&mut s);
    s.execute("layout.set", &json!({"gap": 6, "padding": [1, 2, 3, 4]})).unwrap();
    let bytes = logocraft_format::save(&s.doc().unwrap().doc, false);
    let doc = logocraft_format::load(&bytes).unwrap();
    assert_eq!(doc.node(f).unwrap().extras().layout, node(&s, f).extras().layout, "saved and opened");
    let at = bounds(&s, a);
    s.execute("select.set", &json!({"ids": [f.0]})).unwrap();
    s.execute("layout.remove", &json!({})).unwrap();
    let n = node(&s, f);
    assert!(n.extras().layout.is_none());
    assert_eq!(n.children().unwrap().len(), 3, "the unpainted background went");
    assert_eq!(bounds(&s, a), at, "children stay where they were");
    assert!(s.execute("layout.set", &json!({"gap": 1})).is_err(), "no longer a frame");
}

#[test]
fn shortcuts() {
    assert_eq!(find_command("layout.add").unwrap().shortcut, Some("Shift+A"));
    assert_eq!(find_command("layout.remove").unwrap().shortcut, Some("Alt+Shift+A"));
}
