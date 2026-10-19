// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Currencies: ISO 4217 codes, and KassenSturz's own ISO 4217 table, which
//! says how many decimal places each currency has (D-027, D-062).

use core::fmt;

/// An ISO 4217 currency code such as `EUR`: exactly three ASCII capital
/// letters.
///
/// Only the form is checked here. Which currencies a book uses, and how many
/// decimal places each one has, is stored in the book (D-027).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CurrencyCode([u8; 3]);

impl CurrencyCode {
    /// Checks and wraps a currency code.
    pub const fn new(code: &str) -> Result<Self, CurrencyCodeError> {
        match *code.as_bytes() {
            [a, b, c] => Self::from_bytes([a, b, c]),
            _ => Err(CurrencyCodeError::Malformed),
        }
    }

    const fn from_bytes(bytes: [u8; 3]) -> Result<Self, CurrencyCodeError> {
        let [a, b, c] = bytes;
        if a.is_ascii_uppercase() && b.is_ascii_uppercase() && c.is_ascii_uppercase() {
            Ok(Self(bytes))
        } else {
            Err(CurrencyCodeError::Malformed)
        }
    }

    /// The code as text, e.g. `"EUR"`.
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.0).expect("CurrencyCode::new accepts only ASCII letters")
    }

    /// How many decimal places ISO 4217 gives this currency: 2 for the euro,
    /// 0 for the yen, 3 for the Bahraini dinar. `None` if KassenSturz's table
    /// doesn't list it: it isn't a current currency, or ISO 4217 gives it no
    /// decimal places, as for gold (XAU).
    ///
    /// Only for a currency's first use in a book: from then on the decimal
    /// places stored in the book count, so a newer table can never rescale
    /// amounts already stored (D-027). The table is KassenSturz's own, in
    /// `data/iso4217.txt`: the codes Debian's iso-codes lists, with the decimal
    /// places ISO 4217 gives them (D-062, D-077).
    pub fn iso_minor_digits(self) -> Option<u8> {
        TABLE
            .binary_search_by(|&(code, _)| code.cmp(&self))
            .ok()
            .map(|index| TABLE[index].1)
    }
}

impl fmt::Debug for CurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CurrencyCode({})", self.as_str())
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A currency as a book uses it: its code and its number of decimal places,
/// fixed when the book first used it (D-027).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Currency {
    code: CurrencyCode,
    minor_digits: u8,
}

impl Currency {
    /// The most decimal places a currency may have: 10^18 still fits into 64
    /// bits. ISO 4217 currencies use at most 4.
    pub const MAX_MINOR_DIGITS: u8 = 18;

    /// `code` with `minor_digits` decimal places, e.g. EUR with 2. `None` if
    /// `minor_digits` is above [`Currency::MAX_MINOR_DIGITS`].
    pub const fn new(code: CurrencyCode, minor_digits: u8) -> Option<Self> {
        if minor_digits > Self::MAX_MINOR_DIGITS {
            None
        } else {
            Some(Self { code, minor_digits })
        }
    }

    /// The ISO 4217 code.
    pub const fn code(self) -> CurrencyCode {
        self.code
    }

    /// The number of decimal places: 2 for the euro, 0 for the yen.
    pub const fn minor_digits(self) -> u8 {
        self.minor_digits
    }
}

/// Why a text isn't a currency code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurrencyCodeError {
    /// The text isn't exactly three ASCII capital letters.
    Malformed,
}

/// KassenSturz's ISO 4217 table as text: comment lines starting with `#`,
/// then one line per currency with its code and its decimal places, e.g.
/// `EUR 2`, sorted by code. `tools/update-currency-table.py` writes it from
/// the codes Debian's iso-codes lists, keeping the decimal places the table
/// already has (D-062, D-077).
const TABLE_TEXT: &[u8] = include_bytes!("../data/iso4217.txt");

/// The table, read while compiling: a mistake in the file stops the build.
static TABLE: [(CurrencyCode, u8); count_entries(TABLE_TEXT)] = match parse_table(TABLE_TEXT) {
    Ok(table) => table,
    Err(TableError::MalformedLine) => {
        panic!("iso4217.txt: a line is neither a comment nor like `EUR 2`")
    }
    Err(TableError::Unsorted) => {
        panic!("iso4217.txt: the codes aren't sorted, or one appears twice")
    }
};

/// What's wrong with the table's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableError {
    /// A line is neither a comment nor a code and one digit, like `EUR 2`.
    MalformedLine,
    /// The codes aren't sorted, or one appears twice.
    Unsorted,
}

/// Finds the next currency line at or after `start`, skipping comments and
/// empty lines. Returns where its text starts and ends, and where the line
/// after it starts.
const fn next_entry(text: &[u8], mut start: usize) -> Option<(usize, usize, usize)> {
    while start < text.len() {
        let mut end = start;
        while end < text.len() && text[end] != b'\n' {
            end += 1;
        }
        let next = end + 1;
        // A checkout with Windows line endings (Git's core.autocrlf) works too.
        if end > start && text[end - 1] == b'\r' {
            end -= 1;
        }
        if end > start && text[start] != b'#' {
            return Some((start, end, next));
        }
        start = next;
    }
    None
}

/// How many currencies the table lists.
const fn count_entries(text: &[u8]) -> usize {
    let mut count = 0;
    let mut start = 0;
    while let Some((_, _, next)) = next_entry(text, start) {
        count += 1;
        start = next;
    }
    count
}

/// Reads every currency line: a code, a space and one digit. The codes must be
/// sorted and appear once each, so the lookup can search the table.
const fn parse_table<const N: usize>(text: &[u8]) -> Result<[(CurrencyCode, u8); N], TableError> {
    // A placeholder in every slot, until the lines below fill them all: N is
    // the number of lines.
    let mut table = [(CurrencyCode(*b"AAA"), 0); N];
    let mut index = 0;
    let mut start = 0;
    while let Some((line, end, next)) = next_entry(text, start) {
        if end - line != 5 || text[line + 3] != b' ' || !text[line + 4].is_ascii_digit() {
            return Err(TableError::MalformedLine);
        }
        let Ok(code) = CurrencyCode::from_bytes([text[line], text[line + 1], text[line + 2]])
        else {
            return Err(TableError::MalformedLine);
        };
        if index > 0 && !sorts_before(table[index - 1].0, code) {
            return Err(TableError::Unsorted);
        }
        table[index] = (code, text[line + 4] - b'0');
        index += 1;
        start = next;
    }
    Ok(table)
}

/// Whether `a` comes before `b` in alphabetical order, the order `Ord` uses.
const fn sorts_before(a: CurrencyCode, b: CurrencyCode) -> bool {
    let [a0, a1, a2] = a.0;
    let [b0, b1, b2] = b.0;
    u32::from_be_bytes([0, a0, a1, a2]) < u32::from_be_bytes([0, b0, b1, b2])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(text: &str) -> CurrencyCode {
        CurrencyCode::new(text).expect("valid code")
    }

    #[test]
    fn currency_code_accepts_three_capital_letters() {
        assert_eq!(
            CurrencyCode::new("EUR").map(|c| c.to_string()),
            Ok("EUR".into())
        );
    }

    #[test]
    fn currency_code_rejects_anything_else() {
        for text in ["", "EU", "EURO", "eur", "E1R", "EÜR", " EU"] {
            assert_eq!(
                CurrencyCode::new(text),
                Err(CurrencyCodeError::Malformed),
                "{text:?}"
            );
        }
    }

    #[test]
    fn the_table_has_iso_4217s_decimal_places() {
        for (text, digits) in [
            ("EUR", 2),
            ("USD", 2),
            ("CHF", 2),
            ("JPY", 0),
            ("KRW", 0),
            ("BHD", 3),
            ("KWD", 3),
            ("CLF", 4),
        ] {
            assert_eq!(code(text).iso_minor_digits(), Some(digits), "{text}");
        }
    }

    #[test]
    fn the_table_lists_only_current_currencies_with_decimal_places() {
        // ISO 4217 gives gold, the IMF's special drawing right, the testing
        // code and "no currency" no decimal places. The lev ended when
        // Bulgaria adopted the euro in 2026, the mark in 2002, and "ABC" was
        // never a currency.
        for text in ["XAU", "XDR", "XTS", "XXX", "BGN", "DEM", "ABC"] {
            assert_eq!(code(text).iso_minor_digits(), None, "{text}");
        }
    }

    #[test]
    fn every_currency_in_the_table_can_be_looked_up() {
        // The build checked the order with `sorts_before`; the lookup searches
        // with `Ord`, so both orders have to agree.
        assert!(TABLE.len() > 100, "only {} currencies", TABLE.len());
        for &(code, digits) in &TABLE {
            assert_eq!(code.iso_minor_digits(), Some(digits), "{code}");
        }
    }

    #[test]
    fn comments_empty_lines_and_windows_line_endings_are_fine() {
        const TEXT: &[u8] = b"# A comment\r\n\r\nAAA 0\r\n#\nEUR 2\nJPY 0";
        assert_eq!(count_entries(TEXT), 3);
        assert_eq!(
            parse_table(TEXT),
            Ok([(code("AAA"), 0), (code("EUR"), 2), (code("JPY"), 0)])
        );
    }

    #[test]
    fn the_codes_must_be_sorted_and_appear_once() {
        for text in [&b"USD 2\nEUR 2\n"[..], b"EUR 2\nEUR 2\n"] {
            assert_eq!(
                parse_table::<2>(text),
                Err(TableError::Unsorted),
                "{:?}",
                String::from_utf8_lossy(text)
            );
        }
    }

    #[test]
    fn lines_must_be_a_code_and_one_digit() {
        for text in [
            &b"EUR  2"[..],
            b"EUR 12",
            b"EUR2",
            b"EUR x",
            b" EUR 2",
            b"eur 2",
            b"E1R 2",
        ] {
            assert_eq!(
                parse_table::<1>(text),
                Err(TableError::MalformedLine),
                "{:?}",
                String::from_utf8_lossy(text)
            );
        }
    }
}
