// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A theme plugin's data (D-756, D-925): the colours of a light and a dark
//! look, and optionally a font and a corner radius. It is data only, in the
//! custom section [`THEME_SECTION`] of the plugin file, and KassenSturz draws
//! everything with it; a theme can't run anything.
//!
//! A theme that would make text hard to read is refused whole: text and the
//! accent must each stand out from the background by at least
//! [`MIN_CONTRAST`] (WCAG 2.2 level AA for normal text), so a theme can never
//! hide an amount or a warning.

use serde_json::{Map, Value};

use crate::PluginError;
use crate::manifest::text;

/// The custom section that holds a theme's data.
pub const THEME_SECTION: &str = "kassensturz-theme";
/// The theme format this KassenSturz reads.
pub const THEME_VERSION: i64 = 1;
/// The most bytes a theme's data may have.
pub const MAX_THEME_BYTES: usize = 4 * 1024;
/// The largest corner radius a theme may ask for, in logical pixels.
pub const MAX_CORNER_RADIUS: u8 = 16;
/// The most characters of a font's family name.
pub const MAX_FONT_CHARS: usize = 80;
/// The least contrast between the background and text or accent, as
/// numerator and denominator: 4.5 to 1.
pub const MIN_CONTRAST: (u64, u64) = (9, 2);

/// The keys of the theme object.
const KEYS: [&str; 5] = ["theme_version", "light", "dark", "font", "corner_radius"];
/// The keys of one look.
const COLOUR_KEYS: [&str; 3] = ["background", "text", "accent"];

/// A colour, 8 bits per channel, in sRGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    /// Red, 0 to 255.
    pub red: u8,
    /// Green, 0 to 255.
    pub green: u8,
    /// Blue, 0 to 255.
    pub blue: u8,
}

impl Rgb {
    /// Reads `#RRGGBB`, upper or lower case; nothing else.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
        Some(Self {
            red: channel(0)?,
            green: channel(2)?,
            blue: channel(4)?,
        })
    }

    /// The colour as `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }

    /// Its relative luminance as WCAG defines it, times 10¹⁰, in integers
    /// only.
    pub fn luminance(self) -> u64 {
        let linear = |channel: u8| u64::from(LINEAR[usize::from(channel)]);
        2126 * linear(self.red) + 7152 * linear(self.green) + 722 * linear(self.blue)
    }

    /// Whether `self` and `other` differ in contrast by at least `ratio`
    /// (numerator, denominator), WCAG's `(L1 + 0.05) / (L2 + 0.05)`.
    pub fn contrasts_with(self, other: Self, ratio: (u64, u64)) -> bool {
        // 0.05 at the luminance's scale of 10¹⁰.
        const OFFSET: u64 = 500_000_000;
        let (a, b) = (self.luminance(), other.luminance());
        let (high, low) = if a >= b { (a, b) } else { (b, a) };
        ratio.1 * (high + OFFSET) >= ratio.0 * (low + OFFSET)
    }
}

/// The colours of one look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colours {
    /// Behind everything.
    pub background: Rgb,
    /// Text and icons.
    pub text: Rgb,
    /// Selections, links, focus and the parts that stand out.
    pub accent: Rgb,
}

/// A theme plugin's data, read and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// The light look: dark text on a light background.
    pub light: Colours,
    /// The dark look: light text on a dark background.
    pub dark: Colours,
    /// A font family the app uses if this computer has it.
    pub font: Option<String>,
    /// The corner radius of KassenSturz's own boxes, in logical pixels.
    pub corner_radius: Option<u8>,
}

impl Theme {
    /// Reads and checks a theme's data. Refuses it whole on the first
    /// problem.
    pub fn parse(bytes: &[u8]) -> Result<Self, PluginError> {
        if bytes.len() > MAX_THEME_BYTES {
            return Err(PluginError::ThemeTooBig {
                max_bytes: MAX_THEME_BYTES,
            });
        }
        let value: Value =
            serde_json::from_slice(bytes).map_err(|_| PluginError::ThemeMalformed)?;
        let Value::Object(object) = value else {
            return Err(PluginError::ThemeMalformed);
        };
        let version = match object.get("theme_version") {
            None => return Err(invalid("missing", "theme_version")),
            Some(value) => value
                .as_i64()
                .ok_or_else(|| invalid("wrong-type", "theme_version"))?,
        };
        if version != THEME_VERSION {
            return Err(PluginError::UnsupportedThemeVersion { version });
        }
        if let Some(key) = object.keys().find(|key| !KEYS.contains(&key.as_str())) {
            return Err(invalid("unknown-key", key));
        }
        let light = colours(&object, "light")?;
        let dark = colours(&object, "dark")?;
        if light.background.luminance() <= light.text.luminance() {
            return Err(invalid("wrong-way", "light"));
        }
        if dark.background.luminance() >= dark.text.luminance() {
            return Err(invalid("wrong-way", "dark"));
        }
        let font = match object.get("font") {
            None => None,
            Some(Value::String(family)) => {
                Some(text(family, "font", MAX_FONT_CHARS).map_err(theme_error)?)
            }
            Some(_) => return Err(invalid("wrong-type", "font")),
        };
        let corner_radius = match object.get("corner_radius") {
            None => None,
            Some(value) => {
                let radius = value
                    .as_u64()
                    .ok_or_else(|| invalid("wrong-type", "corner_radius"))?;
                Some(
                    u8::try_from(radius)
                        .ok()
                        .filter(|radius| *radius <= MAX_CORNER_RADIUS)
                        .ok_or_else(|| invalid("out-of-range", "corner_radius"))?,
                )
            }
        };
        Ok(Self {
            light,
            dark,
            font,
            corner_radius,
        })
    }
}

/// One look, `light` or `dark`, with its contrast checked.
fn colours(object: &Map<String, Value>, look: &str) -> Result<Colours, PluginError> {
    let Some(value) = object.get(look) else {
        return Err(invalid("missing", look));
    };
    let Value::Object(colours) = value else {
        return Err(invalid("wrong-type", look));
    };
    if let Some(key) = colours
        .keys()
        .find(|key| !COLOUR_KEYS.contains(&key.as_str()))
    {
        return Err(invalid("unknown-key", &format!("{look}.{key}")));
    }
    let colour = |key: &str| {
        let path = format!("{look}.{key}");
        match colours.get(key) {
            None => Err(invalid("missing", &path)),
            Some(Value::String(text)) => {
                Rgb::parse(text).ok_or_else(|| invalid("malformed", &path))
            }
            Some(_) => Err(invalid("wrong-type", &path)),
        }
    };
    let found = Colours {
        background: colour("background")?,
        text: colour("text")?,
        accent: colour("accent")?,
    };
    for (key, colour) in [("text", found.text), ("accent", found.accent)] {
        if !colour.contrasts_with(found.background, MIN_CONTRAST) {
            return Err(PluginError::ThemeLowContrast {
                key: format!("{look}.{key}"),
            });
        }
    }
    Ok(found)
}

fn invalid(reason: &'static str, key: &str) -> PluginError {
    PluginError::ThemeInvalid {
        reason,
        key: key.to_owned(),
    }
}

/// The manifest's text check, reported as a theme's problem.
fn theme_error(error: PluginError) -> PluginError {
    match error {
        PluginError::ManifestInvalid { reason, key } => PluginError::ThemeInvalid { reason, key },
        other => other,
    }
}

/// Each 8-bit sRGB channel value as linear light, times 10⁶ and rounded:
/// `c / 12.92` up to 0.04045, else `((c + 0.055) / 1.055)^2.4`, with
/// `c = value / 255` (IEC 61966-2-1, as WCAG uses it). A table, so the check
/// needs no floating point (AGENTS.md §3.1 rule 1 keeps it out of the
/// workspace).
#[rustfmt::skip]
const LINEAR: [u32; 256] = [
    0, 304, 607, 911, 1214, 1518, 1821, 2125,
    2428, 2732, 3035, 3347, 3677, 4025, 4391, 4777,
    5182, 5605, 6049, 6512, 6995, 7499, 8023, 8568,
    9134, 9721, 10330, 10960, 11612, 12286, 12983, 13702,
    14444, 15209, 15996, 16807, 17642, 18500, 19382, 20289,
    21219, 22174, 23153, 24158, 25187, 26241, 27321, 28426,
    29557, 30713, 31896, 33105, 34340, 35601, 36889, 38204,
    39546, 40915, 42311, 43735, 45186, 46665, 48172, 49707,
    51269, 52861, 54480, 56128, 57805, 59511, 61246, 63010,
    64803, 66626, 68478, 70360, 72272, 74214, 76185, 78187,
    80220, 82283, 84376, 86500, 88656, 90842, 93059, 95307,
    97587, 99899, 102242, 104616, 107023, 109462, 111932, 114435,
    116971, 119538, 122139, 124772, 127438, 130136, 132868, 135633,
    138432, 141263, 144128, 147027, 149960, 152926, 155926, 158961,
    162029, 165132, 168269, 171441, 174647, 177888, 181164, 184475,
    187821, 191202, 194618, 198069, 201556, 205079, 208637, 212231,
    215861, 219526, 223228, 226966, 230740, 234551, 238398, 242281,
    246201, 250158, 254152, 258183, 262251, 266356, 270498, 274677,
    278894, 283149, 287441, 291771, 296138, 300544, 304987, 309469,
    313989, 318547, 323143, 327778, 332452, 337164, 341914, 346704,
    351533, 356400, 361307, 366253, 371238, 376262, 381326, 386429,
    391572, 396755, 401978, 407240, 412543, 417885, 423268, 428690,
    434154, 439657, 445201, 450786, 456411, 462077, 467784, 473531,
    479320, 485150, 491021, 496933, 502886, 508881, 514918, 520996,
    527115, 533276, 539479, 545724, 552011, 558340, 564712, 571125,
    577580, 584078, 590619, 597202, 603827, 610496, 617207, 623960,
    630757, 637597, 644480, 651406, 658375, 665387, 672443, 679542,
    686685, 693872, 701102, 708376, 715694, 723055, 730461, 737910,
    745404, 752942, 760525, 768151, 775822, 783538, 791298, 799103,
    806952, 814847, 822786, 830770, 838799, 846873, 854993, 863157,
    871367, 879622, 887923, 896269, 904661, 913099, 921582, 930111,
    938686, 947307, 955973, 964686, 973445, 982251, 991102, 1000000,
];
