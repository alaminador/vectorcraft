//! Component properties and instance overrides (`component.addProperty`, `instance.*`): instances
//! show their own values and overrides, which survive edits of the main.

use logocraft_color::{Color, Paint};
use logocraft_doc::{Node, NodeId, NodeKind};
use serde_json::{Value, json};

use super::*;

fn id_of(v: &Value) -> NodeId {
    NodeId(v["id"].as_u64().unwrap())
}

fn node(s: &Session, id: NodeId) -> Node {
    s.doc().unwrap().doc.node(id).cloned().unwrap()
}

/// The art an instance shows (its symbol's).
fn shown(s: &Session, inst: NodeId) -> Node {
    let d = &s.doc().unwrap().doc;
    let NodeKind::SymbolInstance { symbol, .. } = &d.node(inst).unwrap().kind else { panic!("not an instance") };
    (*d.symbols.iter().find(|x| x.name == *symbol).unwrap().art).clone()
}

fn texts(n: &Node) -> Vec<String> {
    let mut out = vec![];
    n.walk(&mut |l: &Node| {
        if let NodeKind::Text(t) = &l.kind {
            out.push(t.runs.iter().map(|r| r.text.as_str()).collect());
        }
    });
    out
}

fn rect_fills(n: &Node) -> Vec<String> {
    let mut out = vec![];
    n.walk(&mut |l: &Node| {
        if matches!(l.kind, NodeKind::Path { .. }) {
            out.push(l.appearance.fill_paint().color().map(|c| c.to_hex()).unwrap_or_default());
        }
    });
    out
}

/// A button component (a rectangle, a label, an icon dot) and one instance of it.
fn button(s: &mut Session) -> (NodeId, NodeId, NodeId, NodeId, NodeId) {
    s.execute("file.new", &json!({"width": 600, "height": 400})).unwrap();
    let bg = id_of(&s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 120, "height": 40})).unwrap());
    s.execute("paint.setFill", &json!({"color": "#5b3df5"})).unwrap();
    let label = id_of(&s.execute("text.create", &json!({"x": 30, "y": 36, "text": "Buy now"})).unwrap());
    let dot = id_of(&s.execute("shape.ellipse", &json!({"x": 110, "y": 25, "width": 10, "height": 10})).unwrap());
    s.execute("select.set", &json!({"ids": [bg.0, label.0, dot.0]})).unwrap();
    let main = id_of(&s.execute("component.create", &json!({"name": "Button"})).unwrap());
    let inst = id_of(&s.execute("symbol.place", &json!({"name": "Button", "x": 300, "y": 200})).unwrap());
    (main, inst, bg, label, dot)
}

#[test]
fn text_and_boolean_properties_drive_instances() {
    let mut s = Session::new();
    let (main, inst, _, label, dot) = button(&mut s);
    s.execute("component.addProperty", &json!({"id": main.0, "name": "Label", "layer": label.0})).unwrap();
    s.execute("component.addProperty", &json!({"id": main.0, "name": "Icon", "layer": dot.0})).unwrap();
    let props = s.execute("component.properties", &json!({"id": main.0})).unwrap();
    assert_eq!(props["props"].as_array().unwrap().len(), 2);
    assert_eq!(props["props"][1]["default"], "Buy now", "a text property starts with the layer's text");
    // The instance's own values.
    s.execute("select.set", &json!({"ids": [inst.0]})).unwrap();
    s.execute("instance.setProperties", &json!({"properties": {"Label": "Sign up", "Icon": false}})).unwrap();
    let art = shown(&s, inst);
    assert_eq!(texts(&art), vec!["Sign up"]);
    let mut hidden = 0;
    art.walk(&mut |l: &Node| hidden += usize::from(!l.visible));
    assert_eq!(hidden, 1, "the icon is hidden");
    // Another instance still shows the defaults.
    let other = id_of(&s.execute("symbol.place", &json!({"name": "Button", "x": 300, "y": 300})).unwrap());
    assert_eq!(texts(&shown(&s, other)), vec!["Buy now"]);
    // The values survive an edit of the main.
    s.execute("select.set", &json!({"ids": [main.0]})).unwrap();
    s.execute("object.move", &json!({"dx": 5, "dy": 0})).unwrap();
    assert_eq!(texts(&shown(&s, inst)), vec!["Sign up"]);
    // Bad values are refused.
    s.execute("select.set", &json!({"ids": [inst.0]})).unwrap();
    assert!(s.execute("instance.setProperties", &json!({"properties": {"Icon": "maybe"}})).is_err());
    assert!(s.execute("instance.setProperties", &json!({"properties": {"Nope": 1}})).is_err());
}

#[test]
fn layer_overrides_survive_main_edits_and_reset() {
    let mut s = Session::new();
    let (main, inst, bg, label, _) = button(&mut s);
    s.execute("select.set", &json!({"ids": [inst.0]})).unwrap();
    s.execute("instance.setOverride", &json!({"layer": bg.0, "fill": "#ff0000"})).unwrap();
    s.execute("instance.setOverride", &json!({"layer": label.0, "characters": "Hello"})).unwrap();
    let art = shown(&s, inst);
    assert!(rect_fills(&art).contains(&"#ff0000".to_string()));
    assert_eq!(texts(&art), vec!["Hello"]);
    // Recolour the main's background: the instance keeps its red, the others follow the main.
    let other = id_of(&s.execute("symbol.place", &json!({"name": "Button", "x": 300, "y": 300})).unwrap());
    s.execute("select.set", &json!({"ids": [bg.0]})).unwrap();
    s.execute("paint.setFill", &json!({"color": "#00ff00"})).unwrap();
    assert!(rect_fills(&shown(&s, inst)).contains(&"#ff0000".to_string()), "the override stays");
    assert!(rect_fills(&shown(&s, other)).contains(&"#00ff00".to_string()), "the main's edit reaches the others");
    // A main edit the instance doesn't override reaches it too.
    s.execute("select.set", &json!({"ids": [label.0]})).unwrap();
    s.execute("text.setText", &json!({"text": "Buy"})).unwrap();
    assert_eq!(texts(&shown(&s, other)), vec!["Buy"]);
    assert_eq!(texts(&shown(&s, inst)), vec!["Hello"], "its text override wins");
    // Reset one layer, then everything.
    s.execute("select.set", &json!({"ids": [inst.0]})).unwrap();
    s.execute("component.resetOverrides", &json!({"layer": bg.0})).unwrap();
    assert!(rect_fills(&shown(&s, inst)).contains(&"#00ff00".to_string()));
    s.execute("component.resetOverrides", &json!({})).unwrap();
    let NodeKind::SymbolInstance { symbol, .. } = node(&s, inst).kind else { panic!() };
    assert_eq!(symbol, "Button", "back to the component's own symbol");
    let d = &s.doc().unwrap().doc;
    assert!(!d.symbols.iter().any(|x| cmd::components::is_derived(&x.name)), "made symbols went");
    // Go to main still works from an instance with overrides.
    s.execute("instance.setOverride", &json!({"layer": bg.0, "fill": "#123456"})).unwrap();
    assert_eq!(id_of(&s.execute("component.goToMain", &json!({})).unwrap()), main);
    assert_eq!(s.execute("component.list", &json!({})).unwrap()["components"][0]["instances"], 2);
}

#[test]
fn detach_keeps_the_look_and_overrides_save() {
    let mut s = Session::new();
    let (_, inst, bg, _, _) = button(&mut s);
    s.execute("select.set", &json!({"ids": [inst.0]})).unwrap();
    s.execute("instance.setOverride", &json!({"layer": bg.0, "fill": "#ff8800"})).unwrap();
    // Saved and opened: the instance keeps its overrides.
    let bytes = logocraft_format::save(&s.doc().unwrap().doc, false);
    let doc = logocraft_format::load(&bytes).unwrap();
    let data = doc.node(inst).unwrap().extras().instance.clone().unwrap();
    assert_eq!(data.overrides.get(&bg.0).and_then(|o| o.fill.as_ref()).and_then(|f| f.color()), Some(Color::rgb8(0xff, 0x88, 0)));
    // Detach: plain art in the overridden look.
    let r = s.execute("component.detach", &json!({})).unwrap();
    let art = node(&s, NodeId(r["ids"][0].as_u64().unwrap()));
    assert!(rect_fills(&art).contains(&"#ff8800".to_string()));
    let _ = Paint::None;
}

#[test]
fn binding_and_property_edits() {
    let mut s = Session::new();
    let (main, inst, bg, label, _) = button(&mut s);
    // A property made first, bound afterwards; renamed; the instance's value follows the rename.
    s.execute("component.addProperty", &json!({"id": main.0, "name": "Text", "type": "text", "default": "Go"})).unwrap();
    s.execute("component.bindProperty", &json!({"layer": label.0, "property": "Text"})).unwrap();
    s.execute("select.set", &json!({"ids": [inst.0]})).unwrap();
    s.execute("instance.setProperties", &json!({"properties": {"Text": "Pay"}})).unwrap();
    s.execute("component.editProperty", &json!({"id": main.0, "name": "Text", "newName": "Label"})).unwrap();
    assert_eq!(texts(&shown(&s, inst)), vec!["Pay"]);
    // A boolean can't drive the text field; text can't drive a rectangle.
    assert!(s.execute("component.bindProperty", &json!({"layer": bg.0, "property": "Label"})).is_err());
    // Deleting the property unbinds it: the main's text shows again.
    s.execute("component.deleteProperty", &json!({"id": main.0, "name": "Label"})).unwrap();
    assert_eq!(texts(&shown(&s, inst)), vec!["Buy now"]);
    assert!(node(&s, label).extras().prop_refs.is_none());
}
