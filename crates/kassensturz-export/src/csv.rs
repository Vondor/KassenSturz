// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! CSV, the format every spreadsheet reads (D-029, D-099).

use std::io::Write;

use kassensturz_core::Money;

use crate::data::{AccountTransactions, Counterpart, ExportData, TransactionRow};
use crate::decimal_text;
use crate::error::ExportError;
use crate::exporter::{ExportKind, Exporter, ExporterInfo};

/// The first row, naming the columns. They are keys of the format, not
/// text for the user, so they are the same in every language and never
/// change: files from later versions can be read by the same scripts.
const HEADER: [&str; 11] = [
    "date",
    "account",
    "payee",
    "category",
    "transfer",
    "memo",
    "amount",
    "currency",
    "parts",
    "original_amount",
    "original_currency",
];

/// UTF-8's byte order mark, which Excel needs to read UTF-8.
const BYTE_ORDER_MARK: &[u8] = b"\xEF\xBB\xBF";

/// What separates the fields of a row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Separator {
    /// `,`, as RFC 4180 describes.
    #[default]
    Comma,
    /// `;`, which spreadsheets in countries with a decimal comma expect.
    Semicolon,
    /// A tab.
    Tab,
}

impl Separator {
    const fn character(self) -> char {
        match self {
            Self::Comma => ',',
            Self::Semicolon => ';',
            Self::Tab => '\t',
        }
    }
}

/// The mark between the whole and the decimal places of an amount.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecimalMark {
    /// `12.50`.
    #[default]
    Point,
    /// `12,50`.
    Comma,
}

impl DecimalMark {
    const fn character(self) -> char {
        match self {
            Self::Point => '.',
            Self::Comma => ',',
        }
    }
}

/// How a CSV file is written. The defaults are what scripts and most tools
/// read: RFC 4180 with commas, UTF-8 without a byte order mark, a header
/// row, and amounts with a decimal point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CsvExportSettings {
    /// What separates the fields.
    pub separator: Separator,
    /// The mark in amounts. A comma can't be both this and the separator.
    pub decimal_mark: DecimalMark,
    /// Whether the first row names the columns.
    pub header: bool,
    /// Whether the file begins with UTF-8's byte order mark, which Excel
    /// needs to show umlauts right.
    pub byte_order_mark: bool,
}

impl Default for CsvExportSettings {
    fn default() -> Self {
        Self {
            separator: Separator::Comma,
            decimal_mark: DecimalMark::Point,
            header: true,
            byte_order_mark: false,
        }
    }
}

/// Writes the transactions of accounts as CSV, one row per transaction and
/// account, with the columns `date`, `account`, `payee`, `category`,
/// `transfer`, `memo`, `amount`, `currency`, `parts`, `original_amount` and
/// `original_currency`.
///
/// - **Rows** are an account's transactions as its register shows them,
///   oldest first; accounts follow each other in the order given. A Transfer
///   between two exported accounts is in both, once for each side, with
///   opposite amounts.
/// - **`amount`** is what the transaction did to the account, in the
///   account's currency (`currency`), written exactly with the currency's
///   decimal places, with a minus sign for money out and no digit grouping.
///   The amounts of an account's rows add up to its balance.
/// - **`date`** is the booking date as ISO 8601, `2026-10-05`.
/// - **`category`** is the category's path, `Food/Groceries`; **`transfer`**
///   is the other account's name for a Transfer; **`parts`** is the number of
///   other parts, only if there are several. Then both are empty. A
///   transaction without a category yet leaves all three empty.
/// - **`original_amount`** and **`original_currency`** are filled for an
///   amount that was converted from a withdrawn currency (D-104): the
///   amount as it was in marks, say, written like `amount` with the decimal
///   places that currency had, and its code. Otherwise both are empty.
/// - **`memo`** is the purpose a bank delivered, with line breaks kept
///   inside quotes.
/// - **Quoting** follows RFC 4180: a field with the separator, a quote or a
///   line break is in double quotes, quotes doubled; rows end with CR LF.
/// - **Formulas:** a text that starts with `=`, `+`, `-`, `@`, a tab or a
///   carriage return would be run as a formula by a spreadsheet, and
///   payees and memos come from files anyone can send (AGENTS.md §8). Such
///   a text gets a single quote in front, which spreadsheets don't show.
///   Amounts are numbers and are left alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CsvExport {
    settings: CsvExportSettings,
}

impl CsvExport {
    /// An exporter with `settings`, or [`ExportError::BadSettings`] if they
    /// contradict each other: a decimal comma with a comma as separator.
    pub fn new(settings: CsvExportSettings) -> Result<Self, ExportError> {
        if settings.separator.character() == settings.decimal_mark.character() {
            return Err(ExportError::BadSettings);
        }
        Ok(Self { settings })
    }
}

impl Exporter for CsvExport {
    fn info(&self) -> ExporterInfo {
        ExporterInfo {
            id: "csv",
            kind: ExportKind::Transactions,
            extension: "csv",
        }
    }

    fn write(&self, data: &ExportData, out: &mut dyn Write) -> Result<(), ExportError> {
        data.check()?;
        let ExportData::Transactions(accounts) = data else {
            return Err(ExportError::WrongKind);
        };
        if self.settings.byte_order_mark {
            out.write_all(BYTE_ORDER_MARK)?;
        }
        if self.settings.header {
            self.write_row(out, &HEADER.map(str::to_owned))?;
        }
        for account in accounts {
            for row in &account.rows {
                self.write_row(out, &self.fields(account, row)?)?;
            }
        }
        Ok(())
    }
}

impl CsvExport {
    /// The fields of a row, in the order of [`HEADER`].
    fn fields(
        &self,
        account: &AccountTransactions,
        row: &TransactionRow,
    ) -> Result<[String; 11], ExportError> {
        let text = |value: Option<&str>| value.map(guard_formula).unwrap_or_default();
        let (category, transfer, parts) = match &row.counterpart {
            Counterpart::Uncategorized => (None, None, None),
            Counterpart::Category(path) => (Some(path.as_str()), None, None),
            Counterpart::Transfer(name) => (None, Some(name.as_str()), None),
            Counterpart::Parts(count) => (None, None, Some(count.to_string())),
        };
        let (original_amount, original_currency) = match &row.original {
            Some(original) => (
                self.amount(original.amount, original.minor_digits)?,
                original.amount.currency().to_string(),
            ),
            None => (String::new(), String::new()),
        };
        Ok([
            row.date.to_string(),
            guard_formula(&account.account),
            text(row.payee.as_deref()),
            text(category),
            text(transfer),
            text(row.memo.as_deref()),
            self.amount(row.amount, account.minor_digits)?,
            account.currency.to_string(),
            parts.unwrap_or_default(),
            original_amount,
            original_currency,
        ])
    }

    /// `amount` with `digits` decimal places: 1234 and 2 give `12.34`, -5
    /// and 2 give `-0.05`. Exact integer arithmetic (AGENTS.md §3.1 rule 1).
    fn amount(&self, amount: Money, digits: u8) -> Result<String, ExportError> {
        decimal_text(
            amount.minor_units(),
            digits,
            self.settings.decimal_mark.character(),
        )
        .ok_or(ExportError::BadDecimalPlaces {
            currency: amount.currency(),
            digits,
        })
    }

    fn write_row(&self, out: &mut dyn Write, fields: &[String]) -> Result<(), ExportError> {
        let separator = self.settings.separator.character();
        for (index, field) in fields.iter().enumerate() {
            if index > 0 {
                write!(out, "{separator}")?;
            }
            if field.contains([separator, '"', '\r', '\n']) {
                write!(out, "\"{}\"", field.replace('"', "\"\""))?;
            } else {
                out.write_all(field.as_bytes())?;
            }
        }
        out.write_all(b"\r\n")?;
        Ok(())
    }
}

/// `text`, with a single quote in front if a spreadsheet would run it as a
/// formula.
fn guard_formula(text: &str) -> String {
    if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{text}")
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests;
