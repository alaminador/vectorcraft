//! Graphic Styles panel depth: Merge Graphic Styles, moving styles and Override Character Color.

use super::*;

#[test]
fn override_character_color_round_trips_and_defaults_on() {
    let p = Prefs { override_char_color: false, ..Prefs::default() };
    let back: Prefs = serde_json::from_value(p.to_json()).unwrap();
    assert!(!back.override_char_color);
    // Preferences saved before the option had it on.
    let mut old = Prefs::default().to_json();
    old.as_object_mut().unwrap().remove("overrideCharColor");
    assert!(serde_json::from_value::<Prefs>(old).unwrap().override_char_color);
}
