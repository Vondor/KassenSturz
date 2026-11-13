// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Checks of what an answer claims against what happened in the turn
//! (US6, #848, #849, #862), and the removal of internal IDs from an answer
//! (#858).
//!
//! Small models say that a change is prepared or a page is open when
//! nothing happened, answer about the book without having asked it, and
//! say that a bill can be paid in KassenSturz. Code knows what happened,
//! so it adds a fixed note under the answer. Each check reads a short list
//! of phrases per language (English and German) and does not try to
//! understand the text beyond that; a sentence that denies the claim
//! ("nothing was prepared") is not a claim. The note's text is the front
//! end's, translated there; this crate only says which note.

use std::collections::{HashMap, HashSet};

/// A fixed note KassenSturz writes under an answer, in its own words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnswerNote {
    /// The answer says a change is prepared, but no change set was.
    NothingPrepared,
    /// The answer says a page is open, but none was opened.
    NothingOpened,
    /// The answer names book data, but the turn called no function.
    NotChecked,
    /// The answer says a bill or transfer can be made in KassenSturz.
    CannotPay,
}

impl AnswerNote {
    /// The key the front end shows the note by.
    pub const fn key(self) -> &'static str {
        match self {
            Self::NothingPrepared => "nothing-prepared",
            Self::NothingOpened => "nothing-opened",
            Self::NotChecked => "not-checked",
            Self::CannotPay => "cannot-pay",
        }
    }
}

/// What the turn did, so the answer can be held against it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Turn {
    /// A change set, a chart or an issue draft was prepared.
    pub prepared: bool,
    /// A page was asked to open.
    pub opened: bool,
    /// The model called at least one function (a refused call counts).
    pub called: bool,
    /// The model had no functions and got the overview instead.
    pub overview: bool,
    /// The answer already carries the "no change was prepared" note.
    pub no_change_said: bool,
}

/// The notes that belong under `text`, in a fixed order.
pub(crate) fn notes_for(text: &str, turn: Turn, names: &HashSet<String>) -> Vec<AnswerNote> {
    let sentences = sentences(text);
    let mut notes = Vec::new();
    if !turn.prepared
        && !turn.no_change_said
        && sentences.iter().any(|s| claims(s, PREPARED) && !denies(s))
    {
        notes.push(AnswerNote::NothingPrepared);
    }
    if !turn.opened && sentences.iter().any(|s| claims(s, OPENED) && !denies(s)) {
        notes.push(AnswerNote::NothingOpened);
    }
    if !turn.called && !turn.overview && (names_book_data(text, names) || gives_a_count(&sentences))
    {
        notes.push(AnswerNote::NotChecked);
    }
    if sentences.iter().any(|s| says_it_can_pay(s)) {
        notes.push(AnswerNote::CannotPay);
    }
    notes
}

const PREPARED: &[&str] = &[
    "prepared",
    "vorbereitet",
    "ready for your review",
    "waiting for your review",
    "waiting for your confirmation",
    "ready to confirm",
    "zur bestätigung bereit",
    "zur überprüfung bereit",
    "wartet auf ihre bestätigung",
    "wartet auf ihre prüfung",
];

const OPENED: &[&str] = &[
    "opened",
    "is now open",
    "now open",
    "page is open",
    "geöffnet",
    "ist jetzt offen",
    "ist nun offen",
    "habe ich geöffnet",
];

const DENIALS: &[&str] = &[
    "not ",
    "n't",
    "no change",
    "nothing",
    "never",
    "couldn",
    "unable",
    "cannot",
    "can't",
    "nicht",
    "kein",
    "nichts",
    "niemals",
    "konnte",
    "wenn ",
    "if ",
    "once ",
    "sobald",
];

fn claims(sentence: &str, phrases: &[&str]) -> bool {
    let lower = sentence.to_lowercase();
    phrases.iter().any(|phrase| lower.contains(phrase))
}

fn denies(sentence: &str) -> bool {
    let lower = sentence.to_lowercase();
    DENIALS.iter().any(|word| lower.contains(word))
}

fn sentences(text: &str) -> Vec<String> {
    text.split(['.', '!', '?', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A sentence that says KassenSturz pays or sends money (D-053, AGENTS.md
/// §3.3). One that denies it ("KassenSturz never pays a bill") is not a
/// claim.
fn says_it_can_pay(sentence: &str) -> bool {
    let lower = sentence.to_lowercase();
    lower.contains("kassensturz")
        && [
            "pay ",
            "pay.",
            "paid",
            "payment",
            "bezahl",
            "überweis",
            "überweisen",
            "transfer the money",
            "send the money",
            "send money",
        ]
        .iter()
        .any(|verb| lower.contains(verb))
        && !denies(sentence)
}

/// Whether `text` names an account, category or payee of the book (names
/// are lower-cased and at least three characters).
fn names_book_data(text: &str, names: &HashSet<String>) -> bool {
    let lower = text.to_lowercase();
    names.iter().any(|name| {
        lower.match_indices(name.as_str()).any(|(at, found)| {
            let before = lower[..at].chars().next_back();
            let after = lower[at + found.len()..].chars().next();
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
        })
    })
}

const COUNT_WORDS: &[&str] = &[
    "transaction",
    "buchung",
    "entries",
    "entry",
    "eintr",
    "account",
    "konto",
    "konten",
    "categor",
    "kategorie",
    "payee",
    "empfänger",
    "rule",
    "regel",
];

/// Whether a sentence gives a count: a number followed, within two words,
/// by something the book holds ("12 transactions", "3 Konten").
fn gives_a_count(sentences: &[String]) -> bool {
    sentences.iter().any(|sentence| {
        let words: Vec<String> = sentence
            .split_whitespace()
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .collect();
        words.iter().enumerate().any(|(i, word)| {
            !word.is_empty()
                && word.chars().all(|c| c.is_ascii_digit())
                && words
                    .iter()
                    .skip(i + 1)
                    .take(2)
                    .any(|next| COUNT_WORDS.iter().any(|noun| next.contains(noun)))
        })
    })
}

/// Whether `text` is an ID as KassenSturz writes them: 36 characters,
/// hexadecimal digits in groups of 8-4-4-4-12.
fn is_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, &b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

/// `text` without internal IDs: each is replaced by the name KassenSturz
/// gave it in a function's result when it is known, and left out
/// otherwise, with the brackets or the "ID" word around it (#858).
pub(crate) fn without_ids(text: &str, names: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    let mut removed = false;
    while at < text.len() {
        let rest = &text[at..];
        if rest.len() >= 36
            && rest.is_char_boundary(36)
            && is_id(&rest[..36])
            && !text[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_hexdigit() || c == '-')
            && !rest[36..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_hexdigit() || c == '-')
        {
            match names.get(&rest[..36].to_ascii_lowercase()) {
                Some(name) => out.push_str(name),
                None => removed = true,
            }
            at += 36;
            continue;
        }
        let c = rest.chars().next().unwrap_or(' ');
        out.push(c);
        at += c.len_utf8();
    }
    if removed { tidy(&out) } else { out }
}

/// Takes the leftovers of a removed ID out: empty brackets, a dangling
/// "ID", and doubled spaces.
fn tidy(text: &str) -> String {
    let mut text = text.to_owned();
    for left in [
        " (ID: )", " (id: )", " (ID )", " (id )", " ()", " [ID: ]", " []",
    ] {
        text = text.replace(left, "");
    }
    while text.contains("  ") {
        text = text.replace("  ", " ");
    }
    text.replace(" ,", ",").replace(" .", ".")
}

/// The names KassenSturz gave to IDs in a function's result: every object
/// with a text `id` and a text `name`, at any depth.
pub(crate) fn collect_names(value: &serde_json::Value, names: &mut HashMap<String, String>) {
    match value {
        serde_json::Value::Object(map) => {
            if let (Some(serde_json::Value::String(id)), Some(serde_json::Value::String(name))) =
                (map.get("id"), map.get("name"))
                && is_id(id)
                && names.len() < 5000
            {
                names.insert(id.to_ascii_lowercase(), name.clone());
            }
            for inner in map.values() {
                collect_names(inner, names);
            }
        }
        serde_json::Value::Array(items) => {
            for inner in items {
                collect_names(inner, names);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0b9e5c1e-6a7d-4f3b-9d21-3c8a1b2d4e5f";

    fn none() -> HashSet<String> {
        HashSet::new()
    }

    fn idle() -> Turn {
        Turn {
            called: true,
            ..Turn::default()
        }
    }

    #[test]
    fn a_claim_of_a_prepared_change_without_one_gets_a_note() {
        let notes = notes_for(
            "I've prepared a new Rule for you to confirm.",
            idle(),
            &none(),
        );
        assert_eq!(notes, vec![AnswerNote::NothingPrepared]);
        let notes = notes_for("Ich habe die Änderung vorbereitet.", idle(), &none());
        assert_eq!(notes, vec![AnswerNote::NothingPrepared]);
    }

    #[test]
    fn a_prepared_change_or_a_denial_gets_no_note() {
        let turn = Turn {
            prepared: true,
            ..idle()
        };
        assert!(notes_for("I've prepared a Rule.", turn, &none()).is_empty());
        assert!(
            notes_for(
                "I could not prepare it, nothing is prepared.",
                idle(),
                &none()
            )
            .is_empty()
        );
        assert!(notes_for("Es wurde nichts vorbereitet.", idle(), &none()).is_empty());
        let said = Turn {
            no_change_said: true,
            ..idle()
        };
        assert!(notes_for("I've prepared a Rule.", said, &none()).is_empty());
    }

    #[test]
    fn a_page_claimed_open_without_one_gets_a_note() {
        let notes = notes_for("I've opened the Rules page.", idle(), &none());
        assert_eq!(notes, vec![AnswerNote::NothingOpened]);
        let turn = Turn {
            opened: true,
            ..idle()
        };
        assert!(notes_for("I've opened the Rules page.", turn, &none()).is_empty());
        assert_eq!(
            notes_for("Die Seite ist jetzt geöffnet.", idle(), &none()),
            vec![AnswerNote::NothingOpened]
        );
    }

    #[test]
    fn book_data_without_a_function_call_gets_the_not_checked_note() {
        let names: HashSet<String> = ["checking".to_owned(), "food".to_owned()].into();
        let turn = Turn::default();
        assert_eq!(
            notes_for("Your Checking account is fine.", turn, &names),
            vec![AnswerNote::NotChecked]
        );
        assert_eq!(
            notes_for("You have 12 transactions in March.", turn, &none()),
            vec![AnswerNote::NotChecked]
        );
        // A name inside another word is not a name.
        assert!(notes_for("The foodstuffs are many.", turn, &names).is_empty());
        // A function was called, or the model got the overview.
        assert!(notes_for("Your Checking account is fine.", idle(), &names).is_empty());
        let overview = Turn {
            overview: true,
            ..turn
        };
        assert!(notes_for("Your Checking account is fine.", overview, &names).is_empty());
        // No book data and no count: nothing to check.
        assert!(notes_for("Hello! How can I help?", turn, &names).is_empty());
        assert!(notes_for("It was in 2026.", turn, &names).is_empty());
    }

    #[test]
    fn a_bill_said_to_be_payable_in_kassensturz_gets_a_note() {
        for text in [
            "You can pay this bill directly in KassenSturz.",
            "KassenSturz can pay the invoice for you.",
            "Sie können die Rechnung direkt in KassenSturz bezahlen.",
        ] {
            assert_eq!(
                notes_for(text, idle(), &none()),
                vec![AnswerNote::CannotPay],
                "{text}"
            );
        }
        for text in [
            "KassenSturz never pays a bill; do that in your bank's app.",
            "In KassenSturz können Sie nichts bezahlen.",
            "The payment of 40.00 was booked on Monday.",
        ] {
            assert!(notes_for(text, idle(), &none()).is_empty(), "{text}");
        }
    }

    #[test]
    fn an_id_is_replaced_by_its_name_or_left_out() {
        let mut names = HashMap::new();
        names.insert(ID.to_owned(), "Food › Algae".to_owned());
        assert_eq!(
            without_ids(&format!("Category {ID} is high."), &names),
            "Category Food › Algae is high."
        );
        assert_eq!(
            without_ids(&format!("Category (ID: {ID}) is high."), &HashMap::new()),
            "Category is high."
        );
        assert_eq!(
            without_ids(&format!("Total ({ID})."), &HashMap::new()),
            "Total."
        );
        // Not an ID: too short, or part of a longer hex run.
        assert_eq!(without_ids("0b9e5c1e-6a7d", &names), "0b9e5c1e-6a7d");
        assert_eq!(without_ids(&format!("a{ID}"), &names), format!("a{ID}"));
    }

    #[test]
    fn names_come_from_the_results_objects() {
        let value =
            serde_json::json!({"categories": [{"id": ID, "name": "Food", "kind": "spending"}]});
        let mut names = HashMap::new();
        collect_names(&value, &mut names);
        assert_eq!(names.get(ID).map(String::as_str), Some("Food"));
    }
}
