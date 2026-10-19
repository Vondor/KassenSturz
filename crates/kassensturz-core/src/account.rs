// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Accounts and categories.

use crate::currency::CurrencyCode;
use crate::id::define_id;

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

/// The sort of account the user sees: a checking account, cash, a credit
/// card. It tells later steps what an account is for, e.g. a "make it a
/// Transfer" Rule into a cash account (D-031) or a bank's credit card
/// statements (D-029). Categories have none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccountType {
    /// A checking account (German: Girokonto).
    Checking,
    /// A savings account.
    Savings,
    /// Cash, e.g. the money in a wallet.
    Cash,
    /// A credit card. Its balance is what the user owes (D-067).
    CreditCard,
}

impl AccountType {
    /// Every type, in the order the UI lists them.
    pub const ALL: [Self; 4] = [Self::Checking, Self::Savings, Self::Cash, Self::CreditCard];

    /// The kind of account this type is: money the user has, or money the user
    /// owes.
    pub const fn kind(self) -> AccountKind {
        match self {
            Self::Checking | Self::Savings | Self::Cash => AccountKind::Asset,
            Self::CreditCard => AccountKind::Liability,
        }
    }
}

impl AccountKind {
    /// Whether users see this kind as an account, as opposed to a category or
    /// an internal account.
    pub const fn is_account(self) -> bool {
        matches!(self, Self::Asset | Self::Liability)
    }
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
    /// What sort of account it is, if the user said. Only accounts have one
    /// (see [`AccountType::kind`]), and old books have none.
    pub account_type: Option<AccountType>,
    /// Whether the user closed it. A closed account keeps its history but
    /// takes no new transactions and is hidden from the lists.
    pub closed: bool,
}
