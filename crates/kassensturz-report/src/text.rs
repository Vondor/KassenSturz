// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Results as text (D-065, D-283): QML gets no numbers, only words the user
//! reads.

use kassensturz_core::{Date, Money};

use crate::Percent;

/// Writes amounts and months for the user's locale. The app implements it
/// with its formatter (D-002); this crate has no locale of its own.
pub trait TextFormat {
    /// `money` as the user's number format writes it, with its currency.
    fn amount(&self, money: Money) -> String;
    /// The month that starts on `first_day`, e.g. "October 2026".
    fn month(&self, first_day: Date) -> String;
    /// `percent` as the user's number format writes it, e.g. "12,3 %".
    fn percent(&self, percent: Percent) -> String;
    /// A plain number from a formula, given in hundredths (1250 is 12.5),
    /// as the user's number format writes it: at most two decimal places,
    /// without trailing zeros, e.g. "12,5".
    fn number(&self, hundredths: i64) -> String;
    /// The name for money that isn't sorted into a category yet.
    fn unsorted(&self) -> String;
}

/// A result as a table of text, which is also the screen-reader alternative
/// to a chart (D-283).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextTable {
    /// The currency of every amount in the table.
    pub currency: String,
    /// Each column's fixed name, which the UI translates, e.g. `category`.
    pub columns: Vec<&'static str>,
    /// The rows, each with one cell per column.
    pub rows: Vec<Vec<String>>,
}
