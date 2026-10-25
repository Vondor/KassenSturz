// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The user's consent to what is sent to a provider on another computer
//! (D-035, D-168, D-182; threat T-22).
//!
//! Before anything goes to a provider on another computer, the user agrees
//! to a plain statement of what is sent. The statement's words are the
//! app's (translated, M5.7); this module holds what they must cover,
//! [`STATEMENT`], and its version, [`STATEMENT_VERSION`]. The consent
//! record names the provider's address, the version and the day, and it
//! covers a request only for that same address and the current version, so
//! a new address or a wider statement asks again. A provider on this
//! computer needs no consent: nothing sent to it leaves the machine
//! (D-175).

use jiff::civil::Date;

use crate::address::{Place, ProviderAddress};
use crate::config::{CONSENT_ADDRESS_KEY, CONSENT_DAY_KEY, CONSENT_STATEMENT_KEY};

/// What can be sent to a provider. The consent statement names each one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SentData {
    /// What the user types into the chat.
    Questions,
    /// What KassenSturz's read functions return from the open book:
    /// account and category names, balances, totals, transactions with
    /// their payees and memos, and Rules (D-170). Never a key, a PIN, a
    /// bank login or a bank link.
    BookData,
    /// The descriptions of the functions the model may call, and
    /// KassenSturz's own instructions to the model.
    Instructions,
    /// The provider's key, when it needs one (D-169).
    Key,
}

impl SentData {
    /// The word the app's statement uses to find its sentence for it.
    pub fn as_word(self) -> &'static str {
        match self {
            Self::Questions => "questions",
            Self::BookData => "book-data",
            Self::Instructions => "instructions",
            Self::Key => "key",
        }
    }
}

/// Everything the statement of what is sent covers.
pub const STATEMENT: [SentData; 4] = [
    SentData::Questions,
    SentData::BookData,
    SentData::Instructions,
    SentData::Key,
];

/// The version of [`STATEMENT`] and of the app's words for it. Raise it
/// whenever either covers more, so every user is asked again (D-168).
pub const STATEMENT_VERSION: u32 = 1;

/// A consent the user gave: to send to `address` what version `statement`
/// of the statement names, on `day`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsentRecord {
    /// The provider's address the user agreed to send to.
    pub address: ProviderAddress,
    /// The version of the statement the user agreed to.
    pub statement: u32,
    /// The day the user agreed.
    pub day: Date,
}

impl ConsentRecord {
    /// The consent the user gives now, to the current statement.
    pub fn give(address: &ProviderAddress, today: Date) -> Self {
        Self {
            address: address.clone(),
            statement: STATEMENT_VERSION,
            day: today,
        }
    }

    /// Whether it covers sending to `address`: the same address and the
    /// current statement.
    pub fn covers(&self, address: &ProviderAddress) -> bool {
        self.address == *address && self.statement == STATEMENT_VERSION
    }

    /// The entries the app writes to the `[AI]` group for it.
    pub fn entries(&self) -> [(&'static str, String); 3] {
        [
            (CONSENT_ADDRESS_KEY, self.address.as_str().to_owned()),
            (CONSENT_STATEMENT_KEY, self.statement.to_string()),
            (CONSENT_DAY_KEY, self.day.to_string()),
        ]
    }
}

/// Whether sending to `address` needs a consent that `consent` doesn't
/// give.
pub fn consent_missing(address: &ProviderAddress, consent: Option<&ConsentRecord>) -> bool {
    address.place() == Place::AnotherComputer && !consent.is_some_and(|c| c.covers(address))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AiConfig;

    fn address(text: &str) -> ProviderAddress {
        ProviderAddress::parse(text).unwrap()
    }

    #[test]
    fn the_statement_is_pinned_to_its_version() {
        // Changing what is sent changes this list: raise STATEMENT_VERSION
        // with it, so users are asked again, and then update this test.
        assert_eq!(STATEMENT_VERSION, 1);
        let words: Vec<&str> = STATEMENT.iter().map(|data| data.as_word()).collect();
        assert_eq!(words, ["questions", "book-data", "instructions", "key"]);
    }

    #[test]
    fn consent_covers_the_same_address_and_statement_only() {
        let cloud = address("https://api.example.com/v1");
        let day = jiff::civil::date(2026, 10, 25);
        let given = ConsentRecord::give(&cloud, day);
        assert!(given.covers(&cloud));
        assert!(given.covers(&address("https://api.example.com/v1/")));
        assert!(!given.covers(&address("https://api.example.com/v2")));
        assert!(!given.covers(&address("https://other.example.com/v1")));
        let older = ConsentRecord {
            statement: STATEMENT_VERSION - 1,
            ..given.clone()
        };
        assert!(!older.covers(&cloud));
    }

    #[test]
    fn only_another_computer_needs_consent() {
        let local = address("http://localhost:11434/v1");
        let home = address("https://192.168.1.20:11434/v1");
        assert!(!consent_missing(&local, None));
        assert!(consent_missing(&home, None));
        let given = ConsentRecord::give(&home, jiff::civil::date(2026, 10, 25));
        assert!(!consent_missing(&home, Some(&given)));
        assert!(consent_missing(
            &address("https://192.168.1.21:11434/v1"),
            Some(&given)
        ));
    }

    #[test]
    fn entries_read_back() {
        let given = ConsentRecord::give(
            &address("https://api.example.com/v1"),
            jiff::civil::date(2026, 10, 25),
        );
        let mut text = String::from("[AI]\n");
        for (key, value) in given.entries() {
            text.push_str(&format!("{key}={value}\n"));
        }
        assert_eq!(AiConfig::from_text(&text).consent(), Some(&given));
    }
}
