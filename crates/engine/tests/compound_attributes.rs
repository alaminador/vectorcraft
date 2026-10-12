//! Compound appearance and fill-rule edits from a directly selected member (#1197).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use logocraft_doc::{NodeId, NodeKind};
use logocraft_engine::Session;
use logocraft_geom::{Affine, FillRule, PathData};
use logocraft_render::{RenderOptions, Renderer};
use serde_json::{Value, json};

fn run(s: &mut Session, command: &str, params: Value) -> Value {
    s.execute(command, &params).unwrap()
}

fn rectangles() -> (Session, NodeId, NodeId) {
    let mut s = Session::new();
    run(&mut s, "file.new", json!({"width": 100, "height": 100}));
    let outer = NodeId(run(&mut s, "shape.rectangle", json!({"x": 10, "y": 10, "width": 80, "height": 80}))["id"].as_u64().unwrap());
    run(&mut s, "paint.setFill", json!({"color": "#0000ff"}));
    run(&mut s, "paint.setStroke", json!({"none": true}));
    let inner = NodeId(run(&mut s, "shape.rectangle", json!({"x": 30, "y": 30, "width": 40, "height": 40}))["id"].as_u64().unwrap());
    run(&mut s, "paint.setFill", json!({"color": "#ffffff"}));
    (s, outer, inner)
}

fn make(s: &mut Session, outer: NodeId, inner: NodeId) -> NodeId {
    // Selection order must not decide the compound's appearance or stacking order.
    run(s, "select.set", json!({"ids": [inner.0, outer.0]}));
    NodeId(run(s, "object.compoundPath.make", json!({}))["id"].as_u64().unwrap())
}

fn path(s: &Session, id: NodeId) -> PathData {
    s.doc().unwrap().doc.node(id).unwrap().path_data().unwrap().clone()
}

fn pixels(s: &Session) -> ([u8; 4], [u8; 4]) {
    let image = Renderer::new().render(&s.doc().unwrap().doc, 100, 100, Affine::IDENTITY, &RenderOptions::default());
    (image.pixel(20, 20), image.pixel(50, 50))
}

#[test]
fn compound_inherits_the_backmost_appearance_and_preserves_contours() {
    let (mut s, outer, inner) = rectangles();
    let before_outer = path(&s, outer);
    let mut reversed_inner = path(&s, inner);
    reversed_inner.reverse();
    let appearance = s.doc().unwrap().doc.node(outer).unwrap().appearance.clone();
    let compound = make(&mut s, outer, inner);
    let node = s.doc().unwrap().doc.node(compound).unwrap();
    assert_eq!(node.appearance, appearance);
    assert!(matches!(node.kind, NodeKind::Compound { rule: FillRule::NonZero, .. }));
    assert_eq!(path(&s, outer), before_outer);
    assert_eq!(path(&s, inner), reversed_inner);
    let (edge, centre) = pixels(&s);
    assert!(edge[2] > 200 && edge[0] < 20 && edge[3] > 240, "backmost blue fill: {edge:?}");
    assert_eq!(centre[3], 0, "the inner contour cuts a hole");
    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc().unwrap().doc.node(compound).is_none());
    assert_eq!(s.doc().unwrap().doc.node(outer).unwrap().appearance, appearance);
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(s.doc().unwrap().doc.node(compound).unwrap().appearance, appearance);
}
