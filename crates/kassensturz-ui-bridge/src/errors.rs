// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Errors as the screens get them: a code and its parameters as text.
//!
//! The Rust side never writes a message (D-034): QML keeps one translated
//! message per code (`FormError.qml`) and fills in the parameters, which
//! this module has already formatted for the user's locale (D-065).

use kassensturz_core::{CurrencyCode, ErrorParam, ErrorReport, Money};
use kassensturz_store::{Book, StoreError};

use crate::format::{Affixes, AmountFormat, Locale};
use crate::parse::AmountInputError;
use crate::register::AmountWriter;

/// An error as the UI gets it: a code and its parameters, already as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorText {
    /// The code, e.g. `input.malformed-amount` or `store.duplicate-account-name`.
    pub code: &'static str,
    /// The parameters by name, e.g. the example of a correctly typed amount.
    pub params: Vec<(&'static str, String)>,
}

impl ErrorText {
    /// An error without parameters.
    pub fn plain(code: &'static str) -> Self {
        Self {
            code,
            params: Vec::new(),
        }
    }

    /// Why typed text isn't an amount in `currency`, with an example of how
    /// to type one.
    pub fn amount(
        error: AmountInputError,
        book: &Book,
        currency: CurrencyCode,
        locale: &impl Locale,
    ) -> Self {
        match error {
            AmountInputError::Empty => Self::plain("input.empty-amount"),
            AmountInputError::Malformed => Self {
                code: "input.malformed-amount",
                params: vec![(
                    "example",
                    amount_example(book, currency, locale).unwrap_or_default(),
                )],
            },
            AmountInputError::TooManyDecimalPlaces { max } => Self {
                code: "input.too-many-decimal-places",
                params: vec![("max", max.to_string())],
            },
            AmountInputError::TooLarge => Self::plain("input.amount-too-large"),
        }
    }
}

/// An amount written the way the user should type it, e.g. "1.234,56" in
/// German: shown when what they typed couldn't be read.
pub fn amount_example(
    book: &Book,
    currency: CurrencyCode,
    locale: &impl Locale,
) -> Result<String, StoreError> {
    Ok(amount_example_with(
        book.currency_minor_digits(currency)?,
        currency,
        locale,
    ))
}

/// Like [`amount_example`], for a currency with `minor_digits` decimal
/// places, e.g. one the book hasn't used yet.
pub fn amount_example_with(
    minor_digits: u8,
    currency: CurrencyCode,
    locale: &impl Locale,
) -> String {
    let format = AmountFormat {
        positive: Affixes::default(),
        negative: Affixes::default(),
        ..locale.amount_format(currency)
    };
    // 1234 and as many decimal places as the currency has, all 5s and 6s
    // so no digit can be mistaken for a separator.
    let mut minor: i64 = 1234;
    for place in 0..minor_digits {
        minor = minor
            .saturating_mul(10)
            .saturating_add(if place == 0 { 5 } else { 6 });
    }
    format.format(
        Money::from_minor_units(minor, currency),
        minor_digits,
        |value| locale.group_digits(value),
    )
}

/// The error report of core or store as the UI gets it, with every
/// parameter written for `locale`.
pub fn report_text(book: &Book, report: &ErrorReport, locale: &impl Locale) -> ErrorText {
    let params = report
        .params
        .iter()
        .map(|(name, param)| {
            let text = match param {
                ErrorParam::Account(id) => book
                    .account(*id)
                    .map(|account| account.name)
                    .unwrap_or_default(),
                ErrorParam::Money(amount) => AmountWriter::new(book, amount.currency(), locale)
                    .map(|writer| writer.text(*amount).exact)
                    .unwrap_or_default(),
                ErrorParam::Text(text) => text.clone(),
                ErrorParam::Number(number) => number.to_string(),
                ErrorParam::Currency(currency) => currency.to_string(),
                ErrorParam::Date(date) => locale.short_date(*date),
                ErrorParam::Keyword(keyword) => (*keyword).to_owned(),
                other => format!("{other:?}"),
            };
            (*name, text)
        })
        .collect();
    ErrorText {
        code: report.code,
        params,
    }
}
