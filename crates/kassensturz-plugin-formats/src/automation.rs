// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A plugin with the `automation` extension point (#791 step 5, D-926):
//! after an import it gets the new transactions and the book's category
//! names, and may suggest a category for transactions that have none.
//! It only suggests: KassenSturz shows the suggestions for the user to
//! tick, and applies the ticked ones as one change with one Undo
//! (AGENTS.md §8, as for the AI's change sets).

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

use kassensturz_core::{Date, Money};
use kassensturz_plugin_api::{ExtensionPoint, Permission};
use kassensturz_plugin_host::{HostError, Plugin};
use serde_json::{Value, json};

use crate::read::{self, Fields, Invalid};

/// The call after an import: the new transactions and the category names
/// in, `{"suggestions": [{"transaction": id, "category": name}]}` out.
pub const AUTOMATION_AFTER_IMPORT: &str = "ks_automation_after_import";

/// A transaction the plugin is told about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTransaction {
    /// Its ID, which a suggestion names.
    pub id: String,
    /// Its date.
    pub date: Date,
    /// The account's name.
    pub account: String,
    /// The other party, as the bank gave it.
    pub payee: Option<String>,
    /// The bank's text about it.
    pub memo: Option<String>,
    /// Its amount in the account's currency.
    pub amount: Money,
    /// The decimal places of that currency.
    pub minor_digits: u8,
    /// Whether it has no category yet; only those may get a suggestion.
    pub uncategorized: bool,
}

/// A suggested category for a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// The transaction's ID.
    pub transaction: String,
    /// The name of a category the book has.
    pub category: String,
}

/// A plugin automation. The plugin must ask for `read_book` and
/// `propose_changes` (D-848).
pub struct PluginAutomation {
    plugin: Mutex<Plugin>,
}

impl std::fmt::Debug for PluginAutomation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginAutomation")
            .finish_non_exhaustive()
    }
}

impl PluginAutomation {
    /// Makes an automation of a loaded plugin.
    pub fn new(plugin: Plugin) -> Result<Self, HostError> {
        let manifest = plugin.manifest();
        if !manifest
            .extension_points
            .contains(&ExtensionPoint::Automation)
        {
            return Err(HostError::ExportMissing {
                name: AUTOMATION_AFTER_IMPORT.to_owned(),
            });
        }
        for permission in [Permission::ReadBook, Permission::ProposeChanges] {
            if !manifest.permissions.contains(&permission) {
                return Err(HostError::PermissionMissing { permission });
            }
        }
        Ok(Self {
            plugin: Mutex::new(plugin),
        })
    }

    /// Asks for suggestions about `transactions`, with the book's
    /// `categories`. An answer that suggests a category the book doesn't
    /// have, names a transaction it wasn't given or one that already has a
    /// category, or names one twice, is refused whole.
    pub fn after_import(
        &self,
        transactions: &[NewTransaction],
        categories: &[String],
    ) -> Result<Vec<Suggestion>, HostError> {
        let input = input(transactions, categories);
        let answer = self
            .plugin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .call(AUTOMATION_AFTER_IMPORT, input.to_string().as_bytes())?;
        suggestions(&answer, transactions, categories).map_err(|Invalid| HostError::OutputInvalid)
    }
}

/// What the plugin gets: every key is there, `null` where there is nothing.
pub(crate) fn input(transactions: &[NewTransaction], categories: &[String]) -> Value {
    json!({
        "categories": categories,
        "transactions": transactions.iter().map(|transaction| json!({
            "id": transaction.id,
            "date": transaction.date.to_string(),
            "account": transaction.account,
            "payee": transaction.payee,
            "memo": transaction.memo,
            "amount": transaction.amount.minor_units().to_string(),
            "currency": transaction.amount.currency().as_str(),
            "minor_digits": transaction.minor_digits,
            "uncategorized": transaction.uncategorized,
        })).collect::<Vec<_>>(),
    })
}

fn suggestions(
    answer: &[u8],
    transactions: &[NewTransaction],
    categories: &[String],
) -> Result<Vec<Suggestion>, Invalid> {
    let object = read::object(answer)?;
    let fields = Fields::of_map(&object, &["suggestions"])?;
    let mut seen = BTreeSet::new();
    fields
        .array("suggestions")?
        .iter()
        .map(|item| {
            let fields = Fields::of(item, &["transaction", "category"])?;
            let id = fields.text("transaction")?;
            let category = fields.text("category")?;
            let open = transactions
                .iter()
                .any(|transaction| transaction.id == id && transaction.uncategorized);
            if !open || !categories.iter().any(|name| name == category) || !seen.insert(id) {
                return Err(Invalid);
            }
            Ok(Suggestion {
                transaction: id.to_owned(),
                category: category.to_owned(),
            })
        })
        .collect()
}
