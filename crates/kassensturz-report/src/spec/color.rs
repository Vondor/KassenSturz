// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Colors by their red, green and blue parts (M7a.10, D-589).
//!
//! A color is three whole numbers from 0 to 255, always checked here and
//! never read from a floating point number (D-065). Outside Rust a color is
//! text, `#rrggbb`: that is what the settings file keeps, what QML gets and
//! what a chart description writes. The text is not trusted: only exactly
//! `#` and six hex digits (any case) are a color, so nothing else (`#fff`,
//! `rgb(1, 2, 3)`, a CSS name, a long or odd string) is guessed at.

use super::model::PaletteName;

/// A color, each part 0 to 255.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

/// Why the text of a color could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorError {
    /// The text is not a color in any form that is accepted here.
    Format,
    /// A part is more than 255 (in the form `red, green, blue`).
    Part,
}

impl Rgb {
    /// A color from its three parts.
    #[must_use]
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    /// The red part.
    #[must_use]
    pub const fn red(self) -> u8 {
        self.red
    }

    /// The green part.
    #[must_use]
    pub const fn green(self) -> u8 {
        self.green
    }

    /// The blue part.
    #[must_use]
    pub const fn blue(self) -> u8 {
        self.blue
    }

    /// Reads exactly `#rrggbb`: a `#`, then six hex digits, any case. This is
    /// the form of a chart description and of the settings file.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?.as_bytes();
        if digits.len() != 6 {
            return None;
        }
        let part = |at: usize| -> Option<u8> {
            let high = hex_digit(digits[at])?;
            let low = hex_digit(digits[at + 1])?;
            Some(high * 16 + low)
        };
        Some(Self::new(part(0)?, part(2)?, part(4)?))
    }

    /// Reads what a person types in the palette entry of the Settings page:
    /// `#rrggbb`, or three whole numbers from 0 to 255 for red, green and
    /// blue, separated by commas, semicolons or spaces.
    ///
    /// # Errors
    ///
    /// [`ColorError::Part`] when a part of three numbers is above 255, else
    /// [`ColorError::Format`].
    pub fn parse_entry(text: &str) -> Result<Self, ColorError> {
        let text = text.trim();
        if text.starts_with('#') {
            return Self::parse(text).ok_or(ColorError::Format);
        }
        let mut parts = text
            .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
            .filter(|part| !part.is_empty());
        let (Some(red), Some(green), Some(blue), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(ColorError::Format);
        };
        let mut channels = [0_u8; 3];
        let mut too_big = false;
        for (channel, part) in channels.iter_mut().zip([red, green, blue]) {
            if part.is_empty() || part.len() > 6 || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ColorError::Format);
            }
            match part.parse::<u16>().map(u8::try_from) {
                Ok(Ok(value)) => *channel = value,
                _ => too_big = true,
            }
        }
        if too_big {
            return Err(ColorError::Part);
        }
        Ok(Self::new(channels[0], channels[1], channels[2]))
    }

    /// The color as `#RRGGBB`, the text that is kept and passed to QML.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.red, self.green, self.blue)
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// A color in a chart description: one of the palette's eight places by its
/// word, or free red, green and blue as `#rrggbb` (for power users, Q-410).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecColor {
    /// A place of the app's palette, which the user can change in Settings.
    Slot(PaletteName),
    /// Exactly this color.
    Rgb(Rgb),
}

impl SpecColor {
    /// Reads a palette word or `#rrggbb`; nothing else is a color here.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        PaletteName::from_word(text)
            .map(Self::Slot)
            .or_else(|| Rgb::parse(text).map(Self::Rgb))
    }
}
