// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A bank statement as every statement format delivers it: CAMT.053, CSV,
//! MT940, OFX, and later bank connections (M4, D-030).

use kassensturz_core::{Currency, Date, Money};

/// One account's entries over a period, as a bank or a file delivered them.
///
/// The import pipeline books it into one account of the book (M2.2). It
/// holds what the file says, nothing derived from the book: which account it
/// is for, its entries and the balances the bank reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statement {
    /// The account the statement is for, as the file names it.
    pub account: StatementAccount,
    /// The currency of every amount in the statement, with the decimal
    /// places its amounts are counted in.
    ///
    /// For a current currency they come from KassenSturz's ISO 4217 table
    /// ([`crate::currency`]). The pipeline compares them with the decimal
    /// places the book stores for the currency (D-027), so an amount is never
    /// read with the decimal point in another place. A withdrawn currency,
    /// such as the Deutsche Mark in an old file, can be given with its own
    /// decimal places; the pipeline converts it into the currency that
    /// replaced it (D-077).
    pub currency: Currency,
    /// What the account held at the start of the statement, if the file
    /// says.
    pub opening_balance: Option<Balance>,
    /// What the account held at the end of the statement, if the file says.
    /// KassenSturz checks its own balance against it (D-029).
    pub closing_balance: Option<Balance>,
    /// The entries, in the order of the file.
    pub entries: Vec<Entry>,
}

/// How a statement names its account. Everything is text exactly as
/// delivered (D-067), not checked, so the user can compare it with what the
/// bank shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatementAccount {
    /// The account's IBAN, if the file gives one.
    pub iban: Option<String>,
    /// Any other number the file gives the account by, e.g. a German bank
    /// code and account number (`37040044/0532013000`) or an OFX account ID.
    pub number: Option<String>,
}

/// A balance the bank reported: how much was in the account at the end of a
/// day. Money the user owes, such as a credit card's debt, is negative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Balance {
    /// The amount, in the statement's currency.
    pub amount: Money,
    /// The day it is for.
    pub date: Date,
}

/// One entry of a statement: money that went into or out of the account.
///
/// Every field is kept exactly as the file delivered it (D-067): the import
/// pipeline stores them with the part booked to the account (D-083), next
/// to the payee and category the user edits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Whether the bank has booked it. Only booked entries are imported in
    /// v1 (D-030).
    pub status: EntryStatus,
    /// The bank's own ID for this entry, used to recognize an entry that is
    /// imported twice (D-030, Q-039). Many formats, such as CSV, have none.
    pub bank_id: Option<String>,
    /// The date the bank booked it, the one the user sees (D-030).
    ///
    /// Where a format may leave it out when it equals the value date, as
    /// MT940 does, the importer gives the value date here.
    pub booking_date: Date,
    /// The date from which the money counted for interest (German:
    /// Wertstellung, D-030), if the file says.
    pub value_date: Option<Date>,
    /// The amount, in the statement's currency. Positive amounts came into
    /// the account, negative ones went out of it, as in a book.
    pub amount: Money,
    /// The other party's name: who paid, or who was paid.
    pub other_party_name: Option<String>,
    /// The other party's IBAN, or whatever account number the file gives
    /// for them.
    pub other_party_iban: Option<String>,
    /// The text the payment came with (German: Verwendungszweck). Where a
    /// format splits it into several lines or fields, the importer joins
    /// them as the format says.
    pub remittance_information: Option<String>,
}

impl Entry {
    /// A booked entry of `amount` on `booking_date`, with nothing else
    /// known yet.
    pub fn booked(booking_date: Date, amount: Money) -> Self {
        Self {
            status: EntryStatus::Booked,
            bank_id: None,
            booking_date,
            value_date: None,
            amount,
            other_party_name: None,
            other_party_iban: None,
            remittance_information: None,
        }
    }

    /// Every text the entry holds, for checking them against the limits.
    pub(crate) fn texts(&self) -> impl Iterator<Item = &str> {
        [
            &self.bank_id,
            &self.other_party_name,
            &self.other_party_iban,
            &self.remittance_information,
        ]
        .into_iter()
        .filter_map(|text| text.as_deref())
    }
}

/// Whether the bank has booked an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryStatus {
    /// Booked: it counts in the account's balance and won't change anymore.
    Booked,
    /// Not booked yet, or only for information, e.g. a card payment the bank
    /// has only reserved. It may still change or vanish, so it isn't
    /// imported in v1 (D-030).
    Pending,
}
