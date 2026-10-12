//! Window → Attributes (`attributes.set`, `attributes.info`, `path.setFillRule`, `path.reverse`)
//! and the links SVG export writes for objects with a URL.

use logocraft_doc::NodeKind;
use logocraft_geom::FillRule;
use serde_json::{Value, json};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 100, "height": 100})).unwrap();
    s
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap()
}

fn rect(s: &mut Session, x: f64) -> NodeId {
    NodeId(run(s, "shape.rectangle", json!({"x": x, "y": 10, "width": 30, "height": 30}))["id"].as_u64().unwrap())
}

#[test]
fn fill_rule_reaches_compound_paths_and_the_paths_in_groups() {
    let mut s = session();
    let a = rect(&mut s, 0.0);
    let b = rect(&mut s, 50.0);
    run(&mut s, "select.set", json!({"ids": [a.0, b.0]}));
    run(&mut s, "object.compoundPath.make", json!({}));
    let compound = s.doc().unwrap().selection.objects[0];
    let d = rect(&mut s, 0.0);
    run(&mut s, "select.set", json!({"ids": [compound.0, d.0]}));
    run(&mut s, "object.group", json!({}));
    assert_eq!(run(&mut s, "path.setFillRule", json!({"rule": "evenOdd"}))["changed"], 2, "the compound path and the rectangle");
    let doc = &s.doc().unwrap().doc;
    assert!(matches!(doc.node(compound).unwrap().kind, NodeKind::Compound { rule: FillRule::EvenOdd, .. }));
    assert!(matches!(doc.node(d).unwrap().kind, NodeKind::Path { rule: FillRule::EvenOdd, .. }));
}

/// #522: an object's own data (SVG `data-*` attributes) is set with `object.setProps {data}`,
/// undone in one step, written to SVG and kept by the native format.
#[test]
fn object_data_is_set_saved_and_written_to_svg() {
    let mut s = session();
    let a = rect(&mut s, 10.0);
    let data = |s: &Session| s.doc().unwrap().doc.node(a).unwrap().extras().attrs.as_deref().map(|x| x.data.clone()).unwrap_or_default();
    let pair = |k: &str, v: &str| (k.to_string(), v.to_string());
    run(&mut s, "object.setProps", json!({"data": {"pivot": "60,100", "z": 3}}));
    assert_eq!(data(&s), [pair("pivot", "60,100"), pair("z", "3")], "values are text");
    run(&mut s, "object.setProps", json!({"data": {"data-pivot": "1,2", "z": null}}));
    assert_eq!(data(&s), [pair("pivot", "1,2")], "a data- prefix is optional; null removes a key");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(data(&s).len(), 2, "one step");
    for bad in [json!({"data": {"vc-x": "1"}}), json!({"data": {"name": "x"}}), json!({"data": {"a b": "1"}}), json!({"data": "pivot"})] {
        assert!(s.execute("object.setProps", &bad).is_err(), "{bad}");
    }
    let svg = run(&mut s, "document.serialize", json!({"format": "svg"}))["text"].as_str().unwrap().to_string();
    assert!(svg.contains("data-pivot=\"60,100\" data-z=\"3\""), "{svg}");
    let native = logocraft_format::save(&s.doc().unwrap().doc, false);
    let back = logocraft_format::load(&native).unwrap();
    assert_eq!(back.node(a).unwrap().extras().attrs, s.doc().unwrap().doc.node(a).unwrap().extras().attrs);
}
