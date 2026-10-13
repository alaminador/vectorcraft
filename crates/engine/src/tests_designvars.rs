//! Design variables (`variable.*`): bound layers follow their variables in the modes their
//! containers pick; instances resolve in their own modes.

use logocraft_doc::{Node, NodeId, NodeKind};
use serde_json::{Value, json};

use super::*;

fn id_of(v: &Value) -> NodeId {
    NodeId(v["id"].as_u64().unwrap())
}

fn node(s: &Session, id: NodeId) -> Node {
    s.doc().unwrap().doc.node(id).cloned().unwrap()
}

fn fill(s: &Session, id: NodeId) -> String {
    node(s, id).appearance.fill_paint().color().map(|c| c.to_hex()).unwrap_or_default()
}

/// A Theme collection (Light, Dark) with a colour `bg` and a number `radius`.
fn theme(s: &mut Session) {
    s.execute("file.new", &json!({"width": 600, "height": 400})).unwrap();
    s.execute("variable.createCollection", &json!({"name": "Theme", "modes": ["Light", "Dark"]})).unwrap();
    s.execute("variable.create", &json!({"collection": "Theme", "name": "bg", "type": "color", "values": {"Light": "#ffffff", "Dark": "#111111"}}))
        .unwrap();
    s.execute("variable.create", &json!({"collection": "Theme", "name": "radius", "type": "number", "value": 8})).unwrap();
}

#[test]
fn bound_layers_follow_values_and_modes() {
    let mut s = Session::new();
    theme(&mut s);
    let r = id_of(&s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 100, "height": 50})).unwrap());
    s.execute("variable.bind", &json!({"field": "fill", "variable": "bg"})).unwrap();
    s.execute("variable.bind", &json!({"field": "cornerRadius", "variable": "radius"})).unwrap();
    assert_eq!(fill(&s, r), "#ffffff", "the default mode");
    let radius = |s: &Session| match node(s, r).live_shape() {
        Some(logocraft_doc::LiveShape::Rectangle { radii, .. }) => radii[0],
        _ => -1.0,
    };
    assert_eq!(radius(&s), 8.0);
    // Edit the variable: the layer follows.
    s.execute("variable.set", &json!({"variable": "bg", "mode": "Light", "value": "#ff0000"})).unwrap();
    assert_eq!(fill(&s, r), "#ff0000");
    // Group it and switch the group to Dark: the layer inside follows its container's mode.
    let r2 = id_of(&s.execute("shape.rectangle", &json!({"x": 200, "y": 10, "width": 50, "height": 50})).unwrap());
    s.execute("select.set", &json!({"ids": [r.0, r2.0]})).unwrap();
    let g = id_of(&s.execute("layout.add", &json!({})).unwrap());
    s.execute("variable.setMode", &json!({"ids": [g.0], "collection": "Theme", "mode": "Dark"})).unwrap();
    assert_eq!(fill(&s, r), "#111111", "dark inside the dark frame");
    s.execute("variable.setMode", &json!({"ids": [g.0], "collection": "Theme", "mode": null})).unwrap();
    assert_eq!(fill(&s, r), "#ff0000", "back to Auto");
    // Wrong type refused.
    s.execute("select.set", &json!({"ids": [r.0]})).unwrap();
    assert!(s.execute("variable.bind", &json!({"field": "fill", "variable": "radius"})).is_err());
    // Unbinding keeps the value; deleting a bound variable keeps it too.
    s.execute("variable.bind", &json!({"field": "cornerRadius", "variable": null})).unwrap();
    s.execute("variable.set", &json!({"variable": "radius", "value": 20})).unwrap();
    assert_eq!(radius(&s), 8.0, "no longer bound");
    s.execute("variable.delete", &json!({"variable": "bg"})).unwrap();
    assert_eq!(fill(&s, r), "#ff0000");
    assert!(node(&s, r).extras().bound.is_none());
}

#[test]
fn aliases_text_and_visibility() {
    let mut s = Session::new();
    theme(&mut s);
    s.execute("variable.create", &json!({"collection": "Theme", "name": "surface", "type": "color"})).unwrap();
    s.execute("variable.set", &json!({"variable": "surface", "mode": "Light", "alias": "bg"})).unwrap();
    s.execute("variable.set", &json!({"variable": "surface", "mode": "Dark", "alias": "bg"})).unwrap();
    assert!(s.execute("variable.set", &json!({"variable": "bg", "alias": "surface"})).is_err(), "no loops");
    assert!(s.execute("variable.set", &json!({"variable": "surface", "alias": "radius"})).is_err(), "same type only");
    let r = id_of(&s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 100, "height": 50})).unwrap());
    s.execute("variable.bind", &json!({"field": "fill", "variable": "surface"})).unwrap();
    assert_eq!(fill(&s, r), "#ffffff");
    s.execute("variable.create", &json!({"collection": "Copy", "name": "cta", "type": "string", "value": "Buy now"})).unwrap();
    s.execute("variable.create", &json!({"collection": "Copy", "name": "showBadge", "type": "boolean", "value": false})).unwrap();
    let t = id_of(&s.execute("text.create", &json!({"x": 10, "y": 100, "text": "x"})).unwrap());
    s.execute("variable.bind", &json!({"field": "characters", "variable": "cta"})).unwrap();
    s.execute("variable.bind", &json!({"field": "visible", "variable": "showBadge"})).unwrap();
    let NodeKind::Text(text) = node(&s, t).kind else { panic!() };
    assert_eq!(text.runs.iter().map(|r| r.text.as_str()).collect::<String>(), "Buy now");
    assert!(!node(&s, t).visible);
    let list = s.execute("variable.list", &json!({})).unwrap();
    assert_eq!(list["collections"].as_array().unwrap().len(), 2, "Copy was made");
}

#[test]
fn instances_resolve_in_their_own_modes() {
    let mut s = Session::new();
    theme(&mut s);
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 100, "height": 50})).unwrap();
    s.execute("variable.bind", &json!({"field": "fill", "variable": "bg"})).unwrap();
    s.execute("component.create", &json!({"name": "Card"})).unwrap();
    let inst = id_of(&s.execute("symbol.place", &json!({"name": "Card", "x": 300, "y": 200})).unwrap());
    let shown_fill = |s: &Session| {
        let d = &s.doc().unwrap().doc;
        let NodeKind::SymbolInstance { symbol, .. } = &d.node(inst).unwrap().kind else { panic!() };
        let art = d.symbols.iter().find(|x| x.name == *symbol).unwrap().art.clone();
        let mut out = String::new();
        art.walk(&mut |l: &Node| {
            if matches!(l.kind, NodeKind::Path { .. }) {
                out = l.appearance.fill_paint().color().map(|c| c.to_hex()).unwrap_or_default();
            }
        });
        out
    };
    assert_eq!(shown_fill(&s), "#ffffff");
    s.execute("variable.setMode", &json!({"ids": [inst.0], "collection": "Theme", "mode": "Dark"})).unwrap();
    assert_eq!(shown_fill(&s), "#111111", "the instance in Dark");
    // The variable changes: the dark instance follows.
    s.execute("variable.set", &json!({"variable": "bg", "mode": "Dark", "value": "#222222"})).unwrap();
    assert_eq!(shown_fill(&s), "#222222");
    // Saved and opened with its variables and bindings.
    let bytes = logocraft_format::save(&s.doc().unwrap().doc, false);
    let doc = logocraft_format::load(&bytes).unwrap();
    assert_eq!(doc.design_variables.variables.len(), 2);
    assert!(doc.node(inst).unwrap().extras().var_modes.is_some());
}
