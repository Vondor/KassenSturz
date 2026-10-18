// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Accounts and categories.

use crate::id::define_id;
use crate::money::CurrencyCode;

define_id!(
    /// Identifies an account or a category within a book.
    AccountId
);

/// What an account stands for.
///
/// Users only ever see "accounts" and "categories" (`docs/GLOSSARY.md`); these
/// are the double-entry concepts behind them (AGENTS.md §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccountKind {
    /// Money the user has: a checking or savings account, cash. Shown as an
    /// account.
    Asset,
    /// Money the user owes, such as a credit card. Shown as an account whose
    /// balance reads "You owe" (D-067).
    Liability,
    /// Where money comes from, such as Salary. Shown as a category.
    Income,
    /// What money is spent on, such as Groceries. Shown as a category.
    Expense,
    /// The internal balancing account for starting balances. Never shown.
    Equity,
}

/// An account or a category.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// Its ID.
    pub id: AccountId,
    /// The name the user gave it, e.g. "Checking account" or "Groceries".
    pub name: String,
    /// What it stands for.
    pub kind: AccountKind,
    /// The currency its amounts are in.
    pub currency: CurrencyCode,
}
