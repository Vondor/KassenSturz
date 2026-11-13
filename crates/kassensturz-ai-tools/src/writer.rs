// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Writes a result's values and notes each figure in it on the way, so a
//! result can't hold a number its [`Figures`] lack.

use std::collections::HashMap;

use kassensturz_core::{CurrencyCode, Money};
use kassensturz_store::{Book, StoreError};
use serde_json::Value;

use crate::figures::Figures;
use crate::text::for_model;

/// Builds the values of one result from one book.
pub(crate) struct Writer<'a> {
    book: &'a Book,
    digits: HashMap<CurrencyCode, u8>,
    pub(crate) figures: Figures,
}

impl<'a> Writer<'a> {
    pub(crate) fn new(book: &'a Book) -> Self {
        Self {
            book,
            digits: HashMap::new(),
            figures: Figures::new(),
        }
    }

    /// The decimal places of `currency` in this book.
    pub(crate) fn digits(&mut self, currency: CurrencyCode) -> Result<u8, StoreError> {
        match self.digits.get(&currency) {
            Some(&digits) => Ok(digits),
            None => {
                let digits = self.book.currency_minor_digits(currency)?;
                self.digits.insert(currency, digits);
                Ok(digits)
            }
        }
    }

    /// `amount` as exact decimal text with `.` as the decimal mark and the
    /// currency's decimal places, e.g. `-1234.50` (D-065): never a JSON
    /// number, which a reader may take as floating point.
    pub(crate) fn amount(&mut self, amount: Money) -> Result<Value, StoreError> {
        let digits = self.digits(amount.currency())?;
        self.figures.add_money(amount, digits);
        Ok(Value::String(decimal_text(amount.minor_units(), digits)))
    }

    /// `part` as [`Self::amount`] writes it, noted as a part of `total`
    /// that an answer may name only next to that total, such as what is
    /// booked to a parent category itself (#850). `total` must be written
    /// with [`Self::amount`] too.
    pub(crate) fn own_bookings(&mut self, part: Money, total: Money) -> Result<Value, StoreError> {
        let digits = self.digits(part.currency())?;
        self.figures.add_part_of(part, total, digits);
        Ok(Value::String(decimal_text(part.minor_units(), digits)))
    }

    /// A count, such as how many rows were left out.
    pub(crate) fn count(&mut self, count: usize) -> Value {
        let count = u64::try_from(count).unwrap_or(u64::MAX);
        self.figures.add_count(count);
        Value::from(count)
    }

    /// Text from the book, made harmless for a model (`text::for_model`).
    pub(crate) fn text(&mut self, text: &str) -> Value {
        let shown = for_model(text);
        self.figures.add_text(&shown);
        Value::String(shown)
    }

    /// A pattern of a Rule, cleaned as [`Self::text`] cleans one but kept
    /// whole up to `max_chars` characters, the longest `propose_changes`
    /// takes for a pattern; a longer one is cut there and the flag is
    /// `true`, so a model never takes it for the whole (#825).
    pub(crate) fn pattern_text(&mut self, text: &str, max_chars: usize) -> (Value, bool) {
        let cleaned = crate::text::cleaned_for_model(text);
        let mut shown: String = cleaned.chars().take(max_chars).collect();
        let cut = shown.len() < cleaned.len();
        if cut {
            shown = shown.trim_end().to_owned();
        }
        self.figures.add_text(&shown);
        (Value::String(shown), cut)
    }

    /// A longer text of the user's, cleaned as [`Self::text`] cleans one,
    /// whole or not at all: `None` when it is over `max_bytes` (D-729).
    pub(crate) fn whole_text(&mut self, text: &str, max_bytes: usize) -> Option<Value> {
        let shown = crate::text::cleaned_for_model(text);
        if shown.len() > max_bytes {
            return None;
        }
        self.figures.add_text(&shown);
        Some(Value::String(shown))
    }
}

/// `units` minor units with `digits` decimal places as text, exactly:
/// 123450 and 2 give `1234.50`, -5 and 2 give `-0.05`.
pub(crate) fn decimal_text(units: i64, digits: u8) -> String {
    let sign = if units < 0 { "-" } else { "" };
    let digits = usize::from(digits);
    let magnitude = format!("{:0>width$}", units.unsigned_abs(), width = digits + 1);
    if digits == 0 {
        return format!("{sign}{magnitude}");
    }
    let (whole, fraction) = magnitude.split_at(magnitude.len() - digits);
    format!("{sign}{whole}.{fraction}")
}

#[cfg(test)]
mod tests {
    use super::decimal_text;

    #[test]
    fn amounts_are_written_exactly() {
        assert_eq!(decimal_text(123_450, 2), "1234.50");
        assert_eq!(decimal_text(-5, 2), "-0.05");
        assert_eq!(decimal_text(0, 2), "0.00");
        assert_eq!(decimal_text(5000, 0), "5000");
        assert_eq!(decimal_text(1234, 3), "1.234");
        assert_eq!(decimal_text(i64::MIN, 2), "-92233720368547758.08");
    }
}
