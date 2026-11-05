//! Rule-based text cleanup applied to raw transcripts.

use crate::settings::CleanupConfig;
use regex::{NoExpand, Regex};

/// Whole-word, case-insensitive pattern for `phrase`. `\b` is only added on
/// sides where the phrase edge is a word character.
fn phrase_pattern(phrase: &str) -> String {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let lead = if phrase.chars().next().is_some_and(is_word) {
        r"\b"
    } else {
        ""
    };
    let trail = if phrase.chars().last().is_some_and(is_word) {
        r"\b"
    } else {
        ""
    };
    format!("(?i){lead}{}{trail}", regex::escape(phrase))
}

fn compile(pattern: &str) -> Option<Regex> {
    Regex::new(pattern).ok()
}

fn upper_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Uppercase the first non-whitespace character after every newline.
fn capitalize_after_newlines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut after_newline = false;
    for ch in s.chars() {
        if after_newline && !ch.is_whitespace() {
            out.extend(ch.to_uppercase());
            after_newline = false;
        } else {
            out.push(ch);
            if ch == '\n' {
                after_newline = true;
            }
        }
    }
    out
}

/// Clean a raw transcript. Returns `""` when nothing is left.
pub fn clean(raw: &str, cfg: &CleanupConfig) -> String {
    // 1. trim and collapse whitespace
    let mut text = raw.split_whitespace().collect::<Vec<_>>().join(" ");

    // 2. fillers (also eat one following `,`/`.` and the whitespace after it)
    if cfg.remove_fillers {
        for filler in cfg.fillers.iter().filter(|f| !f.trim().is_empty()) {
            if let Some(re) = compile(&format!(r"{}[,.]?\s*", phrase_pattern(filler.trim()))) {
                text = re.replace_all(&text, NoExpand("")).into_owned();
            }
        }
        text = text.trim().to_string();
    }

    // 3. replacements, longest phrase first
    let mut replacements: Vec<&(String, String)> = cfg
        .replacements
        .iter()
        .filter(|(from, _)| !from.trim().is_empty())
        .collect();
    replacements.sort_by_key(|(from, _)| std::cmp::Reverse(from.trim().chars().count()));
    let mut inserted_newline = false;
    for (from, to) in replacements {
        let core = format!("{}[,.]?", phrase_pattern(from.trim()));
        let multiline = to.contains('\n');
        let pattern = if multiline {
            format!(r"[ \t]*{core}[ \t]*")
        } else {
            core
        };
        if let Some(re) = compile(&pattern) {
            if multiline && re.is_match(&text) {
                inserted_newline = true;
            }
            text = re.replace_all(&text, NoExpand(to.as_str())).into_owned();
        }
    }
    if inserted_newline {
        text = capitalize_after_newlines(&text);
    }

    // 4. vocabulary
    for word in cfg.vocabulary.iter().filter(|w| !w.trim().is_empty()) {
        let word = word.trim();
        if let Some(re) = compile(&phrase_pattern(word)) {
            text = re.replace_all(&text, NoExpand(word)).into_owned();
        }
    }

    if text.trim().is_empty() {
        return String::new();
    }

    // 5. capitalize first letter
    if cfg.capitalize_first {
        text = upper_first(&text);
    }

    // 6. trailing space
    if cfg.trailing_space && !text.ends_with(char::is_whitespace) {
        text.push(' ');
    }
    text
}
