// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How the user wants amounts to look (D-002, D-114): the regional format by
//! default, with the decimal separator, digit grouping, the currency
//! symbol's position, negative amounts and the decimal places shown each
//! changeable in Settings.
//!
//! These are the app's settings, kept with KConfig (D-112), not the book's:
//! everyone sees numbers their way. They change only how amounts are
//! written and read, never what is stored or calculated (D-065).

use std::cell::Cell;

use kassensturz_core::{CurrencyCode, Date};

use crate::format::{Affixes, AmountFormat, Locale};
use crate::locale::QtLocale;
use crate::settings::Settings;

/// The group of the settings file that holds the number format.
const DISPLAY: &str = "Display";

/// A no-break space: a space that groups digits, or separates an amount
/// from a symbol written after it, without ever breaking the line there.
const NO_BREAK_SPACE: char = '\u{a0}';

/// One of the number format's settings: its choices, the first of which is
/// the default, and the word the settings file keeps for each.
pub trait Choice: Copy + PartialEq + Sized + 'static {
    /// The key in the settings file.
    const SETTING: &'static str;
    /// Every choice with its word in the settings file; the first is the
    /// default.
    const CHOICES: &'static [(Self, &'static str)];

    /// The word the settings file keeps for this choice.
    fn key(self) -> &'static str {
        Self::CHOICES
            .iter()
            .find(|(choice, _)| *choice == self)
            .map_or("", |(_, key)| key)
    }

    /// The choice the settings file names with `key`. A word KassenSturz
    /// doesn't know, e.g. from a newer version, or none at all, reads as the
    /// default, so a broken file never makes amounts unreadable.
    fn from_key(key: &str) -> Self {
        let found = Self::CHOICES.iter().find(|(_, word)| *word == key);
        found.unwrap_or(&Self::CHOICES[0]).0
    }
}

/// What separates the whole part of an amount from its decimal places.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecimalSeparator {
    /// As in the regional settings.
    #[default]
    Regional,
    /// "1,50".
    Comma,
    /// "1.50".
    Point,
}

impl Choice for DecimalSeparator {
    const SETTING: &'static str = "DecimalSeparator";
    const CHOICES: &'static [(Self, &'static str)] = &[
        (Self::Regional, "regional"),
        (Self::Comma, "comma"),
        (Self::Point, "point"),
    ];
}

/// What separates groups of digits in large amounts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DigitGrouping {
    /// As in the regional settings.
    #[default]
    Regional,
    /// "1.000.000".
    Point,
    /// "1,000,000".
    Comma,
    /// "1 000 000", with no-break spaces.
    Space,
    /// "1'000'000".
    Apostrophe,
    /// "1000000".
    None,
}

impl Choice for DigitGrouping {
    const SETTING: &'static str = "DigitGrouping";
    const CHOICES: &'static [(Self, &'static str)] = &[
        (Self::Regional, "regional"),
        (Self::Point, "point"),
        (Self::Comma, "comma"),
        (Self::Space, "space"),
        (Self::Apostrophe, "apostrophe"),
        (Self::None, "none"),
    ];
}

/// Where the currency's symbol or code goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SymbolPosition {
    /// As in the regional settings.
    #[default]
    Regional,
    /// "€5.00".
    Before,
    /// "5,00 €".
    After,
}

impl Choice for SymbolPosition {
    const SETTING: &'static str = "CurrencySymbol";
    const CHOICES: &'static [(Self, &'static str)] = &[
        (Self::Regional, "regional"),
        (Self::Before, "before"),
        (Self::After, "after"),
    ];
}

/// How amounts below zero are marked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NegativeAmounts {
    /// As in the regional settings.
    #[default]
    Regional,
    /// "-5,00 €".
    Minus,
    /// "(5,00 €)", as in American accounting (D-075).
    Parentheses,
}

impl Choice for NegativeAmounts {
    const SETTING: &'static str = "NegativeAmounts";
    const CHOICES: &'static [(Self, &'static str)] = &[
        (Self::Regional, "regional"),
        (Self::Minus, "minus"),
        (Self::Parentheses, "parentheses"),
    ];
}

/// How many decimal places amounts show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecimalPlaces {
    /// As many as the currency has (ISO 4217, D-027), e.g. "100,00 €".
    #[default]
    All,
    /// None, rounded to whole units, e.g. "100 €". Rounded only for
    /// display: totals are added up from the exact amounts first, and the
    /// exact amount shows on hover and when editing (D-002).
    None,
}

impl Choice for DecimalPlaces {
    const SETTING: &'static str = "DecimalPlaces";
    const CHOICES: &'static [(Self, &'static str)] = &[(Self::All, "all"), (Self::None, "none")];
}

/// The user's number format: each setting is a choice, `Regional` or `All`
/// by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NumberFormat {
    /// What separates the whole part from the decimal places.
    pub decimal_separator: DecimalSeparator,
    /// What separates groups of digits.
    pub digit_grouping: DigitGrouping,
    /// Where the currency's symbol goes.
    pub symbol_position: SymbolPosition,
    /// How amounts below zero are marked.
    pub negative_amounts: NegativeAmounts,
    /// How many decimal places are shown.
    pub decimal_places: DecimalPlaces,
}

impl NumberFormat {
    /// The number format kept in `settings`; the default for anything it
    /// doesn't hold.
    pub fn read(settings: &Settings) -> Self {
        fn read<C: Choice>(settings: &Settings) -> C {
            C::from_key(&settings.text(DISPLAY, C::SETTING))
        }
        Self {
            decimal_separator: read(settings),
            digit_grouping: read(settings),
            symbol_position: read(settings),
            negative_amounts: read(settings),
            decimal_places: read(settings),
        }
    }

    /// Keeps the number format in `settings`. Returns whether it was saved.
    pub fn write(&self, settings: &Settings) -> bool {
        fn write<C: Choice>(settings: &Settings, choice: C) -> bool {
            settings.set_text(DISPLAY, C::SETTING, choice.key())
        }
        // Every setting is written, even after one failed.
        [
            write(settings, self.decimal_separator),
            write(settings, self.digit_grouping),
            write(settings, self.symbol_position),
            write(settings, self.negative_amounts),
            write(settings, self.decimal_places),
        ]
        .into_iter()
        .all(|saved| saved)
    }

    /// The format with the choice that the settings file names `key` for the
    /// setting named `setting`, e.g. "comma" for "DecimalSeparator". `None`
    /// if there is no such setting.
    ///
    /// The decimal separator and the digit grouping can't be the same, or
    /// "1.234" could be read either way. So choosing one that the other uses
    /// in `regional` changes the other to the remaining one of point and
    /// comma: choosing a decimal point in Germany groups digits with commas.
    /// The Settings page shows that change.
    pub fn with(self, setting: &str, key: &str, regional: &impl Locale) -> Option<Self> {
        let mut format = self;
        match setting {
            DecimalSeparator::SETTING => {
                format.decimal_separator = DecimalSeparator::from_key(key);
                let (decimal, group) = format.chosen_separators(regional);
                if decimal == group {
                    format.digit_grouping = if decimal == "." {
                        DigitGrouping::Comma
                    } else {
                        DigitGrouping::Point
                    };
                }
            }
            DigitGrouping::SETTING => {
                format.digit_grouping = DigitGrouping::from_key(key);
                let (decimal, group) = format.chosen_separators(regional);
                if decimal == group {
                    format.decimal_separator = if group == "." {
                        DecimalSeparator::Comma
                    } else {
                        DecimalSeparator::Point
                    };
                }
            }
            SymbolPosition::SETTING => format.symbol_position = SymbolPosition::from_key(key),
            NegativeAmounts::SETTING => format.negative_amounts = NegativeAmounts::from_key(key),
            DecimalPlaces::SETTING => format.decimal_places = DecimalPlaces::from_key(key),
            _ => return None,
        }
        Some(format)
    }

    /// Every setting's key in the settings file with the word for its
    /// current choice.
    pub fn keys(&self) -> [(&'static str, &'static str); 5] {
        [
            (DecimalSeparator::SETTING, self.decimal_separator.key()),
            (DigitGrouping::SETTING, self.digit_grouping.key()),
            (SymbolPosition::SETTING, self.symbol_position.key()),
            (NegativeAmounts::SETTING, self.negative_amounts.key()),
            (DecimalPlaces::SETTING, self.decimal_places.key()),
        ]
    }

    /// The decimal separator and the digit group separator as chosen, with
    /// `regional`'s for those left at "Regional"; they may be the same.
    fn chosen_separators(&self, regional: &impl Locale) -> (String, String) {
        let decimal = match self.decimal_separator {
            DecimalSeparator::Regional => regional.decimal_point(),
            DecimalSeparator::Comma => ",".into(),
            DecimalSeparator::Point => ".".into(),
        };
        let group = match self.digit_grouping {
            DigitGrouping::Regional => regional.group_separator(),
            DigitGrouping::Point => ".".into(),
            DigitGrouping::Comma => ",".into(),
            DigitGrouping::Space => NO_BREAK_SPACE.into(),
            DigitGrouping::Apostrophe => "'".into(),
            DigitGrouping::None => String::new(),
        };
        (decimal, group)
    }
}

/// The regional format `base` with the user's number format applied.
pub struct UserLocale<L> {
    base: L,
    format: NumberFormat,
}

impl<L: Locale> UserLocale<L> {
    /// `base`, as `format` changes it.
    pub fn new(base: L, format: NumberFormat) -> Self {
        Self { base, format }
    }

    /// The decimal separator and the digit group separator. Should both be
    /// the same, e.g. in a settings file edited by hand, the decimal
    /// separator wins and digits are grouped with the other one of point and
    /// comma, so an amount can always be read only one way.
    fn separators(&self) -> (String, String) {
        let (decimal, group) = self.format.chosen_separators(&self.base);
        if decimal == group {
            let other = if decimal == "." { "," } else { "." };
            return (decimal, other.into());
        }
        (decimal, group)
    }

    /// `base`'s affixes, rebuilt with the symbol's position and the way of
    /// marking negative amounts the user chose. Only called when the user
    /// changed one of them: then the minus sign, the symbol and a no-break
    /// space between symbol and digits are put together in the usual
    /// order, "-€5.00", "(€5.00)", "-5,00 €" or "(5,00 €)", in place of any
    /// unusual arrangement the locale has.
    fn rebuilt_affixes(&self, base: &AmountFormat) -> (Affixes, Affixes) {
        let (positive, negative) = (&base.positive, &base.negative);
        // The positive affixes hold only the symbol and spacing.
        let symbol: String = positive
            .prefix
            .chars()
            .chain(positive.suffix.chars())
            .filter(|c| !c.is_whitespace())
            .collect();
        let symbol_before = match self.format.symbol_position {
            SymbolPosition::Regional => positive.prefix.chars().any(|c| !c.is_whitespace()),
            SymbolPosition::Before => true,
            SymbolPosition::After => false,
        };
        let parentheses = match self.format.negative_amounts {
            NegativeAmounts::Regional => negative.prefix.contains('('),
            NegativeAmounts::Minus => false,
            NegativeAmounts::Parentheses => true,
        };
        // The locale's minus sign is what the negative affixes add to the
        // symbol, e.g. "-", or "−" (U+2212) in some locales. A locale that
        // writes parentheses has none: then it is "-".
        let mut marks = format!("{}{}", negative.prefix, negative.suffix);
        if !symbol.is_empty() {
            marks = marks.replace(&symbol, "");
        }
        let minus: String = marks
            .chars()
            .filter(|c| !c.is_whitespace() && !matches!(c, '(' | ')'))
            .collect();
        let minus = if minus.is_empty() { "-".into() } else { minus };

        let (open, close) = if parentheses {
            ("(", ")")
        } else {
            (minus.as_str(), "")
        };
        if symbol_before {
            (
                Affixes::spaced(symbol.clone(), String::new()),
                Affixes::spaced(format!("{open}{symbol}"), close.into()),
            )
        } else {
            let after = if symbol.is_empty() {
                String::new()
            } else {
                format!("{NO_BREAK_SPACE}{symbol}")
            };
            (
                Affixes::spaced(String::new(), after.clone()),
                Affixes::spaced(open.into(), format!("{after}{close}")),
            )
        }
    }
}

impl<L: Locale> Locale for UserLocale<L> {
    fn amount_format(&self, currency: CurrencyCode) -> AmountFormat {
        let mut format = self.base.amount_format(currency);
        format.decimal_point = self.separators().0;
        if self.format.symbol_position != SymbolPosition::Regional
            || self.format.negative_amounts != NegativeAmounts::Regional
        {
            (format.positive, format.negative) = self.rebuilt_affixes(&format);
        }
        format
    }

    fn decimal_point(&self) -> String {
        self.separators().0
    }

    fn group_digits(&self, value: u64) -> String {
        let grouped = self.base.group_digits(value);
        let regional = self.base.group_separator();
        let group = self.separators().1;
        if group == regional {
            grouped
        } else if regional.is_empty() {
            // A locale that doesn't group digits: groups of three.
            let digits: Vec<char> = grouped.chars().collect();
            let mut text = String::new();
            for (index, digit) in digits.iter().enumerate() {
                if index > 0 && (digits.len() - index).is_multiple_of(3) {
                    text.push_str(&group);
                }
                text.push(*digit);
            }
            text
        } else {
            // The locale's group sizes stay, e.g. groups of two in India.
            grouped.replace(&regional, &group)
        }
    }

    fn group_separator(&self) -> String {
        self.separators().1
    }

    fn short_date(&self, date: Date) -> String {
        self.base.short_date(date)
    }

    fn read_date(&self, text: &str) -> Option<Date> {
        self.base.read_date(text)
    }

    fn today(&self) -> Date {
        self.base.today()
    }

    fn shown_decimal_places(&self, minor_digits: u8) -> u8 {
        match self.format.decimal_places {
            DecimalPlaces::All => minor_digits,
            DecimalPlaces::None => 0,
        }
    }
}

thread_local! {
    /// The number format the app uses, read from the settings file at the
    /// first use. QML objects live on the GUI thread only, as the book does
    /// (`session.rs`).
    static CURRENT: Cell<Option<NumberFormat>> = const { Cell::new(None) };
}

/// The number format the app uses now.
pub fn current() -> NumberFormat {
    CURRENT.with(|current| {
        current.get().unwrap_or_else(|| {
            let format = NumberFormat::read(&Settings::user());
            current.set(Some(format));
            format
        })
    })
}

/// The regional format with the user's number format: how the app writes
/// and reads amounts.
pub fn app_locale() -> UserLocale<QtLocale> {
    UserLocale::new(QtLocale, current())
}

/// Makes `format` the one the app uses, from now on and at the next start.
/// Returns whether it was saved; it is used until the app closes anyway.
pub fn set_current(format: NumberFormat) -> bool {
    CURRENT.with(|current| current.set(Some(format)));
    format.write(&Settings::user())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::parse::AmountReader;
    use crate::register::tests::TestLocale;
    use crate::test_folder::TestFolder;
    use kassensturz_core::Money;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    /// Like Qt's American English: "$1,234.56", "($53.20)", grouped by three
    /// with `group`; a locale that doesn't group digits if it is empty.
    struct American {
        group: &'static str,
    }

    const AMERICAN: American = American { group: "," };

    impl Locale for American {
        fn amount_format(&self, _currency: CurrencyCode) -> AmountFormat {
            AmountFormat {
                decimal_point: ".".into(),
                zero_digit: '0',
                positive: Affixes::spaced("$".into(), String::new()),
                negative: Affixes::spaced("($".into(), ")".into()),
            }
        }

        fn decimal_point(&self) -> String {
            ".".into()
        }

        fn group_digits(&self, value: u64) -> String {
            let digits = value.to_string();
            let mut grouped = String::new();
            for (index, digit) in digits.chars().enumerate() {
                if index > 0 && (digits.len() - index).is_multiple_of(3) {
                    grouped.push_str(self.group);
                }
                grouped.push(digit);
            }
            grouped
        }

        fn group_separator(&self) -> String {
            self.group.into()
        }

        fn short_date(&self, date: Date) -> String {
            date.to_string()
        }

        fn read_date(&self, text: &str) -> Option<Date> {
            text.parse().ok()
        }

        fn today(&self) -> Date {
            Date::constant(2026, 10, 22)
        }
    }

    fn text(locale: &impl Locale, minor_units: i64) -> String {
        locale.amount_format(EUR).format_rounded(
            Money::from_minor_units(minor_units, EUR),
            2,
            locale.shown_decimal_places(2),
            |value| locale.group_digits(value),
        )
    }

    fn read(locale: &impl Locale, typed: &str) -> Option<Money> {
        let format = locale.amount_format(EUR);
        let group_separator = locale.group_separator();
        AmountReader {
            format: &format,
            group_separator: &group_separator,
            currency: EUR,
            minor_digits: 2,
        }
        .read(typed)
        .ok()
    }

    fn american(format: NumberFormat) -> UserLocale<American> {
        UserLocale::new(AMERICAN, format)
    }

    #[test]
    fn the_default_is_the_regional_format() {
        let locale = american(NumberFormat::default());
        assert_eq!(text(&locale, 123_456_789), "$1,234,567.89");
        assert_eq!(text(&locale, -5320), "($53.20)");
    }

    #[test]
    fn changes_each_part_of_the_format() {
        let format = NumberFormat {
            decimal_separator: DecimalSeparator::Comma,
            digit_grouping: DigitGrouping::Point,
            symbol_position: SymbolPosition::After,
            negative_amounts: NegativeAmounts::Minus,
            decimal_places: DecimalPlaces::All,
        };
        let locale = american(format);
        assert_eq!(text(&locale, 123_456_789), "1.234.567,89\u{a0}$");
        assert_eq!(text(&locale, -5320), "-53,20\u{a0}$");

        let locale = american(NumberFormat {
            negative_amounts: NegativeAmounts::Minus,
            ..NumberFormat::default()
        });
        assert_eq!(text(&locale, -5320), "-$53.20");

        let german = UserLocale::new(
            TestLocale,
            NumberFormat {
                symbol_position: SymbolPosition::Before,
                negative_amounts: NegativeAmounts::Parentheses,
                ..NumberFormat::default()
            },
        );
        assert_eq!(text(&german, 5320), "€53,20");
        assert_eq!(text(&german, -5320), "(€53,20)");
    }

    #[test]
    fn groups_digits_as_chosen() {
        let grouped = |digit_grouping| {
            let locale = american(NumberFormat {
                digit_grouping,
                ..NumberFormat::default()
            });
            text(&locale, 123_456_789)
        };
        assert_eq!(grouped(DigitGrouping::Space), "$1\u{a0}234\u{a0}567.89");
        assert_eq!(grouped(DigitGrouping::Apostrophe), "$1'234'567.89");
        assert_eq!(grouped(DigitGrouping::None), "$1234567.89");
        // A locale that doesn't group digits: then groups of three.
        let locale = UserLocale::new(
            American { group: "" },
            NumberFormat {
                digit_grouping: DigitGrouping::Space,
                ..NumberFormat::default()
            },
        );
        assert_eq!(text(&locale, 123_456_789), "$1\u{a0}234\u{a0}567.89");
    }

    #[test]
    fn rounds_only_for_display() {
        let locale = american(NumberFormat {
            decimal_places: DecimalPlaces::None,
            ..NumberFormat::default()
        });
        assert_eq!(text(&locale, 123_450), "$1,235");
        assert_eq!(text(&locale, -5320), "($53)");
        // Typing still takes the currency's decimal places.
        assert_eq!(
            read(&locale, "1,234.56"),
            Some(Money::from_minor_units(123_456, EUR))
        );
    }

    #[test]
    fn choosing_a_separator_the_other_one_uses_swaps_them() {
        // In American English, a decimal comma makes the grouping a point.
        let format = NumberFormat::default()
            .with("DecimalSeparator", "comma", &AMERICAN)
            .expect("a setting");
        assert_eq!(format.digit_grouping, DigitGrouping::Point);
        assert_eq!(text(&american(format), 123_456), "$1.234,56");
        // And grouping with commas again brings the decimal point back.
        let format = format
            .with("DigitGrouping", "comma", &AMERICAN)
            .expect("a setting");
        assert_eq!(format.decimal_separator, DecimalSeparator::Point);
        assert_eq!(text(&american(format), 123_456), "$1,234.56");
        // A space doesn't touch the decimal separator.
        let format = format
            .with("DigitGrouping", "space", &AMERICAN)
            .expect("a setting");
        assert_eq!(format.decimal_separator, DecimalSeparator::Point);
        assert_eq!(
            NumberFormat::default().with("Colour", "red", &AMERICAN),
            None
        );
    }

    #[test]
    fn a_clash_from_a_hand_edited_file_still_reads_one_way() {
        let locale = american(NumberFormat {
            decimal_separator: DecimalSeparator::Comma,
            digit_grouping: DigitGrouping::Comma,
            ..NumberFormat::default()
        });
        assert_eq!(text(&locale, 123_456), "$1.234,56");
        assert_eq!(locale.group_separator(), ".");
    }

    #[test]
    fn keeps_the_format_in_the_settings_file() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert_eq!(NumberFormat::read(&settings), NumberFormat::default());

        let format = NumberFormat {
            decimal_separator: DecimalSeparator::Point,
            digit_grouping: DigitGrouping::Apostrophe,
            symbol_position: SymbolPosition::Before,
            negative_amounts: NegativeAmounts::Parentheses,
            decimal_places: DecimalPlaces::None,
        };
        assert!(format.write(&settings));
        assert_eq!(NumberFormat::read(&Settings::at(&file)), format);
        let written = std::fs::read_to_string(&file).expect("settings file written");
        assert!(
            written.contains("[Display]\nCurrencySymbol=before\nDecimalPlaces=none\n"),
            "{written}"
        );

        // An unknown word reads as the default.
        assert!(settings.set_text(DISPLAY, "DigitGrouping", "dots"));
        assert_eq!(
            NumberFormat::read(&settings).digit_grouping,
            DigitGrouping::Regional
        );
    }

    fn any_format() -> impl Strategy<Value = NumberFormat> {
        fn any_choice<C: Choice + core::fmt::Debug>() -> impl Strategy<Value = C> {
            proptest::sample::select(
                C::CHOICES
                    .iter()
                    .map(|(choice, _)| *choice)
                    .collect::<Vec<_>>(),
            )
        }
        (any_choice(), any_choice(), any_choice(), any_choice()).prop_map(
            |(decimal_separator, digit_grouping, symbol_position, negative_amounts)| NumberFormat {
                decimal_separator,
                digit_grouping,
                symbol_position,
                negative_amounts,
                decimal_places: DecimalPlaces::All,
            },
        )
    }

    proptest! {
        // Where formatting meets reading: whatever the user chose, an amount
        // written with every decimal place reads back as itself.
        #[test]
        fn every_format_reads_back_what_it_writes(
            format in any_format(),
            minor_units in any::<i64>(),
        ) {
            let american = american(format);
            prop_assert_eq!(
                read(&american, &text(&american, minor_units)),
                Some(Money::from_minor_units(minor_units, EUR))
            );
            let german = UserLocale::new(TestLocale, format);
            prop_assert_eq!(
                read(&german, &text(&german, minor_units)),
                Some(Money::from_minor_units(minor_units, EUR))
            );
        }

        #[test]
        fn separators_never_clash(format in any_format()) {
            for (decimal, group) in [
                american(format).separators(),
                UserLocale::new(TestLocale, format).separators(),
            ] {
                prop_assert_ne!(decimal, group);
            }
        }
    }
}
