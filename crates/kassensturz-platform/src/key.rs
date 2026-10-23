// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The name a secret is stored under.

use std::fmt;

use crate::error::KeychainError;

/// The name a secret is stored under: what kind of secret it is and which
/// one, e.g. the banking PIN of one bank login.
///
/// Both parts are short ASCII words: letters, digits and `-`, `_`, `.`,
/// `:`, at most 128 characters each, so a key is the same on every desktop
/// and keychain. A key is not secret, but it is written to the keychain's
/// own store, so it never holds a PIN, a login name or anything else from
/// `docs/THREAT_MODEL.md`'s list: use KassenSturz's own ID of the login
/// (D-067), never the bank's.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SecretKey {
    kind: String,
    id: String,
}

/// The most characters each part of a key may have.
const MAX_PART_LEN: usize = 128;

impl SecretKey {
    /// A key for the secret of kind `kind` (e.g. `bank-pin`) that belongs
    /// to `id` (e.g. a login's ID).
    pub fn new(kind: &str, id: &str) -> Result<Self, KeychainError> {
        if !is_part(kind) || !is_part(id) {
            return Err(KeychainError::InvalidKey);
        }
        Ok(Self {
            kind: kind.to_owned(),
            id: id.to_owned(),
        })
    }

    /// What kind of secret this is.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Which one it belongs to.
    pub fn id(&self) -> &str {
        &self.id
    }
}

fn is_part(part: &str) -> bool {
    !part.is_empty()
        && part.len() <= MAX_PART_LEN
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
}

impl fmt::Display for SecretKey {
    /// `kind/id`, the name the keychain shows next to the app's.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.kind, self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_kind_and_id() {
        let key = SecretKey::new("bank-pin", "0192f3a0-1c2d-7e00-8000-000000000001").unwrap();
        assert_eq!(key.kind(), "bank-pin");
        assert_eq!(key.id(), "0192f3a0-1c2d-7e00-8000-000000000001");
        assert_eq!(
            key.to_string(),
            "bank-pin/0192f3a0-1c2d-7e00-8000-000000000001"
        );
    }

    #[test]
    fn parts_are_short_ascii_words() {
        for (kind, id) in [
            ("", "x"),
            ("x", ""),
            ("bank pin", "x"),
            ("bank/pin", "x"),
            ("x", "ä"),
            ("x", "a\nb"),
            ("x", &"a".repeat(MAX_PART_LEN + 1)),
        ] {
            assert_eq!(
                SecretKey::new(kind, id),
                Err(KeychainError::InvalidKey),
                "{kind:?}/{id:?}"
            );
        }
        assert!(SecretKey::new("x", &"a".repeat(MAX_PART_LEN)).is_ok());
        assert!(SecretKey::new("A-z_0.9:", "x").is_ok());
    }
}
