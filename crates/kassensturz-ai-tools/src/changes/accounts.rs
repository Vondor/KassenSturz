// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The change kinds of step 4 (D-253 to D-256): deleting an empty category,
//! adding an account, and renaming, closing or reopening one. They are
//! checked here, shown before and after like the others, and checked once
//! more when the user confirms.

use std::collections::HashSet;

use kassensturz_core::{Account, AccountId, AccountType, CurrencyCode, Money};
use kassensturz_store::{Book, StoreError};
use serde_json::Value;

use super::{Known, Object, plain_text};
use crate::error::ToolError;

/// An empty category deleted (D-253).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteCategory {
    /// The category.
    pub id: AccountId,
    /// Its path: the names above it first, its own last.
    pub path: Vec<String>,
    /// The category as the user reviewed it: applying fails if it changed
    /// since.
    pub(super) reviewed: Account,
}

/// A new account (D-254). Never with a number or an IBAN, and no starting
/// balance: the user adds those.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewAccount {
    /// The ID it will have.
    pub id: AccountId,
    /// Its name.
    pub name: String,
    /// What sort of account it is.
    pub account_type: AccountType,
    /// Its currency.
    pub currency: CurrencyCode,
}

/// An account renamed, closed or opened again (D-255). Accounts are never
/// deleted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditAccount {
    /// The account.
    pub id: AccountId,
    /// Its name before.
    pub before: String,
    /// Its name after.
    pub after: String,
    /// Whether it was closed before.
    pub was_closed: bool,
    /// Whether it is closed after.
    pub closed: bool,
    /// Its balance, which closing needs to be zero.
    pub balance: Money,
    /// The account as the user reviewed it: applying fails if it changed
    /// since.
    pub(super) reviewed: Account,
}

impl EditAccount {
    /// Whether the account is renamed.
    pub fn renames(&self) -> bool {
        self.before != self.after
    }
}

/// The type a model names, as `list_accounts` writes it.
fn account_type(text: &str) -> Option<AccountType> {
    match text {
        "checking" => Some(AccountType::Checking),
        "savings" => Some(AccountType::Savings),
        "cash" => Some(AccountType::Cash),
        "credit_card" => Some(AccountType::CreditCard),
        _ => None,
    }
}

impl Known {
    /// Whether another account has this name (accounts share one list,
    /// ignoring capitals, closed ones included).
    fn account_name_taken(&self, name: &str, except: Option<AccountId>) -> bool {
        let wanted = name.trim().to_lowercase();
        self.accounts.values().any(|other| {
            other.kind.is_account()
                && Some(other.id) != except
                && other.name.trim().to_lowercase() == wanted
        })
    }

    /// An empty category of the book, to delete.
    pub(super) fn delete_category(
        &mut self,
        book: &Book,
        value: &Value,
        gone: &mut HashSet<AccountId>,
    ) -> Result<DeleteCategory, ToolError> {
        let item = Object::read(value, &["category_id"], "delete_categories")?;
        let id = item.id("category_id")?.ok_or(ToolError::BadArgument {
            argument: "category_id",
        })?;
        let reviewed = self
            .accounts
            .get(&id)
            .filter(|category| category.kind.is_category())
            .ok_or(ToolError::UnknownCategory)?
            .clone();
        // A category made by this set isn't in the book to delete.
        if self.keys.values().any(|&new| new == id) || !gone.insert(id) {
            return Err(ToolError::UnknownCategory);
        }
        if let Some(used_by) = book.category_use(id)? {
            return Err(ToolError::CategoryInUse { used_by });
        }
        Ok(DeleteCategory {
            path: self.original_paths.get(&id).cloned().unwrap_or_default(),
            id,
            reviewed,
        })
    }

    /// A new account.
    pub(super) fn new_account(&mut self, value: &Value) -> Result<NewAccount, ToolError> {
        let item = Object::read(value, &["name", "type", "currency"], "new_accounts")?;
        let name = plain_text(item.required_string("name")?, "name")?;
        let account_type = account_type(item.required_string("type")?)
            .ok_or(ToolError::BadArgument { argument: "type" })?;
        let currency = match item.string("currency")? {
            Some(code) => CurrencyCode::new(code)
                .ok()
                .filter(|currency| {
                    currency.iso_minor_digits().is_some() && currency.withdrawn().is_none()
                })
                .ok_or(ToolError::BadArgument {
                    argument: "currency",
                })?,
            None => self.usual_currency.ok_or(ToolError::BadArgument {
                argument: "currency",
            })?,
        };
        if self.account_name_taken(&name, None) {
            return Err(ToolError::NameTaken);
        }
        let id = AccountId::generate();
        self.accounts.insert(
            id,
            Account {
                id,
                name: name.clone(),
                kind: account_type.kind(),
                currency,
                account_type: Some(account_type),
                parent: None,
                closed: false,
                number: None,
                notes: None,
                institution: None,
            },
        );
        Ok(NewAccount {
            id,
            name,
            account_type,
            currency,
        })
    }

    /// Renames, closes or reopens an account of the book.
    pub(super) fn edit_account(
        &mut self,
        book: &Book,
        value: &Value,
        edited: &mut HashSet<AccountId>,
    ) -> Result<EditAccount, ToolError> {
        let item = Object::read(value, &["account_id", "name", "closed"], "edit_accounts")?;
        let id = item.id("account_id")?.ok_or(ToolError::BadArgument {
            argument: "account_id",
        })?;
        let reviewed = self
            .accounts
            .get(&id)
            .filter(|account| account.kind.is_account())
            .ok_or(ToolError::UnknownAccount)?
            .clone();
        // An account made by this set isn't in the book to edit, and one
        // account is edited once.
        if !edited.insert(id) || self.keys.values().any(|&new| new == id) {
            return Err(ToolError::BadArgument {
                argument: "account_id",
            });
        }
        let name = item
            .string("name")?
            .map(|name| plain_text(name, "name"))
            .transpose()?
            .filter(|name| *name != reviewed.name);
        let closed = item
            .boolean("closed")?
            .filter(|closed| *closed != reviewed.closed);
        if name.is_none() && closed.is_none() {
            return Err(ToolError::BadArgument {
                argument: "account_id",
            });
        }
        if let Some(name) = &name
            && self.account_name_taken(name, Some(id))
        {
            return Err(ToolError::NameTaken);
        }
        let balance = book.balance(id)?;
        // Closing needs a zero balance, so no money is hidden (D-255).
        if closed == Some(true) && balance.minor_units() != 0 {
            return Err(ToolError::AccountNotEmpty);
        }
        let after = Account {
            name: name.clone().unwrap_or_else(|| reviewed.name.clone()),
            closed: closed.unwrap_or(reviewed.closed),
            ..reviewed.clone()
        };
        let edit = EditAccount {
            id,
            before: reviewed.name.clone(),
            after: after.name.clone(),
            was_closed: reviewed.closed,
            closed: after.closed,
            balance,
            reviewed,
        };
        self.accounts.insert(id, after);
        Ok(edit)
    }
}

/// Deletes the category, if it is still as reviewed and still unused.
pub(super) fn apply_delete_category(
    book: &mut Book,
    delete: &DeleteCategory,
) -> Result<(), super::ChangeError> {
    match book.account(delete.id) {
        Err(StoreError::UnknownAccount(_)) => return Err(super::ChangeError::OutOfDate),
        other => {
            if other? != delete.reviewed {
                return Err(super::ChangeError::OutOfDate);
            }
        }
    }
    book.remove_category(delete.id)?;
    Ok(())
}

/// Adds the account.
pub(super) fn apply_new_account(
    book: &mut Book,
    new: &NewAccount,
) -> Result<(), super::ChangeError> {
    book.use_currency(new.currency)?;
    book.add_account(&Account {
        id: new.id,
        name: new.name.clone(),
        kind: new.account_type.kind(),
        currency: new.currency,
        account_type: Some(new.account_type),
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    })?;
    Ok(())
}

/// Renames, closes or reopens the account, if it is still as reviewed.
pub(super) fn apply_edit_account(
    book: &mut Book,
    edit: &EditAccount,
) -> Result<(), super::ChangeError> {
    match book.account(edit.id) {
        Err(StoreError::UnknownAccount(_)) => return Err(super::ChangeError::OutOfDate),
        other => {
            if other? != edit.reviewed {
                return Err(super::ChangeError::OutOfDate);
            }
        }
    }
    if edit.renames() {
        book.rename_account(edit.id, &edit.after)?;
    }
    if edit.closed != edit.was_closed {
        if edit.closed {
            book.close_account(edit.id)?;
        } else {
            book.reopen_account(edit.id)?;
        }
    }
    Ok(())
}
