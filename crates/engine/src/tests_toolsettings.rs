//! Tool options that persist across tool switches, documents and launches (`Prefs::tool_settings`).

use serde_json::json;

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
    s
}

fn select(s: &mut Session, tool: &str) {
    s.select_tool(tool, ViewInfo::default()).unwrap();
}

#[test]
fn options_survive_a_tool_switch() {
    let mut s = session();
    select(&mut s, "star");
    s.set_tool_option("points", &json!(7));
    select(&mut s, "selection");
    select(&mut s, "star");
    assert_eq!(s.tool_options()["points"], json!(7));
    // Interaction state isn't kept: the Rotate tool's reference point starts afresh.
    select(&mut s, "rotate");
    s.set_tool_option("origin", &json!([10, 20]));
    select(&mut s, "selection");
    select(&mut s, "rotate");
    assert_eq!(s.tool_options(), json!({"origin": null}));
}

#[test]
fn a_gesture_keeps_what_it_changed_and_a_document_switch_keeps_options() {
    let mut s = session();
    select(&mut s, "polygon");
    s.set_tool_option("sides", &json!(8));
    s.execute("file.new", &json!({"width": 300, "height": 300})).unwrap();
    assert_eq!(s.tool_id(), "polygon");
    assert_eq!(s.tool_options()["sides"], json!(8));
    s.set_active(0);
    assert_eq!(s.tool_options()["sides"], json!(8));
}

#[test]
fn options_are_saved_with_the_preferences() {
    let mut s = session();
    select(&mut s, "polygon");
    s.set_tool_option("sides", &json!(9));
    select(&mut s, "selection");
    let saved = serde_json::to_string(&s.prefs).unwrap();
    assert!(saved.contains("toolSettings"), "{saved}");
    // Next launch.
    let mut t = session();
    t.apply_prefs(serde_json::from_str(&saved).unwrap());
    select(&mut t, "polygon");
    assert_eq!(t.tool_options()["sides"], json!(9));
    // Older preference files have none; none are written until a tool keeps something.
    assert!(Prefs::default().to_json().get("toolSettings").is_none());
    let old: Prefs = serde_json::from_value(json!({"keyboardIncrement": 2})).unwrap();
    assert!(old.tool_settings.is_empty());
    // Resetting the preferences keeps them.
    t.execute("prefs.reset", &json!({})).unwrap();
    assert_eq!(t.prefs.tool_settings["polygon"]["sides"], json!(9));
}
