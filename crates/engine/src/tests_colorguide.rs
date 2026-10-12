//! The Color Guide: `color.harmony` and saving several colours as swatches at once.

use serde_json::{Value, json};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
    s
}

fn hexes(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|h| h.as_str().unwrap().to_string()).collect()
}

#[test]
fn new_swatch_saves_several_colours_in_one_step() {
    let mut s = session();
    let before = s.doc().unwrap().doc.swatches.len();
    let undo = s.doc().unwrap().history.undo.len();
    let r = s.execute("swatch.new", &json!({"colors": ["#112233", "#445566", {"c": 10, "m": 20, "y": 30, "k": 0}]})).unwrap();
    assert_eq!(hexes(&r["names"]).len(), 3);
    let st = s.doc().unwrap();
    assert_eq!(st.doc.swatches.len(), before + 3);
    assert_eq!(st.history.undo.len(), undo + 1, "one undo step");
    assert_eq!(r["name"], "R=17 G=34 B=51");
    assert!(st.doc.swatch("C=10 M=20 Y=30 K=0").is_some());
    // Into a colour group, as global swatches.
    s.execute("swatch.newGroup", &json!({"name": "Guide"})).unwrap();
    let r = s.execute("swatch.new", &json!({"colors": ["#010203", "#040506"], "group": "Guide", "global": true})).unwrap();
    let d = &s.doc().unwrap().doc;
    let g = d.swatch_groups.iter().find(|g| g.name == "Guide").unwrap();
    assert_eq!(g.swatches.len(), 2);
    assert!(g.swatches.iter().all(|w| w.global));
    assert_eq!(hexes(&r["names"]), g.swatches.iter().map(|w| w.name.clone()).collect::<Vec<_>>());
    assert!(s.execute("swatch.new", &json!({"colors": []})).is_err());
    assert!(s.execute("swatch.new", &json!({"colors": ["nope"]})).is_err());
}
