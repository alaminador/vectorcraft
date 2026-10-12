//! The split command modules (`cmd/{appearance,gradient,stroke,style,swatch}.rs`) and the shared
//! helpers in `cmd/mod.rs` (`leaf_targets`, `paint_targets`, `unique_name`).

use serde_json::json;

use super::*;
use crate::cmd::{leaf_targets, paint_targets, unique_name};

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
    s
}

fn rect(s: &mut Session, x: f64) -> NodeId {
    let r = s.execute("shape.rectangle", &json!({"x": x, "y": 10, "width": 20, "height": 20})).unwrap();
    NodeId(r["id"].as_u64().unwrap())
}

fn id_of(v: &serde_json::Value) -> NodeId {
    NodeId(v["id"].as_u64().unwrap())
}

#[test]
fn moved_commands_keep_their_ids() {
    let ids = [
        "appearance.addFill",
        "appearance.addStroke",
        "appearance.clear",
        "appearance.reduceToBasic",
        "appearance.setItem",
        "appearance.removeItem",
        "appearance.addEffect",
        "appearance.duplicateItem",
        "appearance.moveItem",
        "appearance.copyFrom",
        "paint.editGradient",
        "paint.setGradientGeom",
        "stroke.set",
        "stroke.setAdvanced",
        "swatch.new",
        "swatch.delete",
        "swatch.newGroup",
        "swatch.duplicate",
        "swatch.sortByName",
    ];
    let pos = |id: &str| command_specs().iter().position(|c| c.id == id).unwrap_or_else(|| panic!("{id}"));
    let (paint_end, after_paint) = (pos("transparency.set"), pos("transparency.makeOpacityMask"));
    for id in ids {
        assert_eq!(command_specs().iter().filter(|c| c.id == id).count(), 1, "{id}");
        // Registered right after `paint` (where they lived), so the command palette, which lists
        // the first matches in registry order, still shows them (e.g. Stroke Options for "stroke").
        assert!((paint_end..after_paint).contains(&pos(id)), "{id}");
    }
}

#[test]
fn unique_name_appends_the_first_free_number() {
    let taken = ["A", "A 2", "A 3"];
    assert_eq!(unique_name("B", |n| taken.contains(&n)), "B");
    assert_eq!(unique_name("A", |n| taken.contains(&n)), "A 4");
    assert_eq!(unique_name("A 2", |n| taken.contains(&n)), "A 2 2");
}

#[test]
fn leaf_targets_expand_groups_and_stop_at_compound_paths() {
    let mut s = session();
    let (a, b) = (rect(&mut s, 0.0), rect(&mut s, 30.0));
    s.execute("select.set", &json!({"ids": [a.0, b.0]})).unwrap();
    let g = id_of(&s.execute("object.group", &json!({})).unwrap());
    assert_eq!(leaf_targets(&s, &[g]).unwrap(), vec![a, b]);
    let (c, d) = (rect(&mut s, 60.0), rect(&mut s, 90.0));
    s.execute("select.set", &json!({"ids": [c.0, d.0]})).unwrap();
    let comp = id_of(&s.execute("object.compoundPath.make", &json!({})).unwrap());
    assert_eq!(leaf_targets(&s, &[comp]).unwrap(), vec![comp]);
    // `paint_targets` reads `ids`, then `id`, then the selection.
    assert_eq!(paint_targets(&s, &json!({"ids": [g.0]})).unwrap(), vec![a, b]);
    assert_eq!(paint_targets(&s, &json!({"id": comp.0})).unwrap(), vec![comp]);
    s.execute("select.set", &json!({"ids": [g.0, comp.0]})).unwrap();
    assert_eq!(paint_targets(&s, &json!({})).unwrap(), vec![a, b, comp]);
}
