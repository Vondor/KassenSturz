// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Dates and amounts as CSV files write them, read exactly.

use kassensturz_core::{Currency, Date, Money};

use crate::amount::{DecimalMark, parse_amount};
use crate::csv::settings::DateOrder;
use crate::error::ImportError;

/// Reads `text` as a date in `order`, e.g. `5.10.2026`, `05.10.26` or
/// `20261005` for [`DateOrder::DayMonthYear`]. Spaces around it are left
/// out.
///
/// A two-digit year is read as POSIX `strptime` reads `%y`: 69 to 99 are
/// 1969 to 1999, 00 to 68 are 2000 to 2068.
pub(super) fn parse_date(text: &str, order: DateOrder) -> Result<Date, ImportError> {
    let bad = || ImportError::BadDate {
        found: text.to_owned(),
        line: None,
    };
    let trimmed = text.trim();
    let parts: Vec<&str> = if trimmed.len() == 8 && trimmed.bytes().all(|b| b.is_ascii_digit()) {
        // All eight digits without separators: the year has four.
        match order {
            DateOrder::YearMonthDay => vec![&trimmed[..4], &trimmed[4..6], &trimmed[6..]],
            DateOrder::DayMonthYear | DateOrder::MonthDayYear => {
                vec![&trimmed[..2], &trimmed[2..4], &trimmed[4..]]
            }
        }
    } else {
        let separator = trimmed
            .chars()
            .find(|char| !char.is_ascii_digit())
            .filter(|char| matches!(char, '.' | '-' | '/'))
            .ok_or_else(bad)?;
        trimmed.split(separator).collect()
    };
    let [first, second, third] = parts[..] else {
        return Err(bad());
    };
    let (year, month, day) = match order {
        DateOrder::DayMonthYear => (third, second, first),
        DateOrder::MonthDayYear => (third, first, second),
        DateOrder::YearMonthDay => (first, second, third),
    };
    let number = |part: &str, lengths: &[usize]| {
        (lengths.contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse::<i16>().ok())
            .flatten()
    };
    let year = match year.len() {
        2 => number(year, &[2]).map(|year| if year >= 69 { 1900 + year } else { 2000 + year }),
        _ => number(year, &[4]),
    }
    .ok_or_else(bad)?;
    let month = number(month, &[1, 2])
        .and_then(|month| i8::try_from(month).ok())
        .ok_or_else(bad)?;
    let day = number(day, &[1, 2])
        .and_then(|day| i8::try_from(day).ok())
        .ok_or_else(bad)?;
    Date::new(year, month, day).map_err(|_| bad())
}

/// Reads `text` as an amount of `currency` with `mark` between whole units
/// and decimal places, e.g. `-1.234,56` with a comma.
///
/// Spaces around it are left out. Digit grouping is taken out only where it
/// is written exactly as grouping: one separator, the other decimal mark, a
/// space or an apostrophe, between groups of three digits (`1.234.567,89`,
/// `1 234,50`, `1'234.50`). So a file whose decimal mark isn't the one
/// chosen is refused rather than read wrong: `12.50` with a comma chosen is
/// no amount, never 1250. Everything else is [`parse_amount`]'s: no
/// rounding, at most the currency's decimal places.
pub(super) fn parse_csv_amount(
    text: &str,
    mark: DecimalMark,
    currency: Currency,
) -> Result<Money, ImportError> {
    let bad = || ImportError::BadAmount {
        found: text.to_owned(),
        line: None,
    };
    let trimmed = text.trim();
    let (sign, unsigned) = match trimmed.strip_prefix(['-', '+']) {
        Some(rest) => (&trimmed[..1], rest),
        None => ("", trimmed),
    };
    let mark_char = match mark {
        DecimalMark::Point => '.',
        DecimalMark::Comma => ',',
    };
    let (whole, fraction) = match unsigned.split_once(mark_char) {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (unsigned, None),
    };
    let whole = ungrouped(whole, mark).ok_or_else(bad)?;
    let plain = match fraction {
        Some(fraction) => format!("{sign}{whole}{mark_char}{fraction}"),
        None => format!("{sign}{whole}"),
    };
    // Report the amount as the file wrote it, not as it was rewritten.
    parse_amount(&plain, mark, currency).map_err(|error| match error {
        ImportError::BadAmount { line, .. } => ImportError::BadAmount {
            found: text.to_owned(),
            line,
        },
        ImportError::TooManyDecimalPlaces { currency, line, .. } => {
            ImportError::TooManyDecimalPlaces {
                found: text.to_owned(),
                currency,
                line,
            }
        }
        ImportError::AmountTooLarge { line, .. } => ImportError::AmountTooLarge {
            found: text.to_owned(),
            line,
        },
        other => other,
    })
}

/// The whole units without their digit grouping, or `None` if the grouping
/// isn't exactly groups of three digits after the first, with one
/// separator throughout. Without grouping, `whole` is returned as it is,
/// for [`parse_amount`] to judge.
fn ungrouped(whole: &str, mark: DecimalMark) -> Option<String> {
    let other_mark = match mark {
        DecimalMark::Point => ',',
        DecimalMark::Comma => '.',
    };
    let is_separator =
        |char: char| char == other_mark || matches!(char, ' ' | '\u{a0}' | '\u{202f}' | '\'');
    let Some(separator) = whole.chars().find(|&char| is_separator(char)) else {
        return Some(whole.to_owned());
    };
    let mut groups = whole.split(separator);
    let first = groups.next()?;
    let digits = |group: &str| group.bytes().all(|byte| byte.is_ascii_digit());
    if !(1..=3).contains(&first.len()) || !digits(first) {
        return None;
    }
    let mut out = first.to_owned();
    for group in groups {
        if group.len() != 3 || !digits(group) {
            return None;
        }
        out.push_str(group);
    }
    Some(out)
}
