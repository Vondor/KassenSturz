// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colors of the charts, which the user can change (M7a.10, D-589) and
//! whose number the user can change too (D-588).
//!
//! The app has one palette, a display setting like the number format
//! (D-002): it is kept per computer, never in a book, and it changes only how
//! charts look. It starts with eight colors, the Okabe-Ito set (D-283), and a
//! power user can add colors up to [`MAX_COLORS`] or remove them down to
//! [`MIN_COLORS`] (D-588). Every color is three whole numbers from 0 to 255
//! ([`Rgb`]); outside Rust a color is the text `#rrggbb` (D-065).
//!
//! A changed color gets a warning in plain words when it would be hard to see
//! on the background or hard to tell from the place next to it. The check is
//! whole-number arithmetic only: the standard colors were chosen on purpose
//! (D-283), so only colors the user changed are checked, and a warning never
//! stops anyone from keeping a color.

use kassensturz_report::spec::{MAX_LAYERS, Rgb};

/// How many colors the palette has at the start (D-283).
pub const DEFAULT_COLORS: usize = 8;

/// The fewest colors a palette may have: with one, no chart could tell its
/// parts apart, and below that the wrapping would have nothing to wrap to.
pub const MIN_COLORS: usize = 2;

/// The most colors a palette may have, a technical limit and no opinion on
/// what a person can tell apart: no chart has more than 200 colored parts
/// (4 layers of at most 50 series), so a place above 200 could never be used,
/// and each place is a row on the Settings page, which a measurement showed
/// takes seconds to rebuild at 1000 rows.
pub const MAX_COLORS: usize = 200;

// The most colors is the most colored parts a chart can have, and every one
// of those places is below where free colors start.
const _: () = {
    assert!(MAX_COLORS == MAX_LAYERS * crate::custom::MAX_SERIES);
    assert!(MAX_COLORS < crate::model::EXTRA_COLOR_BASE as usize);
};

/// How many of the places have a hand-picked standard color.
const PICKED: usize = 16;

/// The standard colors in palette order: the eight of the Okabe-Ito set
/// (blue, vermillion, bluish green, reddish purple, orange, sky blue, yellow
/// and grey, grey in place of black so the set also reads on a dark theme),
/// then eight more for a person who adds colors (wine red, green, teal,
/// purple, sand, indigo, rose and dark red). Only the first eight are the
/// color-blind-safe set (D-283).
pub const STANDARD_COLORS: [Rgb; PICKED] = [
    Rgb::new(0x00, 0x72, 0xB2),
    Rgb::new(0xD5, 0x5E, 0x00),
    Rgb::new(0x00, 0x9E, 0x73),
    Rgb::new(0xCC, 0x79, 0xA7),
    Rgb::new(0xE6, 0x9F, 0x00),
    Rgb::new(0x56, 0xB4, 0xE9),
    Rgb::new(0xF0, 0xE4, 0x42),
    Rgb::new(0x99, 0x99, 0x99),
    Rgb::new(0x88, 0x22, 0x55),
    Rgb::new(0x11, 0x77, 0x33),
    Rgb::new(0x44, 0xAA, 0x99),
    Rgb::new(0xAA, 0x44, 0x99),
    Rgb::new(0xDD, 0xCC, 0x77),
    Rgb::new(0x33, 0x22, 0x88),
    Rgb::new(0xCC, 0x66, 0x77),
    Rgb::new(0x66, 0x11, 0x00),
];

/// What is wrong with a changed color, by place (0 to 15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteWarning {
    /// The color is hard to see on the background (less than 3 to 1, the
    /// contrast that WCAG asks of the parts of a graphic).
    LowContrast {
        /// The place of the color.
        slot: u8,
    },
    /// The color is hard to tell from the place after or before it.
    TooAlike {
        /// The place of the color.
        slot: u8,
        /// The neighbouring place it is too close to.
        other: u8,
    },
}

/// The colors of the app's charts: [`MIN_COLORS`] to [`MAX_COLORS`] of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    colors: Vec<Rgb>,
}

impl Default for Palette {
    fn default() -> Self {
        Self::standard()
    }
}

impl Palette {
    /// The standard palette: the eight Okabe-Ito colors.
    #[must_use]
    pub fn standard() -> Self {
        Self {
            colors: STANDARD_COLORS[..DEFAULT_COLORS].to_vec(),
        }
    }

    /// The standard color of place `slot`, which is also the one an added
    /// place starts with: the picked color of the first sixteen places, and
    /// for a place above that a color whose hue is a golden angle (137
    /// degrees) further round the color wheel than the place before it, in
    /// two shades, so each added place starts different from the ones near
    /// it.
    #[must_use]
    pub fn standard_color(slot: usize) -> Rgb {
        match STANDARD_COLORS.get(slot) {
            Some(color) => *color,
            None => {
                let turn = slot - PICKED;
                let hue = (turn * 137 + 30) % 360;
                let lightness = if turn.is_multiple_of(2) { 45 } else { 62 };
                hsl(hue, 65, lightness)
            }
        }
    }

    /// The palette of `count` places (kept between [`MIN_COLORS`] and
    /// [`MAX_COLORS`]) from the texts a settings file keeps, one per place.
    /// A text that is not exactly `#rrggbb`, or none, gives the standard
    /// color of that place, so a file edited by hand never breaks the
    /// charts.
    #[must_use]
    pub fn from_texts<S: AsRef<str>>(count: usize, texts: &[S]) -> Self {
        let count = count.clamp(MIN_COLORS, MAX_COLORS);
        let colors = (0..count)
            .map(|slot| {
                texts
                    .get(slot)
                    .and_then(|text| Rgb::parse(text.as_ref()))
                    .unwrap_or_else(|| Self::standard_color(slot))
            })
            .collect();
        Self { colors }
    }

    /// How many colors the palette has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.colors.len()
    }

    /// Never true: a palette has at least [`MIN_COLORS`] places. Here for
    /// the sake of `len`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    /// The color of series or slice `slot`: places beyond the palette start
    /// again at the first.
    #[must_use]
    pub fn color(&self, slot: usize) -> Rgb {
        self.colors[slot % self.colors.len()]
    }

    /// The colors as `#RRGGBB` text, which is what QML gets.
    #[must_use]
    pub fn texts(&self) -> Vec<String> {
        self.colors.iter().map(|color| color.to_hex()).collect()
    }

    /// Sets the color of `slot`. Returns whether the place exists.
    pub fn set(&mut self, slot: usize, color: Rgb) -> bool {
        match self.colors.get_mut(slot) {
            Some(place) => {
                *place = color;
                true
            }
            None => false,
        }
    }

    /// Puts `slot` back to its standard color. Returns whether the place
    /// exists.
    pub fn reset(&mut self, slot: usize) -> bool {
        self.set(slot, Self::standard_color(slot))
    }

    /// Puts the palette back to the eight standard colors.
    pub fn reset_all(&mut self) {
        *self = Self::standard();
    }

    /// Adds a place at the end with its standard color. Returns whether
    /// there was room (there are at most [`MAX_COLORS`] places).
    pub fn add(&mut self) -> bool {
        if self.colors.len() >= MAX_COLORS {
            return false;
        }
        self.colors.push(Self::standard_color(self.colors.len()));
        true
    }

    /// Removes place `slot`; the places after it move up by one. Returns
    /// whether it was removed: a palette keeps at least [`MIN_COLORS`]
    /// places, and the place must exist.
    pub fn remove(&mut self, slot: usize) -> bool {
        if self.colors.len() <= MIN_COLORS || slot >= self.colors.len() {
            return false;
        }
        self.colors.remove(slot);
        true
    }

    /// Whether `slot` has its standard color.
    #[must_use]
    pub fn is_standard(&self, slot: usize) -> bool {
        self.colors.get(slot) == Some(&Self::standard_color(slot))
    }

    /// Whether this is the standard palette: eight places, each with its
    /// standard color.
    #[must_use]
    pub fn all_standard(&self) -> bool {
        self.colors == STANDARD_COLORS[..DEFAULT_COLORS]
    }

    /// The warnings for the colors the user changed: too little contrast
    /// with `background`, and too close to a neighbouring place. Standard
    /// colors are not checked (D-283), but a changed color is also checked
    /// against a standard neighbour.
    #[must_use]
    pub fn warnings(&self, background: Rgb) -> Vec<PaletteWarning> {
        let mut found = Vec::new();
        for (slot, color) in self.colors.iter().enumerate() {
            if !self.is_standard(slot) && !contrast_is_enough(*color, background) {
                found.push(PaletteWarning::LowContrast {
                    slot: slot_u8(slot),
                });
            }
        }
        for (slot, pair) in self.colors.windows(2).enumerate() {
            let other = slot + 1;
            if (!self.is_standard(slot) || !self.is_standard(other)) && too_alike(pair[0], pair[1])
            {
                found.push(PaletteWarning::TooAlike {
                    slot: slot_u8(slot),
                    other: slot_u8(other),
                });
            }
        }
        found
    }
}

/// A color from hue (0 to 359 degrees), saturation and lightness (0 to 100
/// percent), in whole numbers (the workspace denies floating point). It is
/// the usual conversion with every part in ten-thousandths: the chroma
/// `(1 - |2L - 1|) * S`, the second largest part `chroma * (1 - |(H / 60) mod
/// 2 - 1|)` and the smallest `L - chroma / 2`.
fn hsl(hue: usize, saturation: usize, lightness: usize) -> Rgb {
    let chroma = (100 - (2 * lightness).abs_diff(100)) * saturation;
    let toward = 60_usize.abs_diff(hue % 120);
    let second = chroma * (60 - toward) / 60;
    let floor = lightness * 100 - chroma / 2;
    let (red, green, blue) = match hue / 60 {
        0 => (chroma, second, 0),
        1 => (second, chroma, 0),
        2 => (0, chroma, second),
        3 => (0, second, chroma),
        4 => (second, 0, chroma),
        _ => (chroma, 0, second),
    };
    let part = |value: usize| u8::try_from(((value + floor) * 255 + 5_000) / 10_000).unwrap_or(255);
    Rgb::new(part(red), part(green), part(blue))
}

/// A place as the small number it is (below 256).
fn slot_u8(slot: usize) -> u8 {
    u8::try_from(slot).unwrap_or(u8::MAX)
}

/// How much brighter the lighter of two colors is than the darker one, 3 to
/// 1 or more, by WCAG's relative luminance in whole numbers: a part's linear
/// value in millionths from [`LINEAR`], luminance `0.2126 R + 0.7152 G +
/// 0.0722 B`, and `(light + 0.05) >= 3 * (dark + 0.05)`.
#[must_use]
pub fn contrast_is_enough(color: Rgb, background: Rgb) -> bool {
    let (one, other) = (luminance(color), luminance(background));
    let (light, dark) = if one >= other {
        (one, other)
    } else {
        (other, one)
    };
    light + 50_000 >= 3 * (dark + 50_000)
}

/// The relative luminance in millionths (0 black to 1 000 000 white).
fn luminance(color: Rgb) -> u64 {
    let linear = |part: u8| u64::from(LINEAR[usize::from(part)]);
    (2126 * linear(color.red()) + 7152 * linear(color.green()) + 722 * linear(color.blue()))
        / 10_000
}

/// Whether two colors are so close that a bar or a line in one is hard to
/// tell from the other. The distance is the "redmean" approximation of how
/// different two colors look, in whole numbers: its square, in 1/256 units,
/// below 120 squared. The standard neighbours are at least 245 apart.
#[must_use]
pub fn too_alike(one: Rgb, other: Rgb) -> bool {
    let diff = |a: u8, b: u8| i64::from(a) - i64::from(b);
    let mean_red = (i64::from(one.red()) + i64::from(other.red())) / 2;
    let (dr, dg, db) = (
        diff(one.red(), other.red()),
        diff(one.green(), other.green()),
        diff(one.blue(), other.blue()),
    );
    // (2 + r/256) dr^2 + 4 dg^2 + (2 + (255 - r)/256) db^2, times 256.
    let squared = (512 + mean_red) * dr * dr + 1024 * dg * dg + (767 - mean_red) * db * db;
    squared < 120 * 120 * 256
}

/// The linear value of an sRGB part (0 to 255) in millionths, as WCAG
/// defines it (the table is computed once; there is no floating point in
/// the program).
const LINEAR: [u32; 256] = [
    0, 304, 607, 911, 1214, 1518, 1821, 2125, 2428, 2732, 3035, 3347, 3677, 4025, 4391, 4777, 5182,
    5605, 6049, 6512, 6995, 7499, 8023, 8568, 9134, 9721, 10330, 10960, 11612, 12286, 12983, 13702,
    14444, 15209, 15996, 16807, 17642, 18500, 19382, 20289, 21219, 22174, 23153, 24158, 25187,
    26241, 27321, 28426, 29557, 30713, 31896, 33105, 34340, 35601, 36889, 38204, 39546, 40915,
    42311, 43735, 45186, 46665, 48172, 49707, 51269, 52861, 54480, 56128, 57805, 59511, 61246,
    63010, 64803, 66626, 68478, 70360, 72272, 74214, 76185, 78187, 80220, 82283, 84376, 86500,
    88656, 90842, 93059, 95307, 97587, 99899, 102242, 104616, 107023, 109462, 111932, 114435,
    116971, 119538, 122139, 124772, 127438, 130136, 132868, 135633, 138432, 141263, 144128, 147027,
    149960, 152926, 155926, 158961, 162029, 165132, 168269, 171441, 174647, 177888, 181164, 184475,
    187821, 191202, 194618, 198069, 201556, 205079, 208637, 212231, 215861, 219526, 223228, 226966,
    230740, 234551, 238398, 242281, 246201, 250158, 254152, 258183, 262251, 266356, 270498, 274677,
    278894, 283149, 287441, 291771, 296138, 300544, 304987, 309469, 313989, 318547, 323143, 327778,
    332452, 337164, 341914, 346704, 351533, 356400, 361307, 366253, 371238, 376262, 381326, 386429,
    391572, 396755, 401978, 407240, 412543, 417885, 423268, 428690, 434154, 439657, 445201, 450786,
    456411, 462077, 467784, 473531, 479320, 485150, 491021, 496933, 502886, 508881, 514918, 520996,
    527115, 533276, 539479, 545724, 552011, 558340, 564712, 571125, 577580, 584078, 590619, 597202,
    603827, 610496, 617207, 623960, 630757, 637597, 644480, 651406, 658375, 665387, 672443, 679542,
    686685, 693872, 701102, 708376, 715694, 723055, 730461, 737910, 745404, 752942, 760525, 768151,
    775822, 783538, 791298, 799103, 806952, 814847, 822786, 830770, 838799, 846873, 854993, 863157,
    871367, 879622, 887923, 896269, 904661, 913099, 921582, 930111, 938686, 947307, 955973, 964686,
    973445, 982251, 991102, 1000000,
];
