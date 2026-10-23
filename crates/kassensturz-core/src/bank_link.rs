// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which bank account an account is filled from (Q-086, D-148).

use crate::Date;
use crate::account::AccountId;

/// The link between an account in the book and the bank account it is
/// filled from, with the day it was last fetched (Q-086, D-148).
///
/// It holds no login data: not the login name, not a PIN, nothing from the
/// keychain. The bank login lives in the backend's own folder on each
/// computer (D-139), so a book opened on another computer only needs the
/// login set up again there, and the links find their bank accounts again
/// by IBAN, or by bank code and account number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BankLink {
    /// The account in the book: one the user sees, never a category.
    pub account: AccountId,
    /// The bank account's IBAN, without spaces and in capitals, if it has
    /// one. A credit card usually has none.
    pub iban: Option<String>,
    /// The bank's code in its country, e.g. a German bank code, as the login
    /// knows it. Needed together with `number` when there is no IBAN; it
    /// may be given with an IBAN too.
    pub bank_code: Option<String>,
    /// The number the bank lists the account by when it has no IBAN, e.g. a
    /// card number as the bank shows it. Needed with `bank_code` when there
    /// is no IBAN.
    pub number: Option<String>,
    /// The day the account's statement was last fetched from the bank, by
    /// the clock of the computer that fetched it; `None` before the first
    /// fetch.
    pub last_fetched_on: Option<Date>,
}

impl BankLink {
    /// Whether the link names a bank account: an IBAN, or a bank code with
    /// an account number. Texts that are empty or only spaces count as
    /// missing; the IBAN is checked for its form, not its check digits,
    /// which `kassensturz-bank` does before a link is made.
    pub fn names_a_bank_account(&self) -> bool {
        let given = |text: &Option<String>| text.as_deref().is_some_and(|t| !t.trim().is_empty());
        let iban_in_form = self
            .iban
            .as_deref()
            .is_some_and(|iban| iban.chars().all(|c| c.is_ascii_alphanumeric()));
        (given(&self.iban) && iban_in_form) || (given(&self.bank_code) && given(&self.number))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(iban: Option<&str>, bank_code: Option<&str>, number: Option<&str>) -> BankLink {
        BankLink {
            account: AccountId::generate(),
            iban: iban.map(str::to_owned),
            bank_code: bank_code.map(str::to_owned),
            number: number.map(str::to_owned),
            last_fetched_on: None,
        }
    }

    #[test]
    fn a_link_needs_an_iban_or_a_bank_code_with_a_number() {
        assert!(link(Some("DE89370400440532013000"), None, None).names_a_bank_account());
        assert!(link(None, Some("37040044"), Some("4111111111111111")).names_a_bank_account());
        assert!(!link(None, None, None).names_a_bank_account());
        assert!(!link(None, Some("37040044"), None).names_a_bank_account());
        assert!(!link(None, None, Some("4111111111111111")).names_a_bank_account());
        assert!(!link(Some(" "), None, None).names_a_bank_account());
        assert!(!link(None, Some(""), Some("4111111111111111")).names_a_bank_account());
        // An IBAN is kept without spaces, so one with them isn't in form.
        assert!(!link(Some("DE89 3704 0044 0532 0130 00"), None, None).names_a_bank_account());
    }
}
