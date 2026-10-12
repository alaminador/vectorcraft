//! Graphic styles: transparency, capture from groups and type, add-apply, unique names, links
//! (redefine, break link, delete, rename), Select All Unused, Sort by Name and Select > Same.

use serde_json::{Value, json};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 600, "height": 400})).unwrap();
    s
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

/// A selected 50×50 rectangle at `x` with the default appearance.
fn rect(s: &mut Session, x: f64) -> NodeId {
    let id = NodeId(run(s, "shape.rectangle", json!({"x": x, "y": 10, "width": 50, "height": 50}))["id"].as_u64().unwrap());
    run(s, "select.set", json!({"ids": [id.0]}));
    id
}

fn set_fill(s: &mut Session, id: NodeId, hex: &str) {
    run(s, "paint.setFill", json!({"color": hex, "ids": [id.0]}));
}

#[test]
fn select_same_appearance_attribute() {
    let mut s = session();
    let a = rect(&mut s, 10.0);
    run(&mut s, "effect.apply", json!({"effect": "distort.roughen"}));
    let b = rect(&mut s, 100.0);
    set_fill(&mut s, b, "#ff0000");
    run(&mut s, "effect.apply", json!({"effect": "distort.roughen"}));
    let c = rect(&mut s, 200.0);
    // The shared effect.
    run(&mut s, "select.set", json!({"ids": [a.0]}));
    run(&mut s, "select.same.appearanceAttribute", json!({}));
    assert_eq!(s.doc().unwrap().selection.objects, vec![a, b]);
    // The shared stroke (item 1).
    run(&mut s, "select.set", json!({"ids": [a.0]}));
    run(&mut s, "select.same.appearanceAttribute", json!({"item": 1}));
    assert_eq!(s.doc().unwrap().selection.objects, vec![a, b, c]);
}
