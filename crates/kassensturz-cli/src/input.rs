// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading what the user typed: dates, currencies and the words for account
//! types and kinds.

use std::str::FromStr;

use kassensturz_core::{AccountKind, AccountType, CurrencyCode, Date};

use crate::error::CliError;
use crate::text::plain;

/// A date typed as `YYYY-MM-DD` (ISO 8601). Nothing else is accepted, so the
/// day and the month can't be mixed up.
pub(crate) fn parse_date(text: &str) -> Result<Date, CliError> {
    let bytes = text.as_bytes();
    let shaped = bytes.len() == 10
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        });
    shaped
        .then(|| Date::from_str(text).ok())
        .flatten()
        .ok_or_else(|| {
            CliError::message(format!(
                "`{}` isn't a date. Type year, month and day like 2026-10-05",
                plain(text)
            ))
        })
}

/// A currency code such as `EUR`. Lowercase letters are accepted.
pub(crate) fn parse_currency(text: &str) -> Result<CurrencyCode, CliError> {
    Ok(CurrencyCode::new(&text.trim().to_ascii_uppercase())?)
}

/// The account types, as typed and as written.
const ACCOUNT_TYPES: [(&str, AccountType); 4] = [
    ("checking", AccountType::Checking),
    ("savings", AccountType::Savings),
    ("cash", AccountType::Cash),
    ("credit-card", AccountType::CreditCard),
];

pub(crate) fn parse_account_type(text: &str) -> Result<AccountType, CliError> {
    ACCOUNT_TYPES
        .iter()
        .find(|(word, _)| *word == text)
        .map(|(_, account_type)| *account_type)
        .ok_or_else(|| {
            CliError::message(format!(
                "`{}` isn't an account type. Choose one of checking, savings, cash, credit-card",
                plain(text)
            ))
        })
}

pub(crate) fn account_type_word(account_type: AccountType) -> &'static str {
    ACCOUNT_TYPES
        .iter()
        .find(|(_, candidate)| *candidate == account_type)
        .map_or("?", |(word, _)| word)
}

/// The kind of a category: `income` or `expense`.
pub(crate) fn parse_category_kind(text: &str) -> Result<AccountKind, CliError> {
    match text {
        "income" => Ok(AccountKind::Income),
        "expense" => Ok(AccountKind::Expense),
        _ => Err(CliError::message(format!(
            "`{}` isn't a kind of category. Choose income or expense",
            plain(text)
        ))),
    }
}

pub(crate) fn category_kind_word(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::Income => "income",
        AccountKind::Expense => "expense",
        AccountKind::Asset | AccountKind::Liability | AccountKind::Equity => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_iso_only() {
        assert_eq!(
            parse_date("2026-10-05").expect("date").to_string(),
            "2026-10-05"
        );
        for text in [
            "05.10.2026",
            "2026-1-5",
            "2026-13-01",
            "2026-02-30",
            "2026-10-05T10:00",
            "",
        ] {
            assert!(parse_date(text).is_err(), "{text:?} should be refused");
        }
    }

    #[test]
    fn currencies_accept_lowercase() {
        assert_eq!(parse_currency("eur").expect("code").as_str(), "EUR");
        assert!(parse_currency("EURO").is_err());
    }

    #[test]
    fn account_types_round_trip() {
        for account_type in AccountType::ALL {
            assert_eq!(
                parse_account_type(account_type_word(account_type)).expect("type"),
                account_type
            );
        }
        assert!(parse_account_type("current").is_err());
    }
}
