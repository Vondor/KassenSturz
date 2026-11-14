// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What an account's or category's name may hold (B-272): one line, no
//! character that shows as nothing, and not so long that it can't be shown.
//! Two names are the same when they look the same, so a look-alike "Giro"
//! with a hidden character can't stand next to the real one.

use crate::error::StoreError;

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
/// ([`account_name_key`]).
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
    if name.chars().count() > MAX_ACCOUNT_NAME_CHARS {
        return Err(refused("too-long"));
    }
    Ok(())
}

/// What two names are compared by: without the spaces around them, with
/// each run of spaces inside as one, without joiners, variation selectors
/// and characters that show as nothing, and in lower case. Two names with
/// the same key look the same.
pub fn account_name_key(name: &str) -> String {
    name.chars()
        .filter(|&c| {
            !is_invisible(c) && !matches!(c, '\u{200C}' | '\u{200D}' | '\u{FE00}'..='\u{FE0F}')
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// `wanted` made into a name the book keeps, for a name another program
/// wrote: control characters such as a line break become spaces, characters
/// that show as nothing go, and a name longer than
/// [`MAX_ACCOUNT_NAME_CHARS`] is cut. May be empty.
pub fn fitting_account_name(wanted: &str) -> String {
    let cleaned: String = wanted
        .chars()
        .filter(|&c| !is_invisible(c))
        .map(|c| if c.is_control() { ' ' } else { c })
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
        let giro = account_name_key("Giro");
        for look_alike in [
            "Giro\u{200B}",
            "Gi\u{200D}ro",
            " giro\u{00A0}",
            "\u{FEFF}GIRO",
            "Gi\u{00AD}ro",
            "\u{202E}Giro",
        ] {
            assert_eq!(account_name_key(look_alike), giro, "{look_alike:?}");
        }
        assert_eq!(account_name_key("Daily  money"), "daily money");
        assert_ne!(account_name_key("Giro 2"), giro);
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
