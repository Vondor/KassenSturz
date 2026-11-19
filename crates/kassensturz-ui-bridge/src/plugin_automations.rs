// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Automation plugins after an import (#791 step 5, D-926): each one the
//! user added is told about the transactions just imported and may
//! suggest a category for those that have none. The user ticks the ones
//! to use, and they are applied as one change, which one Undo takes back.
//! Nothing a plugin answers changes the book by itself (AGENTS.md §8).
//!
//! The change is recorded as "change a transaction" in the history, the
//! word an edit of one transaction has: a word of its own would be new in
//! the book's history table, which is a change of the book format.

use std::collections::HashMap;
use std::path::Path;

use kassensturz_core::{AccountId, Report, Split, Transaction, TransactionId};
use kassensturz_plugin_api::ExtensionPoint;
use kassensturz_plugin_formats::{NewTransaction, PluginAutomation};
use kassensturz_store::{Action, Book, StoreError};

use cxx_qt_lib::QVariant;

use crate::errors::{ErrorText, report_text};
use crate::format::Locale;
use crate::plugins;
use crate::variant::{error_object, list, map, text};

/// An automation plugin the user added.
pub struct AutomationPlugin {
    /// The plugin's file name in the plugin folder.
    pub file: String,
    /// The plugin's name, shown as plain text.
    pub name: String,
    /// The loaded plugin.
    pub automation: PluginAutomation,
}

/// The automation plugins in the plugin folder, sorted by name.
pub fn automation_plugins(ai_enabled: bool) -> Vec<AutomationPlugin> {
    plugins::folder()
        .map(|folder| automation_plugins_in(&folder, ai_enabled))
        .unwrap_or_default()
}

/// The automation plugins in `folder`; one without the permissions it
/// needs is left out.
pub fn automation_plugins_in(folder: &Path, ai_enabled: bool) -> Vec<AutomationPlugin> {
    plugins::loaded(folder, ExtensionPoint::Automation, ai_enabled)
        .into_iter()
        .filter_map(|(file, plugin)| {
            let name = plugin.manifest().name.clone();
            PluginAutomation::new(plugin)
                .ok()
                .map(|automation| AutomationPlugin {
                    file,
                    name,
                    automation,
                })
        })
        .collect()
}

/// The book's categories by the name a plugin sees: the path from the
/// top-level category down, joined with `/`, as the exporter writes it.
pub fn category_names(book: &Book) -> Result<Vec<(String, AccountId)>, StoreError> {
    Ok(book
        .categories()?
        .into_iter()
        .map(|entry| (entry.path.join("/"), entry.category.id))
        .collect())
}

/// The part of `transaction` that has no category yet, if it has exactly
/// one such part and one in an account, as an imported entry has.
fn open_part(transaction: &Transaction) -> Option<(&Split, &Split)> {
    let [first, second] = transaction.splits() else {
        return None;
    };
    match (first.account, second.account) {
        (Some(_), None) => Some((first, second)),
        (None, Some(_)) => Some((second, first)),
        _ => None,
    }
}

/// What a plugin is told about the transactions with `ids`.
pub fn new_transactions(
    book: &Book,
    ids: &[TransactionId],
) -> Result<Vec<NewTransaction>, StoreError> {
    let mut names: HashMap<AccountId, String> = HashMap::new();
    let mut digits = HashMap::new();
    let mut found = Vec::with_capacity(ids.len());
    for &id in ids {
        let transaction = match book.transaction(id) {
            Ok(transaction) => transaction,
            // Undone or removed since: nothing to suggest for it.
            Err(StoreError::UnknownTransaction(_)) => continue,
            Err(error) => return Err(error),
        };
        let open = open_part(&transaction);
        let side = open.map(|(account, _)| account).or_else(|| {
            transaction
                .splits()
                .iter()
                .find(|split| split.account.is_some())
        });
        let Some(side) = side else { continue };
        let Some(account) = side.account else {
            continue;
        };
        if let std::collections::hash_map::Entry::Vacant(entry) = names.entry(account) {
            entry.insert(book.account(account)?.name);
        }
        let currency = side.amount.currency();
        let minor_digits = match digits.get(&currency) {
            Some(digits) => *digits,
            None => {
                let found = book.currency_minor_digits(currency)?;
                digits.insert(currency, found);
                found
            }
        };
        let memo = side
            .imported
            .as_ref()
            .and_then(|details| details.remittance_information.clone());
        found.push(NewTransaction {
            id: id.to_string(),
            date: transaction.booking_date(),
            account: names[&account].clone(),
            payee: transaction.payee_of(side).map(str::to_owned),
            memo,
            amount: side.amount,
            minor_digits,
            uncategorized: open.is_some(),
        });
    }
    Ok(found)
}

/// Why suggestions couldn't be used.
#[derive(Debug)]
pub enum UseError {
    /// A transaction changed or got a category since, or a category is
    /// gone: nothing was changed.
    OutOfDate,
    /// The book refused.
    Store(StoreError),
}

impl From<StoreError> for UseError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// Gives each transaction its category, all as one change that one Undo
/// takes back, or none if any of them has a category by now.
pub fn use_suggestions(
    book: &mut Book,
    chosen: &[(TransactionId, AccountId)],
) -> Result<usize, UseError> {
    if chosen.is_empty() {
        return Ok(0);
    }
    book.as_one_action(Action::EditTransaction, |book| {
        for &(id, category) in chosen {
            let before = match book.transaction(id) {
                Err(StoreError::UnknownTransaction(_)) => return Err(UseError::OutOfDate),
                other => other?,
            };
            let Some((_, open)) = open_part(&before) else {
                return Err(UseError::OutOfDate);
            };
            let open = open.id;
            let splits = before
                .splits()
                .iter()
                .map(|split| {
                    if split.id == open {
                        Split {
                            account: Some(category),
                            foreign: None,
                            ..split.clone()
                        }
                    } else {
                        split.clone()
                    }
                })
                .collect();
            let after = Transaction::new(
                before.id(),
                before.booking_date(),
                before.payee().map(str::to_owned),
                splits,
            )
            .map_err(|_| UseError::OutOfDate)?
            .with_details_of(&before);
            book.update_transaction(&after)?;
        }
        Ok(chosen.len())
    })
}

/// What each automation plugin suggests for the transactions with `ids`,
/// as QML gets it: a list of `{ file, plugin, items }` or `{ file, plugin,
/// error }`, leaving out plugins that suggest nothing. Each item is
/// `{ transaction, category, payee, amount, date, categoryName }`, all
/// text written for `locale`; `category` is the category's ID.
pub fn suggestions_object(
    book: &Book,
    plugins: &[AutomationPlugin],
    ids: &[TransactionId],
    locale: &impl Locale,
) -> QVariant {
    let fail = |error: StoreError| error_object(&report_text(book, &error.report(), locale));
    let transactions = match new_transactions(book, ids) {
        Ok(transactions) => transactions,
        Err(error) => return list([fail(error)]),
    };
    if !transactions
        .iter()
        .any(|transaction| transaction.uncategorized)
    {
        return list([]);
    }
    let categories = match category_names(book) {
        Ok(categories) => categories,
        Err(error) => return list([fail(error)]),
    };
    let names: Vec<String> = categories.iter().map(|(name, _)| name.clone()).collect();
    list(plugins.iter().filter_map(|plugin| {
        let head = [("file", text(&plugin.file)), ("plugin", text(&plugin.name))];
        let suggestions = match plugin.automation.after_import(&transactions, &names) {
            Ok(suggestions) => suggestions,
            Err(error) => {
                let failed = report_text(book, &error.report(), locale);
                let mut entries: Vec<(&str, QVariant)> = head.into_iter().collect();
                entries.push(("error", error_object(&failed)));
                return Some(map(entries));
            }
        };
        if suggestions.is_empty() {
            return None;
        }
        let items = list(suggestions.iter().filter_map(|suggestion| {
            let transaction = transactions
                .iter()
                .find(|transaction| transaction.id == suggestion.transaction)?;
            let (_, category) = categories
                .iter()
                .find(|(name, _)| *name == suggestion.category)?;
            let amount = locale.amount_format(transaction.amount.currency()).format(
                transaction.amount,
                transaction.minor_digits,
                |value| locale.group_digits(value),
            );
            Some(map([
                ("transaction", text(&transaction.id)),
                ("category", text(&category.to_string())),
                ("categoryName", text(&suggestion.category)),
                (
                    "payee",
                    text(transaction.payee.as_deref().unwrap_or_default()),
                ),
                ("amount", text(&amount)),
                ("date", text(&locale.short_date(transaction.date))),
            ]))
        }));
        let mut entries: Vec<(&str, QVariant)> = head.into_iter().collect();
        entries.push(("items", items));
        Some(map(entries))
    }))
}

/// [`use_suggestions`] with the IDs as QML hands them over, each among
/// `new` (the transactions the import added) and the book's categories:
/// an error object, or `{ count }`.
pub fn use_object(
    book: &mut Book,
    new: &[TransactionId],
    chosen: &[(String, String)],
    locale: &impl Locale,
) -> QVariant {
    let categories = match category_names(book) {
        Ok(categories) => categories,
        Err(error) => return error_object(&report_text(book, &error.report(), locale)),
    };
    let parsed: Option<Vec<(TransactionId, AccountId)>> = chosen
        .iter()
        .map(|(transaction, category)| {
            let transaction = new.iter().find(|id| id.to_string() == *transaction)?;
            let (_, category) = categories
                .iter()
                .find(|(_, id)| id.to_string() == *category)?;
            Some((*transaction, *category))
        })
        .collect();
    let Some(parsed) = parsed else {
        return error_object(&ErrorText::plain("input.suggestions-out-of-date"));
    };
    match use_suggestions(book, &parsed) {
        Ok(count) => map([(
            "count",
            QVariant::from(&i32::try_from(count).unwrap_or(i32::MAX)),
        )]),
        Err(UseError::OutOfDate) => {
            error_object(&ErrorText::plain("input.suggestions-out-of-date"))
        }
        Err(UseError::Store(error)) => error_object(&report_text(book, &error.report(), locale)),
    }
}
