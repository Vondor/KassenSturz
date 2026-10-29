// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Results as text (D-065, D-283): QML gets no numbers, only words the user
//! reads.

use kassensturz_core::{Date, Money};

/// Writes amounts and months for the user's locale. The app implements it
/// with its formatter (D-002); this crate has no locale of its own.
pub trait TextFormat {
    /// `money` as the user's number format writes it, with its currency.
    fn amount(&self, money: Money) -> String;
    /// The month that starts on `first_day`, e.g. "October 2026".
    fn month(&self, first_day: Date) -> String;
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
