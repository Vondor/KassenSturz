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
                    // A currency the book doesn't use yet, e.g. in the setup
                    // wizard before the book is written, has the decimal
                    // places ISO 4217 gives it (D-027, M6-B105).
                    amount_example(book, currency, locale)
                        .ok()
                        .or_else(|| {
                            currency.iso_minor_digits().map(|minor_digits| {
                                amount_example_with(minor_digits, currency, locale)
                            })
                        })
                        .unwrap_or_default(),
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
                // A Rule is shown by its name, never by its id (B-307).
                ErrorParam::Rule(id) => book.rule(*id).map(|rule| rule.name).unwrap_or_default(),
                ErrorParam::AccountKind(kind) => kind_word(*kind).to_owned(),
                ErrorParam::AccountType(kind) => type_word(*kind).to_owned(),
                ErrorParam::Timestamp(time) => crate::locale::QtLocale::date_time(*time),
                // Ids of transactions, parts and rates mean nothing to a
                // person: no text, and no Debug output of a type (B-307).
                ErrorParam::Transaction(_) | ErrorParam::Split(_) | ErrorParam::Rate(_) => {
                    String::new()
                }
            };
            (*name, text)
        })
        .collect();
    ErrorText {
        code: report.code,
        params,
    }
}

/// The word `docs/ERROR_CODES.md` gives for a kind of account.
fn kind_word(kind: kassensturz_core::AccountKind) -> &'static str {
    use kassensturz_core::AccountKind as K;
    match kind {
        K::Asset => "asset",
        K::Liability => "liability",
        K::Income => "income",
        K::Expense => "expense",
        K::Equity => "equity",
    }
}

/// The word for a sort of account.
fn type_word(kind: kassensturz_core::AccountType) -> &'static str {
    use kassensturz_core::AccountType as T;
    match kind {
        T::Checking => "checking",
        T::Savings => "savings",
        T::Cash => "cash",
        T::CreditCard => "credit-card",
    }
}

/// The text of a parameter that needs no book to be written: what an error
/// of a part that has no book of its own (a bank, the AI) can carry. A
/// parameter that is only an id, or needs the book, gives no text, never
/// the Debug output of its type: that would reach the screen as
/// `Rule(RuleId(...))` (B-307, D-034).
pub fn param_without_book(param: &ErrorParam) -> String {
    match param {
        ErrorParam::Text(text) => text.clone(),
        ErrorParam::Number(number) => number.to_string(),
        ErrorParam::Currency(currency) => currency.to_string(),
        ErrorParam::Keyword(keyword) => (*keyword).to_owned(),
        ErrorParam::AccountKind(kind) => kind_word(*kind).to_owned(),
        ErrorParam::AccountType(kind) => type_word(*kind).to_owned(),
        ErrorParam::Timestamp(time) => crate::locale::QtLocale::date_time(*time),
        ErrorParam::Money(_)
        | ErrorParam::Date(_)
        | ErrorParam::Account(_)
        | ErrorParam::Transaction(_)
        | ErrorParam::Split(_)
        | ErrorParam::Rate(_)
        | ErrorParam::Rule(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // B-307: no parameter's text is the Debug output of a type.
    #[test]
    fn a_parameter_without_a_book_never_shows_a_types_debug_output() {
        let rule = kassensturz_core::RuleId::generate();
        assert_eq!(param_without_book(&ErrorParam::Rule(rule)), "");
        let transaction = kassensturz_core::TransactionId::generate();
        assert_eq!(
            param_without_book(&ErrorParam::Transaction(transaction)),
            ""
        );
        assert_eq!(
            param_without_book(&ErrorParam::AccountKind(
                kassensturz_core::AccountKind::Asset
            )),
            "asset"
        );
        assert_eq!(
            param_without_book(&ErrorParam::Keyword("not-found")),
            "not-found"
        );
    }

    // The sweep: no non-test code in the bridge turns a parameter into
    // text with `{:?}` (B-307, D-034).
    #[test]
    fn no_error_parameter_is_written_with_debug_output() {
        let source = include_str!("errors.rs");
        let code = source.split("#[cfg(test)]").next().expect("code");
        assert!(!code.contains("format!(\"{other:?}\")"));
        for file in [
            include_str!("bank_connection.rs"),
            include_str!("ai_chat.rs"),
            include_str!("ai_settings_model.rs"),
        ] {
            let code = file.split("#[cfg(test)]").next().expect("code");
            assert!(
                !code.contains("other => format!(\"{other:?}\")"),
                "an error parameter is shown with Debug output"
            );
        }
    }
}
