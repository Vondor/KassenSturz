// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! IBANs (ISO 13616): checking the one the user typed and reading the bank
//! code out of it, so KassenSturz can find the bank and ask only what that
//! bank needs (Q-035).

use crate::error::BankError;

/// The shortest IBAN any country issues (Norway, 15 characters).
const MIN_LENGTH: usize = 15;
/// The longest IBAN ISO 13616 allows.
const MAX_LENGTH: usize = 34;
/// The length of a German IBAN: `DEkk BBBB BBBB CCCC CCCC CC`.
const GERMAN_LENGTH: usize = 22;

/// An IBAN that checks out: capital letters and digits without spaces, with
/// the right check digits.
///
/// The bank code is read only for Germany, whose layout is known here: FinTS
/// is offered by banks in Germany (D-059), so an IBAN of another country
/// gets file imports only (`Setup::NoFints`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Iban {
    text: String,
}

impl Iban {
    /// Reads what the user typed: spaces are ignored and small letters
    /// taken as capitals. Fails with `bank.malformed-iban` for anything that
    /// isn't an IBAN: other characters, a wrong length or wrong check digits.
    pub fn parse(text: &str) -> Result<Self, BankError> {
        let text: String = text
            .chars()
            .filter(|character| !character.is_whitespace())
            .map(|character| character.to_ascii_uppercase())
            .collect();
        let valid_characters = text.bytes().all(|byte| byte.is_ascii_alphanumeric());
        let valid_length = (MIN_LENGTH..=MAX_LENGTH).contains(&text.len());
        if !valid_characters || !valid_length {
            return Err(BankError::MalformedIban);
        }
        let bytes = text.as_bytes();
        let valid_start = bytes[0].is_ascii_uppercase()
            && bytes[1].is_ascii_uppercase()
            && bytes[2].is_ascii_digit()
            && bytes[3].is_ascii_digit();
        if !valid_start || check_digits_remainder(&text) != 1 {
            return Err(BankError::MalformedIban);
        }
        if &text[..2] == "DE"
            && (text.len() != GERMAN_LENGTH || !text[4..].bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(BankError::MalformedIban);
        }
        Ok(Self { text })
    }

    /// The IBAN without spaces, e.g. `DE89370400440532013000`.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The IBAN in groups of four, as banks print it, e.g.
    /// `DE89 3704 0044 0532 0130 00`.
    pub fn grouped(&self) -> String {
        let mut grouped = String::with_capacity(self.text.len() + self.text.len() / 4);
        for (index, character) in self.text.chars().enumerate() {
            if index > 0 && index % 4 == 0 {
                grouped.push(' ');
            }
            grouped.push(character);
        }
        grouped
    }

    /// The country, as the IBAN's first two letters, e.g. `DE`.
    pub fn country(&self) -> &str {
        &self.text[..2]
    }

    /// The bank's national code, where the country's layout is known here:
    /// for Germany the eight-digit bank code (Bankleitzahl). `None` for
    /// other countries.
    pub fn bank_code(&self) -> Option<BankCode> {
        match self.country() {
            "DE" => Some(BankCode {
                country: "DE".to_owned(),
                code: self.text[4..12].to_owned(),
            }),
            _ => None,
        }
    }

    /// The account number within the bank, where the country's layout is
    /// known here: for Germany the ten digits after the bank code, which may
    /// start with zeros. `None` for other countries.
    pub fn account_number(&self) -> Option<&str> {
        match self.country() {
            "DE" => Some(&self.text[12..]),
            _ => None,
        }
    }
}

/// The remainder of ISO 7064's check (mod 97-10): the IBAN with its first
/// four characters moved to the end, letters as `A` = 10 to `Z` = 35, as one
/// number modulo 97. It is 1 for a correct IBAN. Computed digit by digit, so
/// no number ever grows beyond a few thousand.
fn check_digits_remainder(text: &str) -> u32 {
    let rearranged = text[4..].bytes().chain(text[..4].bytes());
    let mut remainder = 0_u32;
    for byte in rearranged {
        let value = if byte.is_ascii_digit() {
            u32::from(byte - b'0')
        } else {
            u32::from(byte - b'A') + 10
        };
        remainder = if value < 10 {
            (remainder * 10 + value) % 97
        } else {
            (remainder * 100 + value) % 97
        };
    }
    remainder
}

/// A bank's national code, e.g. a German bank code (Bankleitzahl), with its
/// country: the key the bank directory is looked up by (D-145).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BankCode {
    country: String,
    code: String,
}

impl BankCode {
    /// A bank code of `country` (two capital letters) as the bank list
    /// writes it, e.g. `DE` and `37040044`. Nothing is checked beyond
    /// capitalizing the country, so a code from a bank's own answer can be
    /// used as it is.
    pub fn new(country: &str, code: &str) -> Self {
        Self {
            country: country.to_ascii_uppercase(),
            code: code.to_owned(),
        }
    }

    /// The country, e.g. `DE`.
    pub fn country(&self) -> &str {
        &self.country
    }

    /// The code within the country, e.g. `37040044`.
    pub fn code(&self) -> &str {
        &self.code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SWIFT's example IBANs (D-055), from its IBAN Registry.
    const EXAMPLES: &[&str] = &[
        "DE89370400440532013000",
        "AT611904300234573201",
        "BE68539007547034",
        "CH9300762011623852957",
        "FR1420041010050500013M02606",
        "GB29NWBK60161331926819",
        "NL91ABNA0417164300",
        "NO9386011117947",
    ];

    #[test]
    fn the_registry_examples_check_out() {
        for example in EXAMPLES {
            let iban = Iban::parse(example).unwrap_or_else(|_| panic!("{example}"));
            assert_eq!(iban.as_str(), *example);
        }
    }

    #[test]
    fn spaces_and_small_letters_are_accepted() {
        let iban = Iban::parse(" de89 3704 0044 0532 0130 00 ").expect("an IBAN");
        assert_eq!(iban.as_str(), "DE89370400440532013000");
        assert_eq!(iban.grouped(), "DE89 3704 0044 0532 0130 00");
        assert_eq!(iban.country(), "DE");
    }

    #[test]
    fn a_wrong_check_digit_is_refused() {
        for wrong in [
            "DE88370400440532013000",
            "DE89370400440532013001",
            "GB29NWBK60161331926810",
        ] {
            assert_eq!(Iban::parse(wrong), Err(BankError::MalformedIban), "{wrong}");
        }
    }

    #[test]
    fn other_text_is_refused() {
        for wrong in [
            "",
            "DE89",
            "DE89-3704-0044-0532-0130-00",
            "DE8937040044053201300",
            "DE893704004405320130000",
            "D189370400440532013000",
            "DEA9370400440532013000",
            "DE89370400440532O13000",
            "DE89370400440532013000DE89370400440532013000",
        ] {
            assert_eq!(Iban::parse(wrong), Err(BankError::MalformedIban), "{wrong}");
        }
    }

    #[test]
    fn a_german_iban_gives_the_bank_code_and_account_number() {
        let iban = Iban::parse("DE89370400440532013000").expect("an IBAN");
        let code = iban.bank_code().expect("a German bank code");
        assert_eq!(code, BankCode::new("de", "37040044"));
        assert_eq!(code.country(), "DE");
        assert_eq!(code.code(), "37040044");
        assert_eq!(iban.account_number(), Some("0532013000"));
    }

    #[test]
    fn other_countries_give_no_bank_code() {
        let iban = Iban::parse("AT611904300234573201").expect("an IBAN");
        assert_eq!(iban.bank_code(), None);
        assert_eq!(iban.account_number(), None);
        assert_eq!(iban.country(), "AT");
    }
}
