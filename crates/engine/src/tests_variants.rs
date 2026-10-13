//! Variants (`component.combineVariants` and friends): component sets whose instances switch
//! between variants by their properties.

use logocraft_doc::{NodeId, NodeKind};
use serde_json::{Value, json};

use super::*;

fn id_of(v: &Value) -> NodeId {
    NodeId(v["id"].as_u64().unwrap())
}

/// A button component set: Default and Hover, the second wider.
fn button_set(s: &mut Session) -> (NodeId, NodeId, NodeId) {
    s.execute("file.new", &json!({"width": 600, "height": 400})).unwrap();
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 80, "height": 30})).unwrap();
    let a = id_of(&s.execute("component.create", &json!({"name": "Button"})).unwrap());
    s.execute("shape.rectangle", &json!({"x": 120, "y": 10, "width": 100, "height": 30})).unwrap();
    s.execute("paint.setFill", &json!({"color": "#ff0000"})).unwrap();
    let b = id_of(&s.execute("component.create", &json!({"name": "Button 2"})).unwrap());
    s.execute("select.set", &json!({"ids": [a.0, b.0]})).unwrap();
    let set = id_of(&s.execute("component.combineVariants", &json!({})).unwrap());
    (set, a, b)
}

#[test]
fn combine_as_variants_makes_a_set_with_properties() {
    let mut s = Session::new();
    let (set, a, b) = button_set(&mut s);
    let d = &s.doc().unwrap().doc;
    let n = d.node(set).unwrap();
    assert_eq!(n.extras().component_set.as_deref(), Some("Button"));
    assert!(n.extras().layout.is_some(), "a set is an auto layout frame");
    assert_eq!(d.node(a).unwrap().extras().variant, Some(vec![("Property 1".into(), "Default".into())]));
    assert_eq!(d.node(b).unwrap().extras().variant, Some(vec![("Property 1".into(), "Variant2".into())]));
    let v = s.execute("component.variants", &json!({"id": set.0})).unwrap();
    assert_eq!(v["properties"][0]["values"], json!(["Default", "Variant2"]));
    // Rename the property and set values; a new property reaches every variant.
    s.execute("component.renameProperty", &json!({"id": set.0, "from": "Property 1", "to": "State"})).unwrap();
    s.execute("component.setVariant", &json!({"ids": [b.0], "props": {"State": "Hover", "Size": "Large"}})).unwrap();
    let d = &s.doc().unwrap().doc;
    assert_eq!(d.node(b).unwrap().extras().variant, Some(vec![("State".into(), "Hover".into()), ("Size".into(), "Large".into())]));
    assert_eq!(d.node(a).unwrap().extras().variant, Some(vec![("State".into(), "Default".into()), ("Size".into(), "Large".into())]));
    // Not main components: refused.
    let r = id_of(&s.execute("shape.rectangle", &json!({"x": 300, "y": 300, "width": 10, "height": 10})).unwrap());
    assert!(s.execute("component.combineVariants", &json!({"ids": [r.0]})).is_err());
}

#[test]
fn instances_switch_variants() {
    let mut s = Session::new();
    let (_, a, b) = button_set(&mut s);
    s.execute("component.renameProperty", &json!({"id": a.0, "from": "Property 1", "to": "State"})).unwrap();
    s.execute("component.setVariant", &json!({"ids": [b.0], "props": {"State": "Hover"}})).unwrap();
    let sym_a = s.doc().unwrap().doc.node(a).unwrap().extras().component.clone().unwrap();
    let inst = id_of(&s.execute("symbol.place", &json!({"name": sym_a, "x": 300, "y": 300})).unwrap());
    let r = s.execute("component.setInstanceVariant", &json!({"props": {"State": "Hover"}})).unwrap();
    let sym_b = s.doc().unwrap().doc.node(b).unwrap().extras().component.clone().unwrap();
    assert_eq!(r["symbols"][0], json!(sym_b));
    let n = s.doc().unwrap().doc.node(inst).cloned().unwrap();
    assert!(matches!(&n.kind, NodeKind::SymbolInstance { symbol, .. } if *symbol == sym_b));
    let bx = n.geometric_bounds().unwrap();
    assert!((bx.width() - 100.0).abs() < 1e-6, "takes the variant's size: {bx:?}");
    assert!((bx.center().x - 300.0).abs() < 1e-6, "about its centre");
    assert!(s.execute("component.setInstanceVariant", &json!({"props": {"State": "Pressed"}})).is_err(), "no such variant");
    let v = s.execute("component.variants", &json!({"id": inst.0})).unwrap();
    assert_eq!(v["name"], "Button", "an instance knows its set");
}

#[test]
fn add_variant_copies_one_with_a_new_value() {
    let mut s = Session::new();
    let (set, _, b) = button_set(&mut s);
    let r = s.execute("component.addVariant", &json!({"id": b.0})).unwrap();
    let c = id_of(&r);
    let d = &s.doc().unwrap().doc;
    assert_eq!(d.parent_of(c), Some(set));
    assert_eq!(d.node(c).unwrap().extras().variant, Some(vec![("Property 1".into(), "Variant3".into())]));
    assert!(d.symbols.iter().any(|x| Some(&x.name) == d.node(c).unwrap().extras().component.as_ref()));
    // Still a main component after the post-edit sync, not an instance.
    assert!(!matches!(d.node(c).unwrap().kind, NodeKind::SymbolInstance { .. }));
    // A set selected adds after its last variant; saved and opened with its properties.
    s.execute("component.addVariant", &json!({"id": set.0})).unwrap();
    let doc = logocraft_format::load(&logocraft_format::save(&s.doc().unwrap().doc, false)).unwrap();
    assert_eq!(doc.node(set).unwrap().extras().component_set.as_deref(), Some("Button"));
    assert_eq!(cmd::components::variants_of(&doc, set).len(), 4);
}
