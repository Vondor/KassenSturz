// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Cash transactions the AI adds and removes (D-258).
//!
//! The user keeps the receipt and paid in cash, so the AI may write that
//! payment down: a transaction in one of the user's cash accounts with a
//! day, an amount and, if it knows, a payee, a note and a category. It adds
//! nothing to any other account, because those are filled from the bank.
//! Removing is as narrow: only a transaction that is in a cash account, was
//! entered by hand and has one category or none, so nothing a bank or file
//! delivered and no transfer can be removed. Both need their cell of the
//! grid, and the field locks of D-252 apply to what the AI writes.

use super::transactions::{free_text, parse_amount, parse_date};
use super::{Known, Object};
use kassensturz_core::ai_switch::{Field, Permissions};
use kassensturz_core::{
    Account, AccountId, AccountType, Date, Money, Split, Transaction, TransactionId,
};
use kassensturz_store::{Book, StoreError};
use serde_json::Value;

use crate::error::ToolError;

/// A payment in cash the AI writes down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewTransaction {
    /// The new transaction's ID, made when it was prepared.
    pub transaction: TransactionId,
    /// The cash account.
    pub account: AccountId,
    /// The cash account's name.
    pub account_name: String,
    /// The day.
    pub date: Date,
    /// Who was paid or paid, if given.
    pub payee: Option<String>,
    /// The note, if given.
    pub note: Option<String>,
    /// What it does to the account: negative is money out.
    pub amount: Money,
    /// The category it is booked to as a path, `None` for none yet.
    pub category: Option<Vec<String>>,
    /// The transaction as it will be.
    pub(super) built: Transaction,
}

/// A transaction in a cash account the AI removes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteTransaction {
    /// The transaction.
    pub transaction: TransactionId,
    /// Its day.
    pub date: Date,
    /// Its payee, if any.
    pub payee: Option<String>,
    /// The cash account.
    pub account: String,
    /// What it did to the account.
    pub amount: Money,
    /// The category it was booked to as a path, `None` for none.
    pub category: Option<Vec<String>>,
    /// The transaction as the user reviewed it: removing fails if it
    /// changed since.
    pub(super) reviewed: Transaction,
}

/// Whether `account` is an open cash account of the user.
fn is_open_cash(account: &Account) -> bool {
    account.kind.is_account() && !account.closed && account.account_type == Some(AccountType::Cash)
}

impl Known {
    /// Checks one item of `new_transactions` and builds the transaction,
    /// under the locks of `allowed`.
    pub(super) fn new_transaction(
        &self,
        book: &Book,
        value: &Value,
        allowed: Permissions,
    ) -> Result<NewTransaction, ToolError> {
        let item = Object::read(
            value,
            &[
                "account_id",
                "date",
                "amount",
                "payee",
                "note",
                "category_id",
            ],
            "new_transactions",
        )?;
        // What the AI writes is under the locks too: a transaction needs its
        // account, day and amount, so one of them locked leaves nothing to add.
        for (field, key) in [
            (Field::Account, "account_id"),
            (Field::Date, "date"),
            (Field::Amount, "amount"),
            (Field::Payee, "payee"),
            (Field::Note, "note"),
            (Field::Category, "category_id"),
        ] {
            if allowed.locked(field) && item.string(key).is_ok_and(|text| text.is_some()) {
                return Err(ToolError::FieldLocked {
                    field: field.name(),
                });
            }
        }
        for field in [Field::Account, Field::Date, Field::Amount] {
            if allowed.locked(field) {
                return Err(ToolError::FieldLocked {
                    field: field.name(),
                });
            }
        }
        let id = item.id("account_id")?.ok_or(ToolError::BadArgument {
            argument: "account_id",
        })?;
        let account = self
            .accounts
            .get(&id)
            .filter(|account| account.kind.is_account() && !account.closed)
            .ok_or(ToolError::UnknownAccount)?;
        if !is_open_cash(account) {
            return Err(ToolError::NotCashAccount);
        }
        let date = parse_date(
            item.string("date")?
                .ok_or(ToolError::BadArgument { argument: "date" })?,
        )?;
        let digits = book.currency_minor_digits(account.currency)?;
        let amount = parse_amount(
            item.string("amount")?
                .ok_or(ToolError::BadArgument { argument: "amount" })?,
            account.currency,
            digits,
        )?;
        let opposite = amount
            .checked_neg()
            .map_err(|_| ToolError::BadArgument { argument: "amount" })?;
        let payee = item
            .string("payee")?
            .map(|text| free_text(text, "payee"))
            .transpose()?
            .flatten();
        let note = item
            .string("note")?
            .map(|text| free_text(text, "note"))
            .transpose()?
            .flatten();
        let category = match item.id("category_id")? {
            None => None,
            Some(category_id) => {
                let category = self
                    .accounts
                    .get(&category_id)
                    .filter(|category| category.kind.is_category() && !category.closed)
                    .ok_or(ToolError::UnknownCategory)?;
                if category.currency != account.currency {
                    return Err(ToolError::OtherCurrency);
                }
                Some(category_id)
            }
        };
        let other = match category {
            Some(category) => Split::new(category, opposite),
            None => Split::uncategorized(opposite),
        };
        let transaction = TransactionId::generate();
        let built = Transaction::new(
            transaction,
            date,
            payee.clone(),
            vec![Split::new(id, amount), other],
        )
        .map_err(|_| ToolError::BadArgument { argument: "amount" })?
        .with_note(note.clone());
        Ok(NewTransaction {
            transaction,
            account: id,
            account_name: account.name.clone(),
            date,
            payee,
            note,
            amount,
            category: category.map(|category| self.path(category)),
            built,
        })
    }

    /// Checks one item of `delete_transactions`: a transaction that is in a
    /// cash account, was entered by hand and has one category or none.
    pub(super) fn delete_transaction(
        &self,
        book: &Book,
        value: &Value,
    ) -> Result<DeleteTransaction, ToolError> {
        let item = Object::read(value, &["transaction_id"], "delete_transactions")?;
        let id = item
            .uuid("transaction_id")?
            .map(TransactionId::from_bytes)
            .ok_or(ToolError::BadArgument {
                argument: "transaction_id",
            })?;
        let stored = match book.transaction(id) {
            Err(StoreError::UnknownTransaction(_)) => return Err(ToolError::UnknownTransaction),
            other => other?,
        };
        let account_of = |split: &Split| split.account.and_then(|id| self.accounts.get(&id));
        let [first, second] = stored.splits() else {
            return Err(ToolError::NotDeletable);
        };
        let ((own, own_account), (_, other)) = match (account_of(first), account_of(second)) {
            (Some(account), other) if account.kind.is_account() => {
                ((first, account), (second, other))
            }
            (other, Some(account)) if account.kind.is_account() => {
                ((second, account), (first, other))
            }
            _ => return Err(ToolError::NotDeletable),
        };
        // Not a transfer, not a starting balance, and nothing a bank or file
        // delivered or that was made from another currency.
        let other_ok = other.is_none_or(|account| account.kind.is_category());
        let by_hand = stored.splits().iter().all(|split| {
            split.imported.is_none() && split.foreign.is_none() && split.original.is_none()
        });
        if !is_open_cash(own_account) || !other_ok || !by_hand {
            return Err(ToolError::NotDeletable);
        }
        Ok(DeleteTransaction {
            transaction: id,
            date: stored.date_of(own),
            payee: stored.payee_of(own).map(str::to_owned),
            account: own_account.name.clone(),
            amount: own.account_amount(),
            category: other.map(|category| self.path(category.id)),
            reviewed: stored,
        })
    }
}
