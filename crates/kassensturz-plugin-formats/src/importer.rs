// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A plugin with the `importer` extension point, behind [`Importer`].

use std::sync::{Mutex, PoisonError};

use kassensturz_core::{Currency, Money, Report};
use kassensturz_import::{
    Balance, Entry, EntryStatus, ImportError, ImportKind, Imported, Importer, ImporterInfo, Limits,
    RECOGNITION_BYTES, Recognition, Statement, StatementAccount,
};
use kassensturz_plugin_api::ExtensionPoint;
use kassensturz_plugin_host::{HostError, Plugin};
use serde_json::Value;

use crate::intern;
use crate::read::{self, Fields, Invalid};

/// The call that names the file name extensions the importer reads:
/// empty input, `{"extensions": ["csv"]}` out.
pub const IMPORTER_INFO: &str = "ks_importer_info";
/// The call that judges a file's first bytes: those bytes in,
/// `{"recognition": "yes" | "maybe" | "no"}` out.
pub const IMPORTER_RECOGNIZE: &str = "ks_importer_recognize";
/// The call that reads a whole file: its bytes in, `{"statements": [...]}`
/// or `{"error": "malformed"}` out.
pub const IMPORTER_READ: &str = "ks_importer_read";

/// The most extensions an importer may name.
const MAX_EXTENSIONS: usize = 8;

/// A plugin importer. Its ID is the plugin's ID, which has dots, so it
/// never equals a built-in importer's. It reads bank statements only; a
/// whole book from another program is the built-in importers' job.
pub struct PluginImporter {
    plugin: Mutex<Plugin>,
    info: ImporterInfo,
}

impl std::fmt::Debug for PluginImporter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginImporter")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

impl PluginImporter {
    /// Makes an importer of a loaded plugin and asks it which extensions it
    /// reads. A plugin without the `importer` extension point lacks
    /// [`IMPORTER_INFO`] as far as KassenSturz is concerned.
    pub fn new(mut plugin: Plugin) -> Result<Self, HostError> {
        if !plugin
            .manifest()
            .extension_points
            .contains(&ExtensionPoint::Importer)
        {
            return Err(HostError::ExportMissing {
                name: IMPORTER_INFO.to_owned(),
            });
        }
        let answer = plugin.call(IMPORTER_INFO, &[])?;
        let extensions = extensions(&answer).map_err(|Invalid| HostError::OutputInvalid)?;
        let info = ImporterInfo {
            id: intern::text(plugin.manifest().id.as_str()),
            kind: ImportKind::Statements,
            extensions: intern::list(&extensions),
        };
        Ok(Self {
            plugin: Mutex::new(plugin),
            info,
        })
    }

    fn call(&self, name: &str, input: &[u8]) -> Result<Vec<u8>, HostError> {
        self.plugin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .call(name, input)
    }
}

impl Importer for PluginImporter {
    fn info(&self) -> ImporterInfo {
        self.info
    }

    /// Anything but a clear answer counts as [`Recognition::No`], since
    /// recognizing must never fail.
    fn recognize(&self, start: &[u8]) -> Recognition {
        let start = &start[..start.len().min(RECOGNITION_BYTES)];
        self.call(IMPORTER_RECOGNIZE, start)
            .ok()
            .and_then(|answer| recognition(&answer).ok())
            .unwrap_or(Recognition::No)
    }

    /// The limits are kept by the host's own limits on what goes in and
    /// out, and by `kassensturz_import::import`, which checks the
    /// statements as it checks any importer's.
    fn read(&self, input: &[u8], _limits: &Limits) -> Result<Imported, ImportError> {
        let answer = self
            .call(IMPORTER_READ, input)
            .map_err(|error| ImportError::Plugin(error.report()))?;
        statements(&answer)
    }
}

fn extensions(answer: &[u8]) -> Result<Vec<&str>, Invalid> {
    let object = read::object(answer)?;
    let fields = Fields::of_map(&object, &["extensions"])?;
    let list = fields.array("extensions")?;
    if list.len() > MAX_EXTENSIONS {
        return Err(Invalid);
    }
    let mut extensions = Vec::new();
    for value in list {
        let extension = read::extension(value.as_str().ok_or(Invalid)?)?;
        if extensions.contains(&extension) {
            return Err(Invalid);
        }
        extensions.push(extension);
    }
    // The texts borrow from `object`, which ends here: keep them first.
    Ok(extensions.into_iter().map(intern::text).collect())
}

fn recognition(answer: &[u8]) -> Result<Recognition, Invalid> {
    let object = read::object(answer)?;
    match Fields::of_map(&object, &["recognition"])?.text("recognition")? {
        "yes" => Ok(Recognition::Yes),
        "maybe" => Ok(Recognition::Maybe),
        "no" => Ok(Recognition::No),
        _ => Err(Invalid),
    }
}

/// The statements, or the plugin's own refusal of the file.
fn statements(answer: &[u8]) -> Result<Imported, ImportError> {
    let invalid = |Invalid| ImportError::Plugin(HostError::OutputInvalid.report());
    let object = read::object(answer).map_err(invalid)?;
    if object.contains_key("error") {
        return Err(refusal(&object).map_err(invalid)?);
    }
    let fields = Fields::of_map(&object, &["statements"]).map_err(invalid)?;
    let mut statements = Vec::new();
    for value in fields.array("statements").map_err(invalid)? {
        statements.push(statement(value)?);
    }
    Ok(Imported::Statements(statements))
}

/// `{"error": "malformed", "line": 7}`: the file isn't one the plugin can
/// read. `line` may be left out.
fn refusal(object: &serde_json::Map<String, Value>) -> Result<ImportError, Invalid> {
    let fields = Fields::of_map(object, &["error", "line"])?;
    if fields.text("error")? != "malformed" {
        return Err(Invalid);
    }
    let line = fields
        .optional("line")
        .map(|line| line.as_u64().filter(|line| *line >= 1).ok_or(Invalid))
        .transpose()?;
    Ok(ImportError::Malformed { line })
}

fn statement(value: &Value) -> Result<Statement, ImportError> {
    let invalid = |Invalid| ImportError::Plugin(HostError::OutputInvalid.report());
    let fields = Fields::of(
        value,
        &[
            "account",
            "currency",
            "opening_balance",
            "closing_balance",
            "entries",
        ],
    )
    .map_err(invalid)?;
    let code = read::currency_code(fields.text("currency").map_err(invalid)?).map_err(invalid)?;
    let currency = code
        .iso_minor_digits()
        .and_then(|digits| Currency::new(code, digits))
        .ok_or(ImportError::UnknownCurrency {
            currency: code,
            line: None,
        })?;
    let account = Fields::of(
        fields.required("account").map_err(invalid)?,
        &["iban", "number"],
    )
    .and_then(|account| {
        Ok(StatementAccount {
            iban: account.optional_text("iban")?,
            number: account.optional_text("number")?,
        })
    })
    .map_err(invalid)?;
    let balance = |key: &str| {
        fields
            .optional(key)
            .map(|value| {
                let balance = Fields::of(value, &["amount", "date"])?;
                Ok(Balance {
                    amount: money(balance.text("amount")?, currency)?,
                    date: read::date(balance.text("date")?)?,
                })
            })
            .transpose()
    };
    let opening_balance = balance("opening_balance").map_err(invalid)?;
    let closing_balance = balance("closing_balance").map_err(invalid)?;
    let entries = fields
        .array("entries")
        .and_then(|entries| {
            entries
                .iter()
                .map(|value| entry(value, currency))
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(invalid)?;
    Ok(Statement {
        account,
        currency,
        opening_balance,
        closing_balance,
        entries,
    })
}

fn entry(value: &Value, currency: Currency) -> Result<Entry, Invalid> {
    let fields = Fields::of(
        value,
        &[
            "status",
            "bank_id",
            "booking_date",
            "value_date",
            "amount",
            "other_party_name",
            "other_party_iban",
            "remittance_information",
        ],
    )?;
    let status = match fields.text("status")? {
        "booked" => EntryStatus::Booked,
        "pending" => EntryStatus::Pending,
        _ => return Err(Invalid),
    };
    Ok(Entry {
        status,
        bank_id: fields.optional_text("bank_id")?,
        booking_date: read::date(fields.text("booking_date")?)?,
        value_date: fields
            .optional_text("value_date")?
            .map(|date| read::date(&date))
            .transpose()?,
        amount: money(fields.text("amount")?, currency)?,
        other_party_name: fields.optional_text("other_party_name")?,
        other_party_iban: fields.optional_text("other_party_iban")?,
        remittance_information: fields.optional_text("remittance_information")?,
    })
}

fn money(text: &str, currency: Currency) -> Result<Money, Invalid> {
    Ok(Money::from_minor_units(
        read::minor_units(text)?,
        currency.code(),
    ))
}
