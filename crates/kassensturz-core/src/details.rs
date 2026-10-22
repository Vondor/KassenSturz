// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a book keeps besides accounts, categories and transactions, so
//! that nothing a user typed into another program is lost when their book
//! comes over (D-132): the banks their accounts are at, who they pay and
//! receive money from, and the tags they mark parts with.

use crate::id::define_id;

define_id!(
    /// Identifies a bank or other institution within a book.
    InstitutionId
);

define_id!(
    /// Identifies a payee within a book: someone the user pays or receives
    /// money from, with what the user keeps about them.
    PayeeId
);

define_id!(
    /// Identifies a tag within a book.
    TagId
);

/// A bank or other institution the user's accounts are at (D-132). Every
/// detail but the name is optional text, kept as the user typed it; nothing
/// is checked, since none of it is used for a connection (D-053).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Institution {
    /// Its ID.
    pub id: InstitutionId,
    /// Its name. Never empty.
    pub name: String,
    /// Its bank code: a BIC, a sort code, a routing number, whatever the
    /// user gave.
    pub bank_code: Option<String>,
    /// Who the user talks to there, e.g. their advisor.
    pub contact: Option<String>,
    /// Its postal address, as lines of text.
    pub address: Option<String>,
    /// Its phone number.
    pub phone: Option<String>,
}

/// Someone the user pays or receives money from, with what the user keeps
/// about them (D-132). A transaction names its payee by name
/// ([`Transaction::payee`](crate::Transaction::payee)); this is the entry
/// behind that name, where the book has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payee {
    /// Its ID.
    pub id: PayeeId,
    /// The name, as transactions name it. Never empty, and no two payees of
    /// a book share one, ignoring capitals and spaces around it.
    pub name: String,
    /// Their postal address, as lines of text.
    pub address: Option<String>,
    /// Their e-mail address, as text.
    pub email: Option<String>,
    /// Their phone number.
    pub phone: Option<String>,
    /// The user's number with them, e.g. a customer or membership number.
    pub reference: Option<String>,
    /// How an imported entry is recognized as theirs, as the other program
    /// did it. A Rule is the way KassenSturz recognizes payees (D-014);
    /// nothing uses this yet beyond keeping it.
    pub recognition: PayeeRecognition,
    /// Whether that recognition ignores capitals.
    pub recognition_ignores_case: bool,
}

/// How another program recognized a payee in an imported entry's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PayeeRecognition {
    /// Not at all.
    Never,
    /// By their name.
    ByName,
    /// By any of these key words. Never empty.
    ByKeyWords(Vec<String>),
}

/// A tag the user marks parts with, e.g. "Holiday 2026", to find them
/// across categories (D-132).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    /// Its ID.
    pub id: TagId,
    /// Its name. Never empty, and no two tags of a book share one, ignoring
    /// capitals and spaces around it.
    pub name: String,
}
