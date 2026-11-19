// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Strict reading of a plugin's JSON answer. A key that isn't known, a
//! value of the wrong type or a malformed amount or date makes the whole
//! answer unusable ([`Invalid`]): a plugin that answers something else than
//! it should is never half believed (AGENTS.md §2).

use kassensturz_core::{CurrencyCode, Date};
use serde_json::{Map, Value};

/// The answer can't be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Invalid;

/// The answer as a JSON object.
pub(crate) fn object(bytes: &[u8]) -> Result<Map<String, Value>, Invalid> {
    match serde_json::from_slice(bytes) {
        Ok(Value::Object(object)) => Ok(object),
        _ => Err(Invalid),
    }
}

/// An object whose keys are among `known`.
pub(crate) struct Fields<'a> {
    object: &'a Map<String, Value>,
}

impl<'a> Fields<'a> {
    /// `value` as an object with only the keys in `known`.
    pub(crate) fn of(value: &'a Value, known: &[&str]) -> Result<Self, Invalid> {
        let object = value.as_object().ok_or(Invalid)?;
        Self::of_map(object, known)
    }

    /// `object`, with only the keys in `known`.
    pub(crate) fn of_map(object: &'a Map<String, Value>, known: &[&str]) -> Result<Self, Invalid> {
        if object.keys().any(|key| !known.contains(&key.as_str())) {
            return Err(Invalid);
        }
        Ok(Self { object })
    }

    /// The value of a key that must be there.
    pub(crate) fn required(&self, key: &str) -> Result<&'a Value, Invalid> {
        self.object.get(key).ok_or(Invalid)
    }

    /// The value of a key that may be left out. `null` is a wrong type,
    /// not a left-out key.
    pub(crate) fn optional(&self, key: &str) -> Option<&'a Value> {
        self.object.get(key)
    }

    /// A text that must be there.
    pub(crate) fn text(&self, key: &str) -> Result<&'a str, Invalid> {
        self.required(key)?.as_str().ok_or(Invalid)
    }

    /// A text that may be left out.
    pub(crate) fn optional_text(&self, key: &str) -> Result<Option<String>, Invalid> {
        self.optional(key)
            .map(|value| value.as_str().map(str::to_owned).ok_or(Invalid))
            .transpose()
    }

    /// An array that must be there.
    pub(crate) fn array(&self, key: &str) -> Result<&'a Vec<Value>, Invalid> {
        self.required(key)?.as_array().ok_or(Invalid)
    }
}

/// An amount in minor units, written as text: an optional `-` and digits,
/// without leading zeros, that fit in 64 bits. Text, since JSON numbers
/// are floating point in many languages.
pub(crate) fn minor_units(text: &str) -> Result<i64, Invalid> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let plain = !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && text != "-0";
    if !plain {
        return Err(Invalid);
    }
    text.parse().map_err(|_| Invalid)
}

/// A calendar date, written `YYYY-MM-DD`.
pub(crate) fn date(text: &str) -> Result<Date, Invalid> {
    let shape = text.len() == 10
        && text.bytes().enumerate().all(|(at, byte)| match at {
            4 | 7 => byte == b'-',
            _ => byte.is_ascii_digit(),
        });
    if !shape {
        return Err(Invalid);
    }
    text.parse().map_err(|_| Invalid)
}

/// A currency code: three capital letters. Whether it is a current ISO
/// 4217 currency is the caller's to check.
pub(crate) fn currency_code(text: &str) -> Result<CurrencyCode, Invalid> {
    CurrencyCode::new(text).map_err(|_| Invalid)
}

/// A file name extension: 1 to 16 lowercase ASCII letters and digits,
/// without the dot.
pub(crate) fn extension(text: &str) -> Result<&str, Invalid> {
    let fine = (1..=16).contains(&text.len())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    fine.then_some(text).ok_or(Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_are_plain_whole_numbers_as_text() {
        assert_eq!(minor_units("0"), Ok(0));
        assert_eq!(minor_units("-1234"), Ok(-1234));
        assert_eq!(minor_units("9223372036854775807"), Ok(i64::MAX));
        assert_eq!(minor_units("-9223372036854775808"), Ok(i64::MIN));
        for bad in [
            "",
            "-",
            "-0",
            "007",
            "+5",
            "1.5",
            "1e3",
            " 1",
            "1 ",
            "9223372036854775808",
        ] {
            assert_eq!(minor_units(bad), Err(Invalid), "{bad:?}");
        }
    }

    #[test]
    fn dates_are_written_year_month_day() {
        assert_eq!(
            date("2026-02-28").map(|d| d.to_string()),
            Ok("2026-02-28".to_owned())
        );
        for bad in [
            "2026-02-30",
            "2026-2-28",
            "26-02-28",
            "2026-02-28T00:00",
            "2026/02/28",
        ] {
            assert_eq!(date(bad), Err(Invalid), "{bad:?}");
        }
    }

    #[test]
    fn extensions_are_short_lowercase_words() {
        assert_eq!(extension("csv"), Ok("csv"));
        assert_eq!(extension("mt940"), Ok("mt940"));
        for bad in ["", ".csv", "CSV", "c sv", "abcdefghijklmnopq"] {
            assert_eq!(extension(bad), Err(Invalid), "{bad:?}");
        }
    }

    #[test]
    fn unknown_keys_are_refused() {
        let value = serde_json::json!({ "a": 1, "b": 2 });
        assert!(Fields::of(&value, &["a", "b"]).is_ok());
        assert!(Fields::of(&value, &["a"]).is_err());
        let fields = Fields::of(&value, &["a", "b"]).unwrap();
        assert_eq!(fields.optional_text("c"), Ok(None));
        assert_eq!(fields.optional_text("a"), Err(Invalid));
    }
}
