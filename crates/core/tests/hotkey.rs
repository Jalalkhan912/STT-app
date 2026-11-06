use dictation_core::{events::Event, hotkey::HotkeyTracker, settings::HotkeySpec};
fn t(keys: &[&str]) -> HotkeyTracker {
    HotkeyTracker::new(HotkeySpec {
        keys: keys.iter().map(|s| s.to_string()).collect(),
    })
}

#[test]
fn single_key_press_release_ignores_repeat() {
    let mut h = t(&["AltRight"]);
    assert_eq!(
        h.on_key("AltRight", true, 1000),
        Some(Event::Pressed { at_ms: 1000 })
    );
    assert_eq!(h.on_key("AltRight", true, 1050), None);
    assert_eq!(
        h.on_key("AltRight", false, 2000),
        Some(Event::Released { at_ms: 2000 })
    );
}
#[test]
fn combo_requires_all_keys() {
    let mut h = t(&["ControlLeft", "Space"]);
    assert_eq!(h.on_key("ControlLeft", true, 1), None);
    assert_eq!(
        h.on_key("Space", true, 2),
        Some(Event::Pressed { at_ms: 2 })
    );
    assert_eq!(
        h.on_key("ControlLeft", false, 3),
        Some(Event::Released { at_ms: 3 })
    );
    assert_eq!(h.on_key("Space", false, 4), None);
}
#[test]
fn escape_cancels_and_swallows_release() {
    let mut h = t(&["AltRight"]);
    h.on_key("AltRight", true, 1);
    assert_eq!(h.on_key("Escape", true, 2), Some(Event::Cancel));
    assert_eq!(h.on_key("AltRight", false, 3), None);
    assert_eq!(
        h.on_key("AltRight", true, 4),
        Some(Event::Pressed { at_ms: 4 })
    );
}
#[test]
fn escape_when_idle_does_nothing() {
    assert_eq!(t(&["AltRight"]).on_key("Escape", true, 1), None);
}
#[test]
fn unrelated_keys_ignored() {
    let mut h = t(&["AltRight"]);
    h.on_key("AltRight", true, 1);
    assert_eq!(h.on_key("KeyA", true, 2), None);
    assert_eq!(h.on_key("KeyA", false, 3), None);
}
#[test]
fn empty_spec_never_activates() {
    let mut h = t(&[]);
    assert_eq!(h.on_key("KeyA", true, 1), None);
    assert_eq!(h.on_key("KeyA", false, 2), None);
}
#[test]
fn set_spec_resets_state() {
    let mut h = t(&["AltRight"]);
    h.on_key("AltRight", true, 1);
    h.set_spec(
        HotkeySpec {
            keys: vec!["F9".into()],
        },
        1,
    );
    assert_eq!(h.on_key("F9", true, 2), Some(Event::Pressed { at_ms: 2 }));
}

fn spec(keys: &[&str]) -> HotkeySpec {
    HotkeySpec {
        keys: keys.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn set_spec_while_held_releases() {
    let mut h = t(&["AltRight"]);
    h.on_key("AltRight", true, 1000);
    assert_eq!(
        h.set_spec(spec(&["F9"]), 2000),
        Some(Event::Released { at_ms: 2000 })
    );
    // The old key going up afterwards is not a second release.
    assert_eq!(h.on_key("AltRight", false, 2100), None);
    assert_eq!(
        h.on_key("F9", true, 2200),
        Some(Event::Pressed { at_ms: 2200 })
    );
}

#[test]
fn set_spec_during_short_hold_cancels() {
    let mut h = t(&["AltRight"]);
    h.on_key("AltRight", true, 1000);
    assert_eq!(h.set_spec(spec(&["F9"]), 1100), Some(Event::Cancel));
}

#[test]
fn set_spec_when_idle_emits_nothing() {
    let mut h = t(&["AltRight"]);
    assert_eq!(h.set_spec(spec(&["F9"]), 5), None);
}
