// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The FinTS product registration number (D-058) as a tester types it
//! while the app runs (D-260). The registration office's terms keep the
//! number from being passed on, so it isn't part of the source, the build
//! or any file: whoever holds it types it once per run, and it lives in
//! memory only, until KassenSturz closes. It never shows itself, is never
//! logged, and goes only into the product field of the first message to
//! the bank (HKVVB), through AqBanking.

use std::fmt;

use zeroize::Zeroizing;

use crate::error::BankError;

/// How many characters a FinTS product registration number has.
pub const REGISTRATION_KEY_LENGTH: usize = 25;

/// A FinTS product registration number: exactly
/// [`REGISTRATION_KEY_LENGTH`] ASCII letters and digits. Nothing else is
/// let through, so a typing slip can't put FinTS's own separators (`'`,
/// `+`, `:`, `?`) into a message to the bank. It has no `Display`, its
/// `Debug` shows no value, and the memory is wiped when it is dropped.
#[derive(Clone, PartialEq, Eq)]
pub struct RegistrationKey(Zeroizing<String>);

impl RegistrationKey {
    /// Takes the number the user typed; spaces before and after it, e.g.
    /// from pasting, don't count. The `String` is moved in and wiped, so
    /// no copy outlives this value unwiped. Anything else than 25 letters
    /// and digits is [`BankError::MalformedRegistration`].
    pub fn parse(typed: String) -> Result<Self, BankError> {
        let typed = Zeroizing::new(typed);
        let key = typed.trim();
        if key.len() == REGISTRATION_KEY_LENGTH && key.bytes().all(|b| b.is_ascii_alphanumeric()) {
            Ok(Self(Zeroizing::new(key.to_owned())))
        } else {
            Err(BankError::MalformedRegistration)
        }
    }

    /// The number, for the bank backend only.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RegistrationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RegistrationKey(…)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up number of the right shape; no real one is ever written
    /// down (D-260).
    const MADE_UP: &str = "0123456789ABCDEFGHIJKLMNO";

    #[test]
    fn twenty_five_letters_and_digits_are_a_key() {
        let key = RegistrationKey::parse(MADE_UP.to_owned()).expect("the right shape");
        assert_eq!(key.as_str(), MADE_UP);
        let lower = MADE_UP.to_ascii_lowercase();
        assert_eq!(
            RegistrationKey::parse(lower.clone()).map(|key| key.as_str().to_owned()),
            Ok(lower)
        );
    }

    #[test]
    fn spaces_around_a_pasted_key_dont_count() {
        let key = RegistrationKey::parse(format!("  {MADE_UP}\n")).expect("trimmed");
        assert_eq!(key.as_str(), MADE_UP);
    }

    #[test]
    fn anything_else_is_malformed() {
        for typed in [
            String::new(),
            MADE_UP[..24].to_owned(),
            format!("{MADE_UP}P"),
            format!("{}'", &MADE_UP[..24]),
            format!("{}+", &MADE_UP[..24]),
            format!("{}:", &MADE_UP[..24]),
            format!("{} {}", &MADE_UP[..12], &MADE_UP[13..]),
            format!("{}Ä", &MADE_UP[..24]),
            format!("{}\0", &MADE_UP[..24]),
        ] {
            assert_eq!(
                RegistrationKey::parse(typed.clone()),
                Err(BankError::MalformedRegistration),
                "{typed:?}"
            );
        }
    }

    #[test]
    fn the_key_never_shows_itself() {
        let key = RegistrationKey::parse(MADE_UP.to_owned()).expect("the right shape");
        let shown = format!("{key:?}");
        assert!(!shown.contains(MADE_UP), "{shown}");
        assert_eq!(shown, "RegistrationKey(…)");
    }
}
