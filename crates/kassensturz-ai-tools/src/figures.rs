// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The number check (D-052, D-188, threat T-26): every number in a model's
//! answer must be one that a function produced.
//!
//! Each result of a read function carries its [`Figures`]: its amounts
//! exactly, its counts, and every number in its texts. The chat collects
//! them over a conversation, and [`check_numbers`] lists every number of an
//! answer that isn't among them, so the answer can say so (M5.4). A number
//! is compared by its value without its sign, and an answer may write it the
//! way the user's language does: `1234.5`, `1,234.50`, `1.234,50` and
//! `1 234,50` are all the same figure. Where a number can be read two ways,
//! such as `1.234` (one thousand two hundred thirty-four, or one point two
//! three four), it passes if either reading is a known figure.
//!
//! Not checked, because they aren't figures: dates (`2026-10-25`,
//! `25.10.2026`, `25 October`, `Oct 25th`), times (`14:30`), a year on its
//! own (`2026`, without a currency next to it), ordinals (`3rd`) and digits
//! that are part of a word or a name (`M5`, `A4`, `3D`). Known limits, as
//! D-188 records: the sign isn't checked, so an answer that calls a payment
//! income passes, and numbers written as words ("twelve") aren't seen.

use std::collections::HashSet;
use std::sync::LazyLock;

use kassensturz_core::{CurrencyCode, Money};
use regex::Regex;

/// The most digits a number can have to be compared: what fits a `u128`.
const MAX_DIGITS: usize = 38;

/// A number's value without its sign: `digits` × 10^−`scale`, with no
/// zero at the end after the decimal mark, so `12.50` and `12.5` are equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Exact {
    digits: u128,
    scale: u8,
}

impl Exact {
    fn new(mut digits: u128, mut scale: u8) -> Self {
        while scale > 0 && digits.is_multiple_of(10) {
            digits /= 10;
            scale -= 1;
        }
        Self { digits, scale }
    }

    /// The number with the whole part `whole` and the part after the decimal
    /// mark `fraction`, both ASCII digits; `None` if it has too many digits.
    fn read(whole: &str, fraction: &str) -> Option<Self> {
        if whole.len() + fraction.len() > MAX_DIGITS {
            return None;
        }
        let digits = format!("{whole}{fraction}").parse().ok()?;
        Some(Self::new(digits, u8::try_from(fraction.len()).ok()?))
    }
}

/// The figures that functions produced, which an answer may name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Figures {
    /// Their values, without the sign.
    values: HashSet<Exact>,
    /// Numbers as they were written in a text, e.g. `0.261025.1`, which
    /// pass when an answer writes them the same way.
    written: HashSet<String>,
}

impl Figures {
    /// No figures.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.written.is_empty()
    }

    /// `amount`, in a currency with `minor_digits` decimal places.
    pub fn add_money(&mut self, amount: Money, minor_digits: u8) {
        self.values.insert(Exact::new(
            u128::from(amount.minor_units().unsigned_abs()),
            minor_digits,
        ));
    }

    /// A count, such as how many transactions a search found.
    pub fn add_count(&mut self, count: u64) {
        self.values.insert(Exact::new(u128::from(count), 0));
    }

    /// Every number in `text`, in every way it can be read. For text the
    /// model got from a function (a payee such as "Shop 24") or from the
    /// user and the app (the user's question, the build's version).
    pub fn add_text(&mut self, text: &str) {
        for number in numbers(text) {
            self.values.extend(number.values);
            self.written.insert(number.written.to_owned());
        }
    }

    /// Adds the figures of `other`, e.g. of the next result in a chat.
    pub fn extend(&mut self, other: &Self) {
        self.values.extend(other.values.iter().copied());
        self.written.extend(other.written.iter().cloned());
    }

    fn knows(&self, number: &Number<'_>) -> bool {
        number
            .values
            .iter()
            .any(|value| self.values.contains(value))
            || self.written.contains(number.written)
    }
}

/// A number in an answer that no function produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unverified {
    /// The number as the answer wrote it, without its sign.
    pub text: String,
    /// Where it starts in the answer, in bytes.
    pub start: usize,
    /// Where it ends in the answer, in bytes.
    pub end: usize,
}

/// Every number in `answer` that isn't among `known`, in the order they
/// appear. An empty list means every figure in the answer came from code.
pub fn check_numbers(answer: &str, known: &Figures) -> Vec<Unverified> {
    numbers(answer)
        .into_iter()
        .filter(|number| !known.knows(number))
        .map(|number| Unverified {
            text: number.written.to_owned(),
            start: number.start,
            end: number.end,
        })
        .collect()
}

/// A number found in a text.
#[derive(Debug)]
struct Number<'a> {
    written: &'a str,
    start: usize,
    end: usize,
    /// Every value it can be read as; none if it can't be read as one, like
    /// `1.2.3`, which then passes only if written the same way before.
    values: Vec<Exact>,
}

const MONTHS: &str = "january|jan|february|feb|march|mar|april|apr|may|june|jun|july|jul|\
                      august|aug|september|sept|sep|october|oct|november|nov|december|dec|\
                      januar|februar|märz|mai|juni|juli|oktober|okt|dezember|dez";

/// Dates and times, which aren't figures.
static NOT_FIGURES: LazyLock<Regex> = LazyLock::new(|| {
    let patterns = [
        // 2026-10-25, 2026/10/25
        r"\b\d{4}[-/]\d{1,2}[-/]\d{1,2}\b".to_owned(),
        // 25.10.2026, 10/25/2026, 25-10-26, 25.10.
        r"\b\d{1,2}[./-]\d{1,2}[./-](?:\d{2}|\d{4})\b".to_owned(),
        r"\b\d{1,2}\.\d{1,2}\.(?:\s|$)".to_owned(),
        // 14:30, 14:30:05
        r"\b\d{1,2}:\d{2}(?::\d{2})?\b".to_owned(),
        // The 1. or 2) that numbers a line of a list
        r"(?m:^)[ \t]*\d{1,2}[.)][ \t]".to_owned(),
        // 25 October, 25. Oktober, 3rd of May
        format!(r"(?i)\b\d{{1,2}}(?:st|nd|rd|th)?\.?\s+(?:of\s+)?(?:{MONTHS})\b"),
        // October 25, Oct 25th
        format!(r"(?i)\b(?:{MONTHS})\.?\s+\d{{1,2}}(?:st|nd|rd|th)?\b"),
    ];
    Regex::new(&patterns.join("|")).expect("valid patterns")
});

/// Currency signs that make a number next to them money, not a year.
const CURRENCY_SIGNS: [char; 6] = ['€', '$', '£', '¥', '₣', '¤'];

/// Whether `c` is part of a word: a digit next to it is part of a name.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Characters that group digits in thousands but never mark decimals.
fn groups_only(c: char) -> bool {
    matches!(c, ' ' | '\u{00A0}' | '\u{202F}' | '\'' | '’')
}

/// Whether the letters `word` are an ISO 4217 code, as in `12 EUR`.
fn is_currency_code(word: &str) -> bool {
    word.len() == 3
        && word.chars().all(|c| c.is_ascii_uppercase())
        && CurrencyCode::new(word).is_ok()
}

/// The numbers in `text` that may be figures.
fn numbers(text: &str) -> Vec<Number<'_>> {
    let skipped: Vec<(usize, usize)> = NOT_FIGURES
        .find_iter(text)
        .map(|found| (found.start(), found.end()))
        .collect();
    let in_skipped = |at: usize| skipped.iter().any(|&(start, end)| start <= at && at < end);
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let at = |index: usize| chars.get(index).map(|&(_, c)| c);
    let offset = |index: usize| chars.get(index).map_or(text.len(), |&(offset, _)| offset);
    let digit_at = |index: usize| at(index).is_some_and(|c| c.is_ascii_digit());

    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (start, c) = chars[i];
        if !c.is_ascii_digit() || in_skipped(start) {
            i += 1;
            continue;
        }
        // Digits right after a letter belong to a name: M5, A000002.
        if i > 0 && at(i - 1).is_some_and(is_word) {
            while at(i).is_some_and(is_word) {
                i += 1;
            }
            continue;
        }
        let first = i;
        let mut groups: Vec<&str> = Vec::new();
        let mut marks: Vec<char> = Vec::new();
        let mut group_start = i;
        loop {
            while digit_at(i) {
                i += 1;
            }
            groups.push(&text[offset(group_start)..offset(i)]);
            match at(i) {
                Some(mark @ ('.' | ',')) if digit_at(i + 1) => {
                    marks.push(mark);
                    i += 1;
                    group_start = i;
                }
                Some(mark)
                    if groups_only(mark)
                        && (1..=3).all(|ahead| digit_at(i + ahead))
                        && !digit_at(i + 4) =>
                {
                    marks.push(mark);
                    i += 1;
                    group_start = i;
                }
                _ => break,
            }
        }
        let end = offset(i);
        let mut currency_after = false;
        if at(i).is_some_and(char::is_alphabetic) {
            let word_start = i;
            while at(i).is_some_and(is_word) {
                i += 1;
            }
            let word = &text[offset(word_start)..offset(i)];
            if is_currency_code(word) {
                currency_after = true;
            } else {
                // 3rd, 12abc, 3D: an ordinal or part of a name.
                continue;
            }
        }
        let written = &text[start..end];
        let year = marks.is_empty()
            && written.len() == 4
            && written
                .parse::<u16>()
                .is_ok_and(|year| (1900..=2199).contains(&year));
        if year && !currency_after && !currency_near(&chars, first, i) {
            continue;
        }
        found.push(Number {
            written,
            start,
            end,
            values: readings(&groups, &marks),
        });
    }
    found
}

/// Whether a currency sign or code stands right before the number that
/// starts at `first` or right after the one that ends before `after`,
/// with only spaces or a sign between.
fn currency_near(chars: &[(usize, char)], first: usize, after: usize) -> bool {
    let before = chars[..first]
        .iter()
        .rev()
        .map(|&(_, c)| c)
        .find(|&c| !c.is_whitespace() && !matches!(c, '-' | '−' | '+'));
    let next = chars[after..]
        .iter()
        .map(|&(_, c)| c)
        .find(|c| !c.is_whitespace());
    let word_before: String = chars[..first]
        .iter()
        .rev()
        .map(|&(_, c)| c)
        .skip_while(|c| c.is_whitespace())
        .take_while(|&c| is_word(c))
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    let word_after: String = chars[after..]
        .iter()
        .map(|&(_, c)| c)
        .skip_while(|c| c.is_whitespace())
        .take_while(|&c| is_word(c))
        .collect();
    before.is_some_and(|c| CURRENCY_SIGNS.contains(&c))
        || next.is_some_and(|c| CURRENCY_SIGNS.contains(&c))
        || is_currency_code(&word_before)
        || is_currency_code(&word_after)
}

/// Every value the digit `groups`, separated by `marks`, can be read as:
/// with the last `.` or `,` as the decimal mark and every other mark
/// grouping thousands, and with every mark grouping thousands.
fn readings(groups: &[&str], marks: &[char]) -> Vec<Exact> {
    let Some((&last, grouping)) = marks.split_last() else {
        return Exact::read(groups[0], "").into_iter().collect();
    };
    // Groups of thousands: one to three digits first, then three each, all
    // separated by the same mark.
    let thousands = |groups: &[&str], marks: &[char]| {
        groups[0].len() <= 3
            && groups[1..].iter().all(|group| group.len() == 3)
            && marks.iter().all(|&mark| mark == marks[0])
    };
    let mut values = Vec::new();
    let (whole, fraction) = groups.split_at(groups.len() - 1);
    if matches!(last, '.' | ',')
        && !grouping.contains(&last)
        && (grouping.is_empty() || thousands(whole, grouping))
        && let Some(value) = Exact::read(&whole.concat(), fraction[0])
    {
        values.push(value);
    }
    if thousands(groups, marks)
        && let Some(value) = Exact::read(&groups.concat(), "")
    {
        values.push(value);
    }
    values
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn eur(minor_units: i64) -> Money {
        Money::from_minor_units(minor_units, CurrencyCode::new("EUR").expect("a code"))
    }

    fn known(amounts: &[i64]) -> Figures {
        let mut figures = Figures::new();
        for &amount in amounts {
            figures.add_money(eur(amount), 2);
        }
        figures
    }

    fn flagged(answer: &str, known: &Figures) -> Vec<String> {
        check_numbers(answer, known)
            .into_iter()
            .map(|number| number.text)
            .collect()
    }

    #[test]
    fn the_numbers_of_a_list_are_not_figures_but_a_figure_after_them_is() {
        let known = known(&[-14850]);
        let answer =
            "The last entries:\n\n1. Insurance -148.50 EUR\n   2) Rent 30.00 EUR\n10. Food";
        // Only the 30.00 was not given by a function.
        assert_eq!(flagged(answer, &known), ["30.00"]);
        // A figure at the start of a line, not followed by a marker's dot
        // and space, is still checked; so is a sentence that starts with one.
        assert_eq!(
            flagged("100 EUR left.\n5.5 EUR more.", &known),
            ["100", "5.5"]
        );
    }

    #[test]
    fn a_figure_passes_written_in_any_common_way() {
        let figures = known(&[-123_450, 5_320]);
        for answer in [
            "You spent 1234.50 EUR.",
            "You spent 1,234.50 €.",
            "Sie haben 1.234,50 € ausgegeben.",
            "Du hast 1 234,50 EUR ausgegeben.",
            "You spent €1,234.5 and then 53.2.",
            "Balance: -1234.5",
            "Kontostand: −53,20 EUR",
        ] {
            assert_eq!(flagged(answer, &figures), Vec::<String>::new(), "{answer}");
        }
    }

    #[test]
    fn an_invented_or_rounded_figure_is_flagged_where_it_stands() {
        let figures = known(&[-123_450]);
        let answer = "You spent 1,234.50 EUR, about 1,235 EUR, which is 12% more.";
        let found = check_numbers(answer, &figures);
        assert_eq!(
            found
                .iter()
                .map(|number| number.text.as_str())
                .collect::<Vec<_>>(),
            ["1,235", "12"]
        );
        assert_eq!(&answer[found[0].start..found[0].end], "1,235");
    }

    #[test]
    fn a_number_read_two_ways_passes_if_either_reading_is_known() {
        let mut thousands = Figures::new();
        thousands.add_count(1234);
        assert!(flagged("1.234 transactions", &thousands).is_empty());
        assert!(flagged("1,234 transactions", &thousands).is_empty());
        let mut small = Figures::new();
        small.add_money(
            Money::from_minor_units(1234, CurrencyCode::new("BHD").expect("a code")),
            3,
        );
        assert!(flagged("1.234 BHD", &small).is_empty());
        assert_eq!(flagged("1.235 BHD", &small), ["1.235"]);
    }

    #[test]
    fn dates_times_years_ordinals_and_names_are_not_figures() {
        let none = Figures::new();
        for answer in [
            "On 2026-10-25 and 25.10.2026 and 10/25/2026 at 14:30.",
            "On 25 October, 3rd of May, Oct 25th, 25. Oktober and 1.10.",
            "In 2026, for the 3rd time, see step M5 and account A000002.",
        ] {
            assert_eq!(flagged(answer, &none), Vec::<String>::new(), "{answer}");
        }
        // A year-like number next to a currency is money.
        assert_eq!(flagged("You paid 2000 EUR.", &none), ["2000"]);
        assert_eq!(flagged("You paid €2000.", &none), ["2000"]);
        assert_eq!(flagged("You paid 2000€.", &none), ["2000"]);
    }

    #[test]
    fn numbers_in_texts_from_functions_or_the_user_pass() {
        let mut figures = Figures::new();
        figures.add_text("Shop 24, build 0.261025.1");
        assert!(flagged("You paid Shop 24 (build 0.261025.1).", &figures).is_empty());
        assert_eq!(flagged("You paid Shop 25.", &figures), ["25"]);
    }

    #[test]
    fn a_sentence_end_or_a_list_is_no_decimal_mark() {
        let figures = known(&[5_320, 1_000]);
        assert!(flagged("It was 53.20. Then 10, then 53.2.", &figures).is_empty());
        assert_eq!(flagged("Odd: 1.2.3", &figures), ["1.2.3"]);
    }

    #[test]
    fn huge_numbers_are_flagged_not_misread() {
        let figures = known(&[i64::MAX, i64::MIN]);
        let digits = "9".repeat(60);
        assert_eq!(flagged(&format!("{digits} EUR"), &figures), [digits]);
        assert!(flagged("92233720368547758.07 and 92233720368547758.08", &figures).is_empty());
    }

    /// `units` with two decimal places as people write it: `.` or `,` as
    /// the decimal mark, with or without groups of thousands.
    fn written(units: i64, style: u8) -> String {
        let magnitude = units.unsigned_abs();
        let (whole, cents) = (magnitude / 100, magnitude % 100);
        let digits = whole.to_string();
        let (group, mark) = match style {
            0 => (None, '.'),
            1 => (Some(','), '.'),
            2 => (Some('.'), ','),
            _ => (Some('\u{202F}'), ','),
        };
        let mut grouped = String::new();
        for (index, digit) in digits.chars().enumerate() {
            if index > 0 && (digits.len() - index) % 3 == 0 {
                grouped.extend(group);
            }
            grouped.push(digit);
        }
        let sign = if units < 0 { "-" } else { "" };
        format!("{sign}{grouped}{mark}{cents:02}")
    }

    proptest! {
        #[test]
        fn every_known_amount_passes_however_it_is_written(units: i64, style in 0..4u8) {
            let figures = known(&[units]);
            let answer = format!("The balance is {} EUR.", written(units, style));
            prop_assert_eq!(flagged(&answer, &figures), Vec::<String>::new());
        }
    }
}
