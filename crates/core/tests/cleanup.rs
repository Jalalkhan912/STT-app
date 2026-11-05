use dictation_core::{cleanup::clean, settings::CleanupConfig};
fn c(s: &str) -> String {
    clean(s, &CleanupConfig::default())
}

#[test]
fn removes_leading_filler() {
    assert_eq!(c("Um, I think we should go."), "I think we should go. ");
}
#[test]
fn removes_inner_filler() {
    assert_eq!(c("So, uh, let's start."), "So, let's start. ");
}
#[test]
fn filler_inside_word_untouched() {
    assert_eq!(c("The umbrella is here."), "The umbrella is here. ");
}
#[test]
fn only_fillers_becomes_empty() {
    assert_eq!(c("Um, uh."), "");
}
#[test]
fn collapses_whitespace() {
    assert_eq!(c("  hello   world  "), "Hello world ");
}
#[test]
fn new_line_command() {
    assert_eq!(c("Hello. New line. world."), "Hello.\nWorld. ");
}
#[test]
fn new_paragraph_beats_new_line() {
    assert_eq!(c("One. New paragraph. Two."), "One.\n\nTwo. ");
}
#[test]
fn vocabulary_fixes_case() {
    let mut cfg = CleanupConfig::default();
    cfg.vocabulary = vec!["Kubernetes".into()];
    assert_eq!(
        clean("we deploy on kubernetes.", &cfg),
        "We deploy on Kubernetes. "
    );
}
#[test]
fn user_replacement_with_regex_chars() {
    let mut cfg = CleanupConfig::default();
    cfg.replacements.push(("smiley face".into(), ":-)".into()));
    assert_eq!(clean("great smiley face", &cfg), "Great :-) ");
}
#[test]
fn toggles_off() {
    let cfg = CleanupConfig {
        remove_fillers: false,
        capitalize_first: false,
        trailing_space: false,
        ..Default::default()
    };
    assert_eq!(clean("um hello", &cfg), "um hello");
}
