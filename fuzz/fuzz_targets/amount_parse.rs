// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fuzz target: the reader of typed amounts (D-787, section C). It is part
//! of the Qt app crate, which a fuzz build can't compile, so its two source
//! files, which use nothing but `kassensturz-core`, are included by path.
//! Any text in any of five regional formats must give an error or an
//! amount, never panic, and an amount read must read back as itself when it
//! is written in the same format (money must never change by being typed).

#![no_main]
#![allow(dead_code)]

#[path = "../../crates/kassensturz-ui-bridge/src/format.rs"]
mod format;
#[path = "../../crates/kassensturz-ui-bridge/src/parse.rs"]
mod parse;

use format::{Affixes, AmountFormat};
use kassensturz_core::{CurrencyCode, Money};
use libfuzzer_sys::fuzz_target;
use parse::AmountReader;

/// The group separators a region is written with by people and programs:
/// the CLDR one, a no-break space, a narrow no-break space, a plain space,
/// an apostrophe. The reader is given one of them at a time.
const GROUPS: [[&str; 4]; 5] = [
    [".", "\u{a0}", "\u{202f}", " "],
    [",", "\u{a0}", "\u{202f}", " "],
    ["\u{66c}", ",", "\u{a0}", " "],
    ["\u{2019}", "'", "\u{a0}", "\u{202f}"],
    ["\u{202f}", "\u{a0}", " ", "."],
];

/// A region's format and its group separator.
fn region(selector: u8) -> (AmountFormat, &'static str) {
    let (format, _) = region_format(selector);
    (format, GROUPS[usize::from(selector % 5)][usize::from(selector / 5 % 4)])
}

fn region_format(selector: u8) -> (AmountFormat, &'static str) {
    match selector % 5 {
        0 => (
            AmountFormat {
                decimal_point: ",".into(),
                zero_digit: '0',
                positive: Affixes::spaced(String::new(), "\u{a0}€".into()),
                negative: Affixes::spaced("-".into(), "\u{a0}€".into()),
            },
            ".",
        ),
        1 => (
            AmountFormat {
                decimal_point: ".".into(),
                zero_digit: '0',
                positive: Affixes::spaced("$".into(), String::new()),
                negative: Affixes::spaced("($".into(), ")".into()),
            },
            ",",
        ),
        2 => (
            AmountFormat {
                decimal_point: "\u{66b}".into(),
                zero_digit: '\u{660}',
                positive: Affixes::default(),
                negative: Affixes::spaced("-".into(), String::new()),
            },
            "\u{66c}",
        ),
        3 => (
            AmountFormat {
                decimal_point: ".".into(),
                zero_digit: '0',
                positive: Affixes::spaced("CHF".into(), String::new()),
                negative: Affixes::spaced("CHF-".into(), String::new()),
            },
            "\u{2019}",
        ),
        _ => (
            AmountFormat {
                decimal_point: ",".into(),
                zero_digit: '0',
                positive: Affixes::spaced(String::new(), "\u{202f}€".into()),
                negative: Affixes::spaced("-".into(), "\u{202f}€".into()),
            },
            "\u{202f}",
        ),
    }
}

fuzz_target!(|data: &[u8]| {
    let [selector, digits, rest @ ..] = data else {
        return;
    };
    let (format, group) = region(*selector);
    let minor_digits = digits % 5;
    let currency = CurrencyCode::new("EUR").expect("EUR");
    let text = String::from_utf8_lossy(rest);
    let reader = AmountReader {
        format: &format,
        group_separator: group,
        currency,
        minor_digits,
    };
    let Ok(amount) = reader.read(&text) else {
        return;
    };
    let written = format.format(amount, minor_digits, |whole| {
        let digits = whole.to_string();
        let mut grouped = String::new();
        for (index, digit) in digits.chars().enumerate() {
            if index > 0 && (digits.len() - index) % 3 == 0 {
                grouped.push_str(group);
            }
            grouped.push(digit);
        }
        grouped
    });
    let again = reader.read(&written);
    assert_eq!(
        again.map(Money::minor_units),
        Ok(amount.minor_units()),
        "{text:?} read as {} but writes as {written:?}",
        amount.minor_units()
    );
});
