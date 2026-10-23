// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A value that never shows itself and is wiped when dropped (threat T-02).

use std::fmt;

use zeroize::Zeroizing;

/// A secret, such as a banking PIN.
///
/// It has no `Display`, its `Debug` shows no value, and the memory is
/// wiped when it is dropped. The value is read only through
/// [`Secret::expose`], so every place that handles the plain text is easy
/// to find.
#[derive(Clone)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    /// Takes a secret the user typed or the keychain returned. The `String`
    /// is moved in, so no copy outlives this value unwiped; a caller that
    /// still holds another copy has to wipe or drop it itself.
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    /// The plain text, for the one place that needs it: the keychain, or
    /// the bank backend answering the bank's password request.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether the secret is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(…)")
    }
}

impl PartialEq for Secret {
    /// Compares without stopping at the first differing byte, so the time
    /// taken says nothing about where two secrets differ.
    fn eq(&self, other: &Self) -> bool {
        let (mine, theirs) = (self.0.as_bytes(), other.0.as_bytes());
        if mine.len() != theirs.len() {
            return false;
        }
        mine.iter()
            .zip(theirs)
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }
}

impl Eq for Secret {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_shows_no_value() {
        let secret = Secret::new("1234".to_owned());
        assert_eq!(format!("{secret:?}"), "Secret(…)");
        assert!(!format!("{secret:#?}").contains("1234"));
    }

    #[test]
    fn the_value_is_read_only_through_expose() {
        let secret = Secret::new("1234".to_owned());
        assert_eq!(secret.expose(), "1234");
        assert!(!secret.is_empty());
        assert!(Secret::new(String::new()).is_empty());
    }

    #[test]
    fn equal_values_are_equal_and_clones_too() {
        let secret = Secret::new("1234".to_owned());
        assert_eq!(secret, Secret::new("1234".to_owned()));
        assert_eq!(secret, secret.clone());
        assert_ne!(secret, Secret::new("1235".to_owned()));
        assert_ne!(secret, Secret::new("12345".to_owned()));
        assert_ne!(secret, Secret::new("123".to_owned()));
    }
}
