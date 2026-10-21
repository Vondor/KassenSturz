// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Withdrawn currencies: ones another currency replaced at a fixed rate,
//! such as the German mark, which the euro replaced (D-077, D-104), from
//! KassenSturz's own table in `data/withdrawn.txt`.

use crate::Date;
use crate::currency::{Currency, CurrencyCode, count_entries, next_entry, sorts_before};
use crate::decimal::{ExchangeRate, MAX_DECIMAL_PLACES};
use crate::money::{Money, MoneyError};
use crate::rates::{DatedRate, DatedRateId, RateSource};
use crate::rounding::RoundingMode;

/// A currency that another one replaced at a fixed rate: the German mark,
/// for example, which the euro replaced at 1 EUR = 1.95583 DEM from
/// 1 January 1999.
///
/// No account can be in one (D-104). An importer converts its amounts into
/// the currency that replaced it, and each converted part keeps the amount
/// it had, its currency and the rate as its original amount
/// ([`OriginalAmount`](crate::OriginalAmount), D-077).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Withdrawn {
    currency: Currency,
    successor: CurrencyCode,
    // The rate's digits and decimal places: the table is read while
    // compiling, and an `ExchangeRate` can't be made then.
    rate_digits: i64,
    rate_decimal_places: u8,
    date: Date,
}

impl Withdrawn {
    /// A withdrawn currency that KassenSturz's table doesn't list, with what
    /// the user said replaced it: `successor`, from `date`, when one unit of
    /// it was worth `rate` units of `currency`, e.g. the old Romanian leu
    /// (ROL), which the new one (RON) replaced at 10000 in 2005 (D-106).
    ///
    /// `currency` has the decimal places its amounts were counted in.
    /// `None` if `successor` is `currency` itself.
    pub fn new(
        currency: Currency,
        successor: CurrencyCode,
        rate: ExchangeRate,
        date: Date,
    ) -> Option<Self> {
        (successor != currency.code()).then_some(Self {
            currency,
            successor,
            rate_digits: rate.digits(),
            rate_decimal_places: rate.decimal_places(),
            date,
        })
    }

    /// The withdrawn currency, with the decimal places ISO 4217 gave it
    /// while it was current: DEM with 2, ITL with 0.
    pub const fn currency(self) -> Currency {
        self.currency
    }

    /// The current currency that replaced it: EUR for the mark.
    pub const fn successor(self) -> CurrencyCode {
        self.successor
    }

    /// How many units of it one unit of the successor was worth, as the
    /// regulation writes it: 1.95583 for the mark.
    pub fn rate(self) -> ExchangeRate {
        ExchangeRate::from_digits(self.rate_digits, self.rate_decimal_places)
            .expect("the table's rates were checked while compiling, and others were rates")
    }

    /// The day the rate took effect: 1 January 1999 for the mark.
    pub const fn date(self) -> Date {
        self.date
    }

    /// The fixed rate as a part keeps it: one unit of the successor costs
    /// [`Withdrawn::rate`] units of the withdrawn currency, on the day it
    /// took effect, from [`RateSource::Changeover`]. `id` identifies this
    /// copy; the fixed rate isn't in a book's price table.
    pub fn dated_rate(self, id: DatedRateId) -> DatedRate {
        DatedRate {
            id,
            date: self.date,
            from: self.successor,
            to: self.currency.code(),
            rate: self.rate(),
            source: RateSource::Changeover,
        }
    }

    /// `amount`, in this currency, converted into the successor `to` and
    /// rounded once in `rounding`, usually the book's (D-027): 100.00 DEM
    /// is 51.13 EUR.
    ///
    /// `to` is the successor with the decimal places the book stores for
    /// it. Fails if `amount` or `to` is in another currency, or the result
    /// doesn't fit into 64 bits.
    pub fn convert(
        self,
        amount: Money,
        to: Currency,
        rounding: RoundingMode,
    ) -> Result<Money, MoneyError> {
        if to.code() != self.successor {
            return Err(MoneyError::CurrencyMismatch {
                expected: self.successor,
                found: to.code(),
            });
        }
        amount.checked_div_rate(self.currency, self.rate(), to, rounding)
    }
}

impl CurrencyCode {
    /// The withdrawn currency with this code, if KassenSturz's table of
    /// withdrawn currencies lists it: the mark (DEM) or the lev (BGN), for
    /// example. `None` for a current currency, and for a code that was
    /// never one.
    ///
    /// The table is KassenSturz's own, in `data/withdrawn.txt`:
    /// `tools/update-currency-table.py` moves a currency there when it
    /// leaves the ISO 4217 table, with the decimal places it had, once its
    /// successor, rate and day are typed in (D-104, Q-065).
    pub fn withdrawn(self) -> Option<Withdrawn> {
        TABLE
            .binary_search_by(|entry| entry.currency.code().cmp(&self))
            .ok()
            .map(|index| TABLE[index])
    }
}

/// The table as text: comment lines starting with `#`, then one line per
/// currency, sorted by code, e.g. `DEM 2 EUR 1.95583 1999-01-01`: its code,
/// its decimal places, its successor, the rate and the day.
const TABLE_TEXT: &[u8] = include_bytes!("../data/withdrawn.txt");

/// The table, read while compiling: a mistake in the file stops the build.
static TABLE: [Withdrawn; count_entries(TABLE_TEXT)] = match parse_table(TABLE_TEXT) {
    Ok(table) => table,
    Err(TableError::MalformedLine) => {
        panic!("withdrawn.txt: a line is neither a comment nor like `DEM 2 EUR 1.95583 1999-01-01`")
    }
    Err(TableError::Unsorted) => {
        panic!("withdrawn.txt: the codes aren't sorted, or one appears twice")
    }
};

/// What's wrong with the table's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableError {
    /// A line isn't a code, one digit, a code, a rate above zero and a day,
    /// each after one space, like `DEM 2 EUR 1.95583 1999-01-01`.
    MalformedLine,
    /// The codes aren't sorted, or one appears twice.
    Unsorted,
}

/// Reads every currency line. The codes must be sorted and appear once each,
/// so the lookup can search the table.
const fn parse_table<const N: usize>(text: &[u8]) -> Result<[Withdrawn; N], TableError> {
    // A placeholder in every slot, until the lines below fill them all: N is
    // the number of lines.
    let Some(code) = code_at(b"AAA", 0) else {
        panic!("AAA is a valid code");
    };
    let Some(currency) = Currency::new(code, 0) else {
        panic!("0 decimal places are fine");
    };
    let placeholder = Withdrawn {
        currency,
        successor: code,
        rate_digits: 1,
        rate_decimal_places: 0,
        date: Date::constant(2000, 1, 1),
    };
    let mut table = [placeholder; N];
    let mut index = 0;
    let mut start = 0;
    while let Some((line, end, next)) = next_entry(text, start) {
        let entry = match parse_line(text, line, end) {
            Ok(entry) => entry,
            Err(error) => return Err(error),
        };
        if index > 0 && !sorts_before(table[index - 1].currency.code(), entry.currency.code()) {
            return Err(TableError::Unsorted);
        }
        table[index] = entry;
        index += 1;
        start = next;
    }
    Ok(table)
}

/// Reads the line from `start` to `end`: `DEM 2 EUR 1.95583 1999-01-01`.
const fn parse_line(text: &[u8], start: usize, end: usize) -> Result<Withdrawn, TableError> {
    // The shortest line has a one-digit rate: 3 + 1 + 1 + 1 + 3 + 1 + 1 + 1
    // + 10 bytes.
    if end - start < 22
        || text[start + 3] != b' '
        || !text[start + 4].is_ascii_digit()
        || text[start + 5] != b' '
        || text[start + 9] != b' '
        || text[end - 11] != b' '
    {
        return Err(TableError::MalformedLine);
    }
    let (Some(code), Some(successor)) = (code_at(text, start), code_at(text, start + 6)) else {
        return Err(TableError::MalformedLine);
    };
    if !sorts_before(code, successor) && !sorts_before(successor, code) {
        // Replaced by itself.
        return Err(TableError::MalformedLine);
    }
    let Some(currency) = Currency::new(code, text[start + 4] - b'0') else {
        return Err(TableError::MalformedLine);
    };
    let Some((rate_digits, rate_decimal_places)) = rate_between(text, start + 10, end - 11) else {
        return Err(TableError::MalformedLine);
    };
    let Some(date) = date_at(text, end - 10) else {
        return Err(TableError::MalformedLine);
    };
    Ok(Withdrawn {
        currency,
        successor,
        rate_digits,
        rate_decimal_places,
        date,
    })
}

/// The currency code in the three bytes at `at`.
const fn code_at(text: &[u8], at: usize) -> Option<CurrencyCode> {
    match CurrencyCode::from_bytes([text[at], text[at + 1], text[at + 2]]) {
        Ok(code) => Some(code),
        Err(_) => None,
    }
}

/// The rate from `start` to `end`, e.g. `1.95583`, as its digits and decimal
/// places: digits, optionally a point and more digits, above zero, with at
/// most 18 digits and 18 decimal places, as an [`ExchangeRate`] takes them.
const fn rate_between(text: &[u8], start: usize, end: usize) -> Option<(i64, u8)> {
    let mut digits: i64 = 0;
    let mut count = 0;
    let mut decimal_places: usize = 0;
    let mut point = false;
    let mut at = start;
    while at < end {
        let byte = text[at];
        if byte == b'.' {
            // One point, with digits on both sides.
            if point || at == start || at + 1 == end {
                return None;
            }
            point = true;
        } else if byte.is_ascii_digit() {
            if digits > 0 || byte != b'0' {
                count += 1;
            }
            if count > 18 {
                return None;
            }
            // Below 10^18 before, so this fits into 64 bits.
            digits = digits * 10 + (byte - b'0') as i64;
            if point {
                decimal_places += 1;
            }
        } else {
            return None;
        }
        at += 1;
    }
    if at == start || digits == 0 || decimal_places > MAX_DECIMAL_PLACES as usize {
        return None;
    }
    Some((digits, decimal_places as u8))
}

/// The day in the ten bytes at `at`, e.g. `1999-01-01`. One that doesn't
/// exist, such as 30 February, stops the build in [`Date::constant`].
const fn date_at(text: &[u8], at: usize) -> Option<Date> {
    if text[at + 4] != b'-' || text[at + 7] != b'-' {
        return None;
    }
    let (Some(year), Some(month), Some(day)) = (
        number_at(text, at, 4),
        number_at(text, at + 5, 2),
        number_at(text, at + 8, 2),
    ) else {
        return None;
    };
    if month < 1 || month > 12 || day < 1 || day > 31 {
        return None;
    }
    Some(Date::constant(year as i16, month as i8, day as i8))
}

/// The number written with `length` digits at `at`.
const fn number_at(text: &[u8], at: usize, length: usize) -> Option<u16> {
    let mut number: u16 = 0;
    let mut index = 0;
    while index < length {
        let byte = text[at + index];
        if !byte.is_ascii_digit() {
            return None;
        }
        number = number * 10 + (byte - b'0') as u16;
        index += 1;
    }
    Some(number)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(text: &str) -> CurrencyCode {
        CurrencyCode::new(text).expect("valid code")
    }

    #[test]
    fn the_mark_was_replaced_by_the_euro() {
        let mark = code("DEM").withdrawn().expect("the table lists the mark");
        assert_eq!(mark.currency(), Currency::new(code("DEM"), 2).unwrap());
        assert_eq!(mark.successor(), code("EUR"));
        assert_eq!(mark.rate(), "1.95583".parse().unwrap());
        assert_eq!(mark.date(), Date::constant(1999, 1, 1));
        let rate = mark.dated_rate(DatedRateId::generate());
        assert_eq!(
            (rate.from, rate.to, rate.rate, rate.date, rate.source),
            (
                code("EUR"),
                code("DEM"),
                mark.rate(),
                mark.date(),
                RateSource::Changeover
            )
        );
    }

    #[test]
    fn one_the_table_doesnt_list_is_what_the_user_says() {
        let leu = Currency::new(code("ROL"), 2).unwrap();
        let rate: ExchangeRate = "10000".parse().unwrap();
        let date = Date::constant(2005, 7, 1);
        let replaced = Withdrawn::new(leu, code("RON"), rate, date).expect("replaced");
        assert_eq!(
            (
                replaced.currency(),
                replaced.successor(),
                replaced.rate(),
                replaced.date()
            ),
            (leu, code("RON"), rate, date)
        );
        // 1,000,000.00 ROL are 100.00 RON.
        assert_eq!(
            replaced.convert(
                Money::from_minor_units(100_000_000, code("ROL")),
                Currency::new(code("RON"), 2).unwrap(),
                RoundingMode::HalfAwayFromZero
            ),
            Ok(Money::from_minor_units(10_000, code("RON")))
        );
        assert_eq!(Withdrawn::new(leu, code("ROL"), rate, date), None);
    }

    #[test]
    fn the_table_has_the_euros_predecessors_and_the_lev() {
        for (text, digits, rate, year) in [
            ("ATS", 2, "13.7603", 1999),
            ("BEF", 0, "40.3399", 1999),
            ("ITL", 0, "1936.27", 1999),
            ("GRD", 0, "340.750", 2001),
            ("CYP", 2, "0.585274", 2008),
            ("HRK", 2, "7.53450", 2023),
            ("BGN", 2, "1.95583", 2026),
        ] {
            let withdrawn = code(text).withdrawn().expect(text);
            assert_eq!(withdrawn.currency().minor_digits(), digits, "{text}");
            assert_eq!(withdrawn.rate(), rate.parse().unwrap(), "{text}");
            assert_eq!(withdrawn.date(), Date::constant(year, 1, 1), "{text}");
        }
        for text in ["EUR", "USD", "ABC"] {
            assert_eq!(code(text).withdrawn(), None, "{text}");
        }
    }

    #[test]
    fn every_withdrawn_currency_was_replaced_by_a_current_one() {
        // The build checked the order with `sorts_before`; the lookup
        // searches with `Ord`, so both orders have to agree.
        assert!(TABLE.len() >= 21, "only {} currencies", TABLE.len());
        for withdrawn in &TABLE {
            let currency = withdrawn.currency().code();
            assert_eq!(currency.withdrawn(), Some(*withdrawn), "{currency}");
            assert_eq!(currency.iso_minor_digits(), None, "{currency} is current");
            assert!(
                withdrawn.successor().iso_minor_digits().is_some(),
                "{currency}'s successor isn't current"
            );
            withdrawn.rate();
        }
    }

    #[test]
    fn amounts_convert_into_the_successor_rounded_once() {
        let mark = code("DEM").withdrawn().unwrap();
        let euro = Currency::new(code("EUR"), 2).unwrap();
        let convert = |minor_units, rounding| {
            mark.convert(
                Money::from_minor_units(minor_units, code("DEM")),
                euro,
                rounding,
            )
            .map(Money::minor_units)
        };
        // 100 DEM ÷ 1.95583 = 51.129188... EUR.
        assert_eq!(convert(10_000, RoundingMode::HalfAwayFromZero), Ok(5113));
        assert_eq!(convert(-10_000, RoundingMode::HalfAwayFromZero), Ok(-5113));
        assert_eq!(convert(10_000, RoundingMode::TowardZero), Ok(5112));
        // The lira had no decimal places: 1936.27 ITL are 1.00 EUR.
        let lira = code("ITL").withdrawn().unwrap();
        assert_eq!(
            lira.convert(
                Money::from_minor_units(1936, code("ITL")),
                euro,
                RoundingMode::HalfAwayFromZero
            ),
            Ok(Money::from_minor_units(100, code("EUR")))
        );
        assert_eq!(
            mark.convert(
                Money::from_minor_units(100, code("DEM")),
                Currency::new(code("USD"), 2).unwrap(),
                RoundingMode::HalfAwayFromZero
            ),
            Err(MoneyError::CurrencyMismatch {
                expected: code("EUR"),
                found: code("USD")
            })
        );
    }

    #[test]
    fn comments_and_windows_line_endings_are_fine() {
        const TEXT: &[u8] =
            b"# A comment\r\n\r\nDEM 2 EUR 1.95583 1999-01-01\r\nITL 0 EUR 1936.27 1999-01-01";
        assert_eq!(count_entries(TEXT), 2);
        let table = parse_table::<2>(TEXT).expect("a valid table");
        assert_eq!(table[0].currency().code(), code("DEM"));
        assert_eq!(
            (table[1].rate_digits, table[1].rate_decimal_places),
            (193_627, 2)
        );
    }

    #[test]
    fn the_codes_must_be_sorted_and_appear_once() {
        for text in [
            &b"ITL 0 EUR 1936.27 1999-01-01\nDEM 2 EUR 1.95583 1999-01-01\n"[..],
            b"DEM 2 EUR 1.95583 1999-01-01\nDEM 2 EUR 1.95583 1999-01-01\n",
        ] {
            assert_eq!(
                parse_table::<2>(text),
                Err(TableError::Unsorted),
                "{:?}",
                String::from_utf8_lossy(text)
            );
        }
    }

    #[test]
    fn lines_must_have_every_field_once() {
        for text in [
            &b"DEM 2 EUR 1.95583"[..],
            b"DEM 2 EUR 1.95583 1999-01-01 x",
            b"DEM  2 EUR 1.95583 1999-01-01",
            b"DEM 12 EUR 1.95583 1999-01-01",
            b"DEM x EUR 1.95583 1999-01-01",
            b"dem 2 EUR 1.95583 1999-01-01",
            b"DEM 2 eur 1.95583 1999-01-01",
            b"DEM 2 DEM 1.95583 1999-01-01",
            b"DEM 2 EUR 0 1999-01-01",
            b"DEM 2 EUR 0.000 1999-01-01",
            b"DEM 2 EUR -1.95583 1999-01-01",
            b"DEM 2 EUR 1,95583 1999-01-01",
            b"DEM 2 EUR .95583 1999-01-01",
            b"DEM 2 EUR 1. 1999-01-01",
            b"DEM 2 EUR 1.9.5 1999-01-01",
            b"DEM 2 EUR 1234567890123456789 1999-01-01",
            b"DEM 2 EUR 0.0000000000000000001 1999-01-01",
            b"DEM 2 EUR 1.95583 1999-1-01",
            b"DEM 2 EUR 1.95583 1999-13-01",
            b"DEM 2 EUR 1.95583 1999-01-00",
            b"DEM 2 EUR 1.95583 19990101xx",
        ] {
            assert_eq!(
                parse_table::<1>(text),
                Err(TableError::MalformedLine),
                "{:?}",
                String::from_utf8_lossy(text)
            );
        }
    }

    #[test]
    fn rates_keep_their_decimal_places_and_leading_zeros_dont_count() {
        let text = b"x 0.000000000000000001 x";
        assert_eq!(rate_between(text, 2, text.len() - 2), Some((1, 18)));
        let text = b"x 340.750 x";
        assert_eq!(rate_between(text, 2, text.len() - 2), Some((340_750, 3)));
    }
}
