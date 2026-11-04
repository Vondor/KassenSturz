// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the colors (M7a.10, D-589): `#rrggbb` is read exactly, and
//! nothing outside it is guessed at.

use proptest::prelude::*;

use super::*;

#[test]
fn a_color_is_exactly_a_hash_and_six_hex_digits_in_any_case() {
    assert_eq!(Rgb::parse("#0072B2"), Some(Rgb::new(0, 0x72, 0xB2)));
    assert_eq!(Rgb::parse("#0072b2"), Some(Rgb::new(0, 0x72, 0xB2)));
    assert_eq!(Rgb::parse("#000000"), Some(Rgb::new(0, 0, 0)));
    assert_eq!(Rgb::parse("#FfFfFf"), Some(Rgb::new(255, 255, 255)));
    for text in [
        "",
        "#",
        "#fff",
        "0072B2",
        "#0072B",
        "#0072B22",
        "#0072G2",
        "# 0072B2",
        " #0072B2",
        "#0072B2 ",
        "#+072B2",
        "#-072B2",
        "rgb(0, 114, 178)",
        "blue",
        "#00 72B2",
    ] {
        assert_eq!(Rgb::parse(text), None, "{text:?}");
    }
}

#[test]
fn the_text_written_back_is_upper_case_with_a_hash() {
    assert_eq!(Rgb::new(0, 0x72, 0xB2).to_hex(), "#0072B2");
    assert_eq!(Rgb::new(255, 0, 7).to_hex(), "#FF0007");
}

#[test]
fn the_entry_on_the_settings_page_takes_a_code_or_three_numbers() {
    let expected = Ok(Rgb::new(0, 114, 178));
    for text in [
        "#0072B2",
        "#0072b2",
        " #0072B2 ",
        "0, 114, 178",
        "0,114,178",
        "0 114 178",
        "  0;114;178 ",
        "000, 0114, 178",
    ] {
        assert_eq!(Rgb::parse_entry(text), expected, "{text:?}");
    }
    for text in [
        "",
        "#fff",
        "0, 114",
        "0, 114, 178, 1",
        "a, b, c",
        "-1, 0, 0",
        "1.5, 0, 0",
        "0xFF, 0, 0",
        "#0072B",
        "blue",
        "+1, 2, 3",
        "1e2, 0, 0",
    ] {
        assert_eq!(Rgb::parse_entry(text), Err(ColorError::Format), "{text:?}");
    }
    for text in [
        "256, 0, 0",
        "0, 256, 0",
        "0, 0, 256",
        "999, 999, 999",
        "65536, 0, 0",
    ] {
        assert_eq!(Rgb::parse_entry(text), Err(ColorError::Part), "{text:?}");
    }
    // A part too long to be a sensible number is a format problem, not read.
    assert_eq!(Rgb::parse_entry("1234567, 0, 0"), Err(ColorError::Format));
}

#[test]
fn a_color_in_a_description_is_a_word_or_a_code() {
    assert_eq!(
        SpecColor::parse("bluish-green"),
        Some(SpecColor::Slot(PaletteName::BluishGreen))
    );
    assert_eq!(
        SpecColor::parse("#112233"),
        Some(SpecColor::Rgb(Rgb::new(0x11, 0x22, 0x33)))
    );
    for text in ["", "green", "Blue", "#fff", "rgb(1,2,3)"] {
        assert_eq!(SpecColor::parse(text), None, "{text:?}");
    }
}

proptest! {
    #[test]
    fn every_color_written_is_read_back(red: u8, green: u8, blue: u8) {
        let color = Rgb::new(red, green, blue);
        let text = color.to_hex();
        prop_assert_eq!(text.len(), 7);
        prop_assert_eq!(Rgb::parse(&text), Some(color));
        prop_assert_eq!(Rgb::parse(&text.to_lowercase()), Some(color));
        prop_assert_eq!(Rgb::parse_entry(&text), Ok(color));
        let numbers = format!("{red}, {green}, {blue}");
        prop_assert_eq!(Rgb::parse_entry(&numbers), Ok(color));
    }

    #[test]
    fn only_the_exact_form_is_a_color(text in any::<String>()) {
        let exact = text.len() == 7
            && text.starts_with('#')
            && text[1..].bytes().all(|b| b.is_ascii_hexdigit());
        prop_assert_eq!(Rgb::parse(&text).is_some(), exact);
        // A color that was read is written back to the same color.
        if let Some(color) = Rgb::parse(&text) {
            prop_assert_eq!(Rgb::parse(&color.to_hex()), Some(color));
        }
        // Nothing panics, whatever is typed in the entry.
        let _ = Rgb::parse_entry(&text);
    }

    #[test]
    fn three_numbers_are_read_by_value_and_each_one_is_checked(
        red in 0_u32..2000, green in 0_u32..2000, blue in 0_u32..2000,
    ) {
        let text = format!("{red}, {green}, {blue}");
        let fine = red < 256 && green < 256 && blue < 256;
        match Rgb::parse_entry(&text) {
            Ok(color) => {
                prop_assert!(fine);
                prop_assert_eq!(
                    (u32::from(color.red()), u32::from(color.green()), u32::from(color.blue())),
                    (red, green, blue)
                );
            }
            Err(error) => {
                prop_assert!(!fine);
                prop_assert_eq!(error, ColorError::Part);
            }
        }
    }
}
