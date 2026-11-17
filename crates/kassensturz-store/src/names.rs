// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What an account's or category's name may hold (B-272): one line, no
//! character that shows as nothing, and not so long that it can't be shown.
//! Two names are the same when they look the same, so a look-alike "Giro"
//! with a hidden character can't stand next to the real one. Names of
//! accounts, categories and Rules are stored composed (NFC, D-782); those
//! and the names of payees and tags are compared by [`name_key`].

use crate::error::StoreError;
use unicode_normalization::UnicodeNormalization;

/// The most characters the name of an account or category may have,
/// without the spaces around it.
pub const MAX_ACCOUNT_NAME_CHARS: usize = 200;

/// Whether `c` shows as nothing on its own: a zero-width space or joiner
/// that isn't needed between two letters, a mark that turns the direction of
/// the text, a soft hyphen, or a letter that is only blank space. A name with
/// one could look exactly like another name.
///
/// The joiners U+200C and U+200D are left out: some scripts and emoji need
/// them between two characters. They don't count when names are compared
/// ([`name_key`]).
pub fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{115F}'
            | '\u{1160}'
            | '\u{17B4}'
            | '\u{17B5}'
            | '\u{180E}'
            | '\u{200B}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{3164}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
    )
}

/// Refuses an account's or category's name the book can't keep: empty, on
/// more than one line or with another control character (`line-break`),
/// with a character that shows as nothing (`invisible`), or longer than
/// [`MAX_ACCOUNT_NAME_CHARS`] (`too-long`).
pub(crate) fn check_account_name(name: &str) -> Result<(), StoreError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::EmptyAccountName);
    }
    let refused = |reason| StoreError::AccountName {
        reason,
        max_chars: MAX_ACCOUNT_NAME_CHARS,
    };
    if name.chars().any(char::is_control) {
        return Err(refused("line-break"));
    }
    if name.chars().any(is_invisible) {
        return Err(refused("invisible"));
    }
    if name.nfc().count() > MAX_ACCOUNT_NAME_CHARS {
        return Err(refused("too-long"));
    }
    Ok(())
}

/// Refuses a Rule's name or a payee typed in that holds a line break or
/// another control character (`line-break`), or a character that shows as
/// nothing or turns the text around (`invisible`), as
/// [`StoreError::HiddenText`] for `field`.
pub(crate) fn check_hidden_text(text: &str, field: &'static str) -> Result<(), StoreError> {
    let refused = |reason| StoreError::HiddenText { field, reason };
    if text.chars().any(char::is_control) {
        return Err(refused("line-break"));
    }
    if text.chars().any(is_invisible) {
        return Err(refused("invisible"));
    }
    Ok(())
}

/// What two names are compared by, and what a name typed to find one is
/// compared by: without the spaces around them, with each run of spaces
/// inside as one, without joiners, variation selectors and characters that
/// show as nothing, in Unicode's compatibility form (NFKC, D-782), so "é"
/// typed as "e" and a combining accent is "é" and the ligature "\u{FB01}" is
/// "fi", and in lower case. Two names with the same key look the same. The
/// key is only compared, never stored: the book keeps the name itself,
/// composed ([`stored_name`]).
pub fn name_key(name: &str) -> String {
    name.chars()
        .filter(|&c| {
            !is_invisible(c) && !matches!(c, '\u{200C}' | '\u{200D}' | '\u{FE00}'..='\u{FE0F}')
        })
        .nfkc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .nfkc()
        .collect()
}

/// `name` as the book stores it: in Unicode's composed form (NFC, D-782),
/// which looks the same and is what most keyboards type.
pub fn stored_name(name: &str) -> String {
    name.nfc().collect()
}

/// `text` from a bank, a file or another program's book as KassenSturz
/// shows it and as a payee it takes over (B-365): a character that shows as
/// nothing or turns the text around ([`is_invisible`]) goes, and a control
/// character such as a line break becomes a space. Otherwise unchanged, so
/// an ordinary text stays as it was. The joiners some scripts need stay.
/// What a bank or file delivered is kept as delivered (D-375); this is how
/// it is shown, the same as typed text may be (B-272) and as the AI tools
/// see it.
pub fn shown_text(text: &str) -> String {
    if !text.chars().any(|c| is_invisible(c) || c.is_control()) {
        return text.to_owned();
    }
    text.chars()
        .filter(|&c| !is_invisible(c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// `wanted` made into a name the book keeps, for a name another program
/// wrote: control characters such as a line break become spaces, characters
/// that show as nothing go, the name is composed (NFC, D-782), and a name
/// longer than
/// [`MAX_ACCOUNT_NAME_CHARS`] is cut. May be empty.
pub fn fitting_account_name(wanted: &str) -> String {
    let cleaned: String = wanted
        .chars()
        .filter(|&c| !is_invisible(c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .nfc()
        .collect();
    let cleaned = cleaned.trim();
    cleaned
        .chars()
        .take(MAX_ACCOUNT_NAME_CHARS)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_that_look_alike_have_one_key() {
        let giro = name_key("Giro");
        for look_alike in [
            "Giro\u{200B}",
            "Gi\u{200D}ro",
            " giro\u{00A0}",
            "\u{FEFF}GIRO",
            "Gi\u{00AD}ro",
            "\u{202E}Giro",
        ] {
            assert_eq!(name_key(look_alike), giro, "{look_alike:?}");
        }
        assert_eq!(name_key("Daily  money"), "daily money");
        assert_ne!(name_key("Giro 2"), giro);
        // An accent typed as its own character is the accented letter
        // (D-782), also in capitals.
        assert_eq!(name_key("Giro\u{301}"), name_key("Gir\u{F3}"));
        assert_eq!(name_key("Cafe\u{301}"), name_key("CAF\u{C9}"));
        assert_ne!(name_key("Gir\u{F3}"), giro);
        // A ligature or a full-width letter is the letters it looks like.
        assert_eq!(name_key("\u{FB01}nanz"), name_key("Finanz"));
        assert_eq!(name_key("\u{FF27}iro"), giro);
        // The key of a key is the key: comparing twice changes nothing.
        for name in [
            "\u{FB01}nanz",
            "Cafe\u{301}",
            "\u{130}stanbul",
            " Gi\u{200D}ro  2 ",
        ] {
            assert_eq!(name_key(&name_key(name)), name_key(name), "{name:?}");
        }
    }

    #[test]
    fn a_name_is_stored_and_fitted_composed() {
        assert_eq!(stored_name("Cafe\u{301}"), "Caf\u{E9}");
        assert_eq!(stored_name("Caf\u{E9}"), "Caf\u{E9}");
        assert_eq!(fitting_account_name("Giro\u{301}\n"), "Gir\u{F3}");
        // 200 letters with an accent typed on its own each still fit.
        let long = "e\u{301}".repeat(MAX_ACCOUNT_NAME_CHARS);
        assert!(check_account_name(&long).is_ok());
        assert_eq!(
            fitting_account_name(&long).chars().count(),
            MAX_ACCOUNT_NAME_CHARS
        );
    }

    #[test]
    fn only_a_name_on_one_visible_line_is_kept() {
        assert!(check_account_name("Giro").is_ok());
        // A family emoji is joined with U+200D and stays allowed.
        assert!(check_account_name("Family \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}").is_ok());
        let reason = |name: &str| match check_account_name(name) {
            Err(StoreError::AccountName { reason, max_chars }) => {
                assert_eq!(max_chars, MAX_ACCOUNT_NAME_CHARS);
                reason
            }
            other => panic!("{name:?}: {other:?}"),
        };
        assert_eq!(reason("Giro\u{200B}"), "invisible");
        assert_eq!(reason("\u{202E}oriG"), "invisible");
        assert_eq!(reason("Line\nBreak"), "line-break");
        assert_eq!(reason("Tab\there"), "line-break");
        assert_eq!(reason(&"x".repeat(MAX_ACCOUNT_NAME_CHARS + 1)), "too-long");
        assert!(check_account_name(&format!(" {} ", "x".repeat(MAX_ACCOUNT_NAME_CHARS))).is_ok());
        assert!(matches!(
            check_account_name(" \u{00A0} "),
            Err(StoreError::EmptyAccountName)
        ));
    }

    #[test]
    fn a_name_from_another_program_is_made_to_fit() {
        assert_eq!(fitting_account_name(" Line\nBreak\u{200B} "), "Line Break");
        assert_eq!(
            fitting_account_name(&"x".repeat(5000)).chars().count(),
            MAX_ACCOUNT_NAME_CHARS
        );
        assert_eq!(fitting_account_name("\u{200B}"), "");
        assert!(check_account_name(&fitting_account_name("Giro\u{2066}\t")).is_ok());
    }
}

#[cfg(test)]
mod shown_tests {
    use super::shown_text;

    /// B-365: what hides or turns a delivered text around goes; the
    /// joiners some scripts need and ordinary text stay.
    #[test]
    fn a_text_is_shown_without_what_hides_or_turns_it() {
        assert_eq!(shown_text("Bäcker \u{202E}nebaG\u{202C}"), "Bäcker nebaG");
        assert_eq!(shown_text("\u{2067}Mallory\u{2069}"), "Mallory");
        assert_eq!(shown_text("Rent\u{200B} October"), "Rent October");
        assert_eq!(shown_text("Line one\nline two"), "Line one line two");
        assert_eq!(
            shown_text("\u{1F468}\u{200D}\u{1F373} Café"),
            "\u{1F468}\u{200D}\u{1F373} Café"
        );
        assert_eq!(shown_text("Corner Grocery"), "Corner Grocery");
    }
}
