// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The hand-checked bank list: corrections to the external bank information
//! (AqBanking's), checked by a person in a test round (item 93 of the
//! Proposals list, D-146). How to add an entry is in `docs/BANK_LIST.md`.
//!
//! An entry here wins over the external source entirely, so write the whole
//! entry, not just the field that was wrong. Nothing here is a secret: a
//! bank's server address and login kinds are the same for every customer.
//! Never write a login name, account number, PIN or TAN here.

use crate::directory::{BankInfo, FintsServer, LoginKind};
use crate::iban::BankCode;

/// One bank in the hand-checked list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BankEntry {
    /// The country, two capital letters, e.g. `DE`.
    pub country: &'static str,
    /// The bank code within the country, e.g. the eight digits of a German
    /// bank code.
    pub code: &'static str,
    /// The bank's name as its customers know it.
    pub name: &'static str,
    /// The bank's BIC, if known.
    pub bic: Option<&'static str>,
    /// The FinTS server's address, `https://…`; `None` when the bank has
    /// none, which the app shows as "no server" rather than guessing.
    pub server: Option<&'static str>,
    /// The ways to log in the bank offers; `None` when not checked yet,
    /// which the app shows as an unknown login type.
    pub login_kinds: Option<&'static [LoginKind]>,
    /// When and how the entry was checked, e.g. `2026-11-05, the
    /// maintainer's test round 1`: a person confirmed that a login at this
    /// bank worked with these settings.
    pub checked: &'static str,
}

impl BankEntry {
    /// The entry as the directory hands it out.
    pub fn info(&self) -> BankInfo {
        BankInfo {
            code: BankCode::new(self.country, self.code),
            name: self.name.to_owned(),
            bic: self.bic.map(str::to_owned),
            server: self.server.map(|url| FintsServer {
                url: url.to_owned(),
                login_kinds: self.login_kinds.map(<[LoginKind]>::to_vec),
            }),
        }
    }
}

/// The list. Empty until the first test round with a real bank (Phase 2 of
/// M4): every entry comes from a person who confirmed it, never from an
/// agent's memory (`docs/BANK_LIST.md`).
pub const HAND_CHECKED: &[BankEntry] = &[];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Guards the entries agents add later: the format `docs/BANK_LIST.md`
    /// describes, and nothing that could be a person's data.
    #[test]
    fn every_entry_is_well_formed_and_unique() {
        let mut codes = HashSet::new();
        for entry in HAND_CHECKED {
            assert!(
                entry.country.len() == 2 && entry.country.bytes().all(|b| b.is_ascii_uppercase()),
                "country of {}: {}",
                entry.name,
                entry.country
            );
            assert!(
                !entry.code.is_empty() && entry.code.bytes().all(|b| b.is_ascii_alphanumeric()),
                "code of {}: {}",
                entry.name,
                entry.code
            );
            if entry.country == "DE" {
                assert!(
                    entry.code.len() == 8 && entry.code.bytes().all(|b| b.is_ascii_digit()),
                    "a German bank code has eight digits: {}",
                    entry.code
                );
            }
            assert!(!entry.name.trim().is_empty(), "name of {}", entry.code);
            if let Some(bic) = entry.bic {
                assert!(
                    (bic.len() == 8 || bic.len() == 11)
                        && bic.bytes().all(|b| b.is_ascii_alphanumeric()),
                    "BIC of {}: {bic}",
                    entry.name
                );
            }
            if let Some(server) = entry.server {
                assert!(
                    server.starts_with("https://"),
                    "server of {}: {server} (FinTS runs over TLS, T-12)",
                    entry.name
                );
            }
            if let Some(login_kinds) = entry.login_kinds {
                assert!(!login_kinds.is_empty(), "login kinds of {}", entry.name);
            }
            assert!(
                !entry.checked.trim().is_empty(),
                "who checked {}?",
                entry.name
            );
            assert!(
                codes.insert((entry.country, entry.code)),
                "{} {} is listed twice",
                entry.country,
                entry.code
            );
        }
    }
}
