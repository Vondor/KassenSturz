// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `&'static` texts of `ImporterInfo` and `ExporterInfo`, which the
//! built-in formats write as constants. A plugin's ID and file name
//! extensions are kept once for the life of the app: each distinct text and
//! list is stored the first time and reused after, so loading the same
//! plugin again stores nothing new. A plugin ID has at most 128 bytes and an
//! extension list at most 8 short words, so this stays small.

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

static TEXTS: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
static LISTS: Mutex<BTreeSet<&'static [&'static str]>> = Mutex::new(BTreeSet::new());

/// `text`, kept for the life of the app.
pub(crate) fn text(text: &str) -> &'static str {
    let mut texts = TEXTS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(kept) = texts.get(text) {
        return kept;
    }
    let kept: &'static str = Box::leak(text.to_owned().into_boxed_str());
    texts.insert(kept);
    kept
}

/// `words`, kept for the life of the app.
pub(crate) fn list(words: &[&str]) -> &'static [&'static str] {
    let words: Vec<&'static str> = words.iter().map(|word| text(word)).collect();
    let mut lists = LISTS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(kept) = lists.get(words.as_slice()) {
        return kept;
    }
    let kept: &'static [&'static str] = Box::leak(words.into_boxed_slice());
    lists.insert(kept);
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_text_and_list_are_kept_once() {
        let first = text("org.example.kept-once");
        let second = text(&String::from("org.example.kept-once"));
        assert!(std::ptr::eq(first, second));
        let first = list(&["csv", "txt"]);
        let second = list(&["csv", "txt"]);
        assert!(std::ptr::eq(first, second));
        assert_eq!(first, ["csv", "txt"]);
    }
}
