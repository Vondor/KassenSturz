// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A plugin with the `exporter` extension point, behind [`Exporter`].

use std::io::Write;
use std::sync::{Mutex, PoisonError};

use kassensturz_core::{Money, Report};
use kassensturz_export::{
    AccountTransactions, Counterpart, ExportData, ExportError, ExportKind, Exporter, ExporterInfo,
    TransactionRow,
};
use kassensturz_plugin_api::ExtensionPoint;
use kassensturz_plugin_host::{HostError, Plugin};
use serde_json::{Value, json};

use crate::intern;
use crate::read::{self, Fields, Invalid};

/// The call that names the file name extension the exporter writes:
/// empty input, `{"extension": "csv"}` out.
pub const EXPORTER_INFO: &str = "ks_exporter_info";
/// The call that writes the file: the transactions as JSON in, the file's
/// bytes out.
pub const EXPORTER_WRITE: &str = "ks_exporter_write";

/// A plugin exporter. Its ID is the plugin's ID. It writes the
/// transactions of accounts only; a whole book is the built-in exporter's
/// job, which keeps everything a book holds (D-046).
pub struct PluginExporter {
    plugin: Mutex<Plugin>,
    info: ExporterInfo,
}

impl std::fmt::Debug for PluginExporter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginExporter")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

impl PluginExporter {
    /// Makes an exporter of a loaded plugin and asks it which extension it
    /// writes. A plugin without the `exporter` extension point lacks
    /// [`EXPORTER_INFO`] as far as KassenSturz is concerned.
    pub fn new(mut plugin: Plugin) -> Result<Self, HostError> {
        if !plugin
            .manifest()
            .extension_points
            .contains(&ExtensionPoint::Exporter)
        {
            return Err(HostError::ExportMissing {
                name: EXPORTER_INFO.to_owned(),
            });
        }
        let answer = plugin.call(EXPORTER_INFO, &[])?;
        let extension = extension(&answer).map_err(|Invalid| HostError::OutputInvalid)?;
        let info = ExporterInfo {
            id: intern::text(plugin.manifest().id.as_str()),
            kind: ExportKind::Transactions,
            extension,
        };
        Ok(Self {
            plugin: Mutex::new(plugin),
            info,
        })
    }
}

impl Exporter for PluginExporter {
    fn info(&self) -> ExporterInfo {
        self.info
    }

    /// Hands the transactions to the plugin and writes the bytes it
    /// returns, unchanged. KassenSturz writes the file, so the plugin never
    /// sees a path (D-848).
    fn write(&self, data: &ExportData, out: &mut dyn Write) -> Result<(), ExportError> {
        let ExportData::Transactions(accounts) = data else {
            return Err(ExportError::WrongKind);
        };
        let input = input(accounts).to_string();
        let bytes = self
            .plugin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .call(EXPORTER_WRITE, input.as_bytes())
            .map_err(|error| ExportError::Plugin(error.report()))?;
        out.write_all(&bytes)?;
        Ok(())
    }
}

fn extension(answer: &[u8]) -> Result<&'static str, Invalid> {
    let object = read::object(answer)?;
    let fields = Fields::of_map(&object, &["extension"])?;
    Ok(intern::text(read::extension(fields.text("extension")?)?))
}

/// What the plugin gets: every key is there, `null` where there is nothing.
pub(crate) fn input(accounts: &[AccountTransactions]) -> Value {
    json!({
        "accounts": accounts.iter().map(|account| json!({
            "name": account.account,
            "currency": account.currency.as_str(),
            "minor_digits": account.minor_digits,
            "rows": account.rows.iter().map(row).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn row(row: &TransactionRow) -> Value {
    json!({
        "date": row.date.to_string(),
        "payee": row.payee,
        "counterpart": counterpart(&row.counterpart),
        "memo": row.memo,
        "note": row.note,
        "amount": amount(row.amount),
        "original": row.original.as_ref().map(|original| json!({
            "amount": amount(original.amount),
            "currency": original.amount.currency().as_str(),
            "minor_digits": original.minor_digits,
        })),
    })
}

fn counterpart(counterpart: &Counterpart) -> Value {
    match counterpart {
        Counterpart::Uncategorized => json!({ "kind": "uncategorized" }),
        Counterpart::Category(name) => json!({ "kind": "category", "name": name }),
        Counterpart::Transfer(account) => json!({ "kind": "transfer", "account": account }),
        Counterpart::Parts(count) => json!({ "kind": "parts", "count": count }),
    }
}

/// Minor units as text, never a JSON number (AGENTS.md §3.1 rule 1).
fn amount(money: Money) -> String {
    money.minor_units().to_string()
}
