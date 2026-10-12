//! Components (`component.*`): the main component stays editable on the canvas, its instances
//! follow every edit of it, copies of it are instances, and the mark survives a save.

use logocraft_color::{Color, Paint};
use logocraft_doc::{AppearanceItem, Node, NodeId, NodeKind};
use serde_json::{Value, json};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    s
}

fn id_of(v: &Value) -> NodeId {
    NodeId(v["id"].as_u64().unwrap())
}

fn node(s: &Session, id: NodeId) -> Node {
    s.doc().unwrap().doc.node(id).cloned().unwrap()
}

/// The fill colour of the first path in the symbol `name`'s art.
fn symbol_fill(s: &Session, name: &str) -> Option<Color> {
    let d = &s.doc().unwrap().doc;
    let sym = d.symbols.iter().find(|x| x.name == name)?;
    let mut out = None;
    sym.art.walk(&mut |n| {
        if out.is_none() && matches!(n.kind, NodeKind::Path { .. }) {
            out = n.appearance.items.iter().find_map(|i| match i {
                AppearanceItem::Fill(f) => match &f.paint {
                    Paint::Solid { color, .. } => Some(*color),
                    _ => None,
                },
                _ => None,
            });
        }
    });
    out
}

fn instance_xf(s: &Session, id: NodeId) -> logocraft_geom::Affine {
    match node(s, id).kind {
        NodeKind::SymbolInstance { xf, .. } => xf,
        k => panic!("not an instance: {k:?}"),
    }
}

#[test]
fn create_component_keeps_the_art_as_an_editable_main() {
    let mut s = session();
    let rect = id_of(&s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 40, "height": 20})).unwrap());
    let r = s.execute("component.create", &json!({})).unwrap();
    assert_eq!(r["name"], "Component 1");
    let main = id_of(&r);
    assert_eq!(main, rect, "one object becomes the main itself");
    let n = node(&s, main);
    assert!(matches!(n.kind, NodeKind::Path { .. }), "still a path, editable");
    assert_eq!(n.extras().component.as_deref(), Some("Component 1"));
    assert_eq!(s.doc().unwrap().selection.objects, vec![main]);
    // A second one gets the next name; a main can't be made a component again.
    assert!(s.execute("component.create", &json!({})).is_err());
    s.execute("shape.ellipse", &json!({"x": 100, "y": 10, "width": 20, "height": 20})).unwrap();
    assert_eq!(s.execute("component.create", &json!({})).unwrap()["name"], "Component 2");
    // Several objects are grouped into one main.
    let a = id_of(&s.execute("shape.rectangle", &json!({"x": 200, "y": 10, "width": 10, "height": 10})).unwrap());
    let b = id_of(&s.execute("shape.rectangle", &json!({"x": 220, "y": 10, "width": 10, "height": 10})).unwrap());
    s.execute("select.set", &json!({"ids": [a.0, b.0]})).unwrap();
    let g = id_of(&s.execute("component.create", &json!({"name": "Mark"})).unwrap());
    assert!(matches!(node(&s, g).kind, NodeKind::Group { ref children, .. } if children.len() == 2));
    let list = s.execute("component.list", &json!({})).unwrap();
    assert_eq!(list["components"].as_array().unwrap().len(), 3);
}

#[test]
fn instances_follow_every_edit_of_the_main() {
    let mut s = session();
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 40, "height": 20})).unwrap();
    let main = id_of(&s.execute("component.create", &json!({})).unwrap());
    let inst = id_of(&s.execute("symbol.place", &json!({"name": "Component 1", "x": 200, "y": 150})).unwrap());
    // Recolour the main: the symbol its instances draw follows.
    s.execute("select.set", &json!({"ids": [main.0]})).unwrap();
    s.execute("paint.setFill", &json!({"color": "#ff0000"})).unwrap();
    assert_eq!(symbol_fill(&s, "Component 1"), Some(Color::rgb(1.0, 0.0, 0.0)));
    // Make the main twice as wide: the instance grows with it, about its centre.
    let before = instance_xf(&s, inst);
    s.execute("object.setBounds", &json!({"width": 80})).unwrap();
    let after = instance_xf(&s, inst);
    let (b, a) = (before.as_coeffs(), after.as_coeffs());
    assert!((a[0] / b[0] - 2.0).abs() < 1e-6 && (a[3] / b[3] - 1.0).abs() < 1e-6, "{b:?} → {a:?}");
    assert_eq!((a[4], a[5]), (b[4], b[5]), "the instance stays centred where it was");
    // Undo brings both back.
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(instance_xf(&s, inst), before);
}

#[test]
fn a_copy_of_the_main_is_an_instance() {
    let mut s = session();
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 40, "height": 20})).unwrap();
    let main = id_of(&s.execute("component.create", &json!({})).unwrap());
    s.execute("edit.duplicate", &json!({"dx": 100, "dy": 0})).unwrap();
    let copy = s.doc().unwrap().selection.objects[0];
    assert_ne!(copy, main);
    assert!(matches!(node(&s, copy).kind, NodeKind::SymbolInstance { ref symbol, .. } if symbol == "Component 1"));
    assert_eq!(node(&s, main).extras().component.as_deref(), Some("Component 1"), "the original stays the main");
    let b = node(&s, copy).geometric_bounds().unwrap();
    assert!((b.x0 - 110.0).abs() < 1e-6 && (b.width() - 40.0).abs() < 1e-6, "{b:?}");
    // Copy and paste too.
    s.execute("select.set", &json!({"ids": [main.0]})).unwrap();
    s.execute("edit.copy", &json!({})).unwrap();
    s.execute("edit.paste", &json!({})).unwrap();
    let pasted = s.doc().unwrap().selection.objects[0];
    assert!(matches!(node(&s, pasted).kind, NodeKind::SymbolInstance { .. }));
}

#[test]
fn detach_and_go_to_main() {
    let mut s = session();
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 40, "height": 20})).unwrap();
    let main = id_of(&s.execute("component.create", &json!({})).unwrap());
    let inst = id_of(&s.execute("symbol.place", &json!({"name": "Component 1"})).unwrap());
    let r = s.execute("component.goToMain", &json!({})).unwrap();
    assert_eq!(id_of(&r), main);
    assert_eq!(s.doc().unwrap().selection.objects, vec![main]);
    assert!(s.execute("component.goToMain", &json!({})).is_err(), "a main is not an instance");
    s.execute("select.set", &json!({"ids": [inst.0]})).unwrap();
    let r = s.execute("component.detach", &json!({})).unwrap();
    let art = NodeId(r["ids"][0].as_u64().unwrap());
    assert!(!matches!(node(&s, art).kind, NodeKind::SymbolInstance { .. }));
    assert!(node(&s, art).extras().component.is_none(), "detached art is not a main");
    // With the main deleted, Go to Main Component says so.
    let inst = id_of(&s.execute("symbol.place", &json!({"name": "Component 1"})).unwrap());
    s.execute("select.set", &json!({"ids": [main.0]})).unwrap();
    s.execute("edit.clear", &json!({})).unwrap();
    s.execute("select.set", &json!({"ids": [inst.0]})).unwrap();
    assert!(s.execute("component.goToMain", &json!({})).is_err());
}

#[test]
fn the_main_component_mark_survives_save_and_open() {
    let mut s = session();
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 40, "height": 20})).unwrap();
    let main = id_of(&s.execute("component.create", &json!({"name": "Logo"})).unwrap());
    let bytes = logocraft_format::save(&s.doc().unwrap().doc, false);
    let doc = logocraft_format::load(&bytes).unwrap();
    assert_eq!(doc.node(main).unwrap().extras().component.as_deref(), Some("Logo"));
    assert!(doc.symbols.iter().any(|x| x.name == "Logo"));
}

#[test]
fn the_shortcuts_are_figmas() {
    assert_eq!(find_command("component.create").unwrap().shortcut, Some("Cmd+Alt+K"));
    assert_eq!(find_command("component.detach").unwrap().shortcut, Some("Cmd+Alt+B"));
    assert_eq!(find_command("object.blend.make").unwrap().shortcut, None);
}
