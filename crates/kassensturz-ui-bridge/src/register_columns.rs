// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which columns an account's register shows (US1.19, D-657).
//!
//! The default columns are the ones the register always had: date, from or
//! to, category, payment and deposit, and the balance after each entry. On
//! top of those, the user can add details a bank or file delivered, such as
//! the other party's IBAN, and can hide the category and the balance. The
//! choice belongs to one account and to this computer, so it is kept in the
//! app's settings file (D-112), not in the book: it changes only what is
//! shown, never what is stored.

/// One column of the register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    /// The booking date.
    Date,
    /// Who the money came from or went to.
    OtherParty,
    /// The category, or for a Transfer the other account.
    Category,
    /// The payment and the deposit column. The amount of an entry is always
    /// shown (correct money comes first, AGENTS.md §2).
    Amount,
    /// What the account held after each entry.
    Balance,
    /// The other party's IBAN, or whatever account number the bank gave.
    OtherPartyIban,
    /// What the payment came with (German: Verwendungszweck).
    Purpose,
    /// The day the money counts from or until (German: Wertstellung).
    ValueDate,
}

impl Column {
    /// Every column, in the order the register lays them out.
    pub const ALL: [Self; 8] = [
        Self::Date,
        Self::ValueDate,
        Self::OtherParty,
        Self::OtherPartyIban,
        Self::Purpose,
        Self::Category,
        Self::Amount,
        Self::Balance,
    ];

    /// The key the UI and the settings file know the column by.
    pub fn key(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::OtherParty => "other-party",
            Self::Category => "category",
            Self::Amount => "amount",
            Self::Balance => "balance",
            Self::OtherPartyIban => "other-party-iban",
            Self::Purpose => "purpose",
            Self::ValueDate => "value-date",
        }
    }

    /// The column for a key from [`Column::key`].
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|column| column.key() == key)
    }

    /// Whether the column is always shown: what happened, when, with whom
    /// and for how much.
    pub fn fixed(self) -> bool {
        matches!(self, Self::Date | Self::OtherParty | Self::Amount)
    }

    /// Whether a new user sees the column. These are the columns the
    /// register had before the user could choose.
    pub fn shown_by_default(self) -> bool {
        self.fixed() || matches!(self, Self::Category | Self::Balance)
    }

    /// Whether the column shows what a bank or file delivered, which the
    /// register reads from the book only when one of them is shown.
    pub fn is_delivered(self) -> bool {
        matches!(self, Self::OtherPartyIban | Self::Purpose | Self::ValueDate)
    }

    /// The column's bit in [`Columns`].
    fn bit(self) -> u8 {
        1 << Self::ALL
            .iter()
            .position(|column| *column == self)
            .unwrap_or(0)
    }
}

/// The columns of one account's register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Columns(u8);

impl Default for Columns {
    fn default() -> Self {
        Self(
            Column::ALL
                .into_iter()
                .filter(|column| column.shown_by_default())
                .fold(0, |bits, column| bits | column.bit()),
        )
    }
}

impl Columns {
    /// The columns a settings file's `text` names, as [`Columns::text`]
    /// wrote it. Nothing kept, or nothing this version knows (e.g. keys of a
    /// newer version), reads as the default columns, and a key that is not
    /// known is skipped, so a broken file never hides the register. The
    /// fixed columns are shown whatever the text says.
    pub fn read(text: &str) -> Self {
        let named: Vec<Column> = text
            .split(',')
            .filter_map(|key| Column::from_key(key.trim()))
            .collect();
        if named.is_empty() {
            return Self::default();
        }
        let fixed = Column::ALL
            .into_iter()
            .filter(|column| column.fixed())
            .fold(0, |bits, column| bits | column.bit());
        Self(
            named
                .into_iter()
                .fold(fixed, |bits, column| bits | column.bit()),
        )
    }

    /// The columns as the settings file keeps them: the keys of the shown
    /// ones, in the register's order, separated by commas.
    pub fn text(self) -> String {
        Column::ALL
            .into_iter()
            .filter(|column| self.shows(*column))
            .map(Column::key)
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Whether `column` is shown.
    pub fn shows(self, column: Column) -> bool {
        self.0 & column.bit() != 0
    }

    /// These columns with `column` shown or hidden. A fixed column stays
    /// shown.
    #[must_use]
    pub fn with(self, column: Column, shown: bool) -> Self {
        if column.fixed() {
            return self;
        }
        if shown {
            Self(self.0 | column.bit())
        } else {
            Self(self.0 & !column.bit())
        }
    }

    /// Whether any shown column needs what a bank or file delivered.
    pub fn needs_import(self) -> bool {
        Column::ALL
            .into_iter()
            .any(|column| column.is_delivered() && self.shows(column))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_columns_are_the_ones_the_register_always_had() {
        let shown: Vec<_> = Column::ALL
            .into_iter()
            .filter(|column| Columns::default().shows(*column))
            .collect();
        assert_eq!(
            shown,
            [
                Column::Date,
                Column::OtherParty,
                Column::Category,
                Column::Amount,
                Column::Balance
            ]
        );
        assert!(!Columns::default().needs_import());
    }

    #[test]
    fn keys_round_trip() {
        for column in Column::ALL {
            assert_eq!(Column::from_key(column.key()), Some(column));
        }
        assert_eq!(Column::from_key("iban"), None);
    }

    #[test]
    fn what_is_kept_reads_back_for_every_choice() {
        let optional: Vec<Column> = Column::ALL
            .into_iter()
            .filter(|column| !column.fixed())
            .collect();
        for mask in 0..(1_u32 << optional.len()) {
            let mut columns = Columns::default();
            for (index, column) in optional.iter().enumerate() {
                columns = columns.with(*column, mask & (1 << index) != 0);
            }
            assert_eq!(
                Columns::read(&columns.text()),
                columns,
                "{}",
                columns.text()
            );
        }
    }

    #[test]
    fn nothing_kept_or_nothing_known_reads_as_the_default() {
        assert_eq!(Columns::read(""), Columns::default());
        assert_eq!(Columns::read("size,colour"), Columns::default());
    }

    #[test]
    fn an_unknown_key_is_skipped_and_a_fixed_column_cannot_be_lost() {
        let columns = Columns::read(" purpose , from-the-future,date");
        assert!(columns.shows(Column::Purpose));
        assert!(!columns.shows(Column::Category));
        assert!(!columns.shows(Column::Balance));
        for column in Column::ALL.into_iter().filter(|column| column.fixed()) {
            assert!(columns.shows(column), "{}", column.key());
        }
        for column in Column::ALL.into_iter().filter(|column| column.fixed()) {
            assert!(!columns.with(column, false).text().is_empty());
            assert!(columns.with(column, false).shows(column));
        }
    }

    #[test]
    fn hiding_everything_that_can_be_hidden_is_kept_as_chosen() {
        let bare = Columns::default()
            .with(Column::Category, false)
            .with(Column::Balance, false);
        assert_eq!(bare.text(), "date,other-party,amount");
        assert_eq!(Columns::read(&bare.text()), bare);
        assert_ne!(Columns::read(&bare.text()), Columns::default());
    }

    #[test]
    fn only_the_columns_from_an_import_need_one() {
        for column in Column::ALL {
            let columns = Columns::default().with(column, true);
            assert_eq!(
                columns.needs_import(),
                column.is_delivered(),
                "{}",
                column.key()
            );
        }
    }
}
