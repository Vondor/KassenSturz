// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colors of the charts the user chose (M7a.10, D-589, D-588): the app's
//! palette, kept with KConfig like the number format (D-002, D-112), per
//! computer and never in a book. The work is in `kassensturz-charts`
//! (`Palette`); this file reads and writes the settings file and holds the
//! palette the app uses now.
//!
//! The settings file keeps a place's color as `#RRGGBB` text under `Color1`
//! to `Color200` in the group `Charts`, and nothing for a place that has its
//! standard color, so a later change of the standard colors reaches everyone
//! who never changed them. A text that is not exactly `#rrggbb` reads as the
//! standard color, so a file edited by hand never breaks a chart.
//!
//! `ColorCount` is how many places there are, written only when it is not
//! the standard eight. A file without it, such as every file made before the
//! count could be changed, has eight places and reads exactly as before, so
//! no migration is needed.

use std::cell::RefCell;

use kassensturz_charts::{DEFAULT_COLORS, MAX_COLORS, Palette};

use crate::settings::Settings;

/// The group of the settings file that holds the chart colors.
const CHARTS: &str = "Charts";

/// The key of the number of places.
const COUNT: &str = "ColorCount";

/// The key of place `slot` (0 to 199): `Color1` to `Color200`.
fn key(slot: usize) -> String {
    format!("Color{}", slot + 1)
}

/// The palette kept in `settings`.
pub fn read(settings: &Settings) -> Palette {
    // No count, or one that is not a number, is the standard eight.
    let count = settings
        .text(CHARTS, COUNT)
        .trim()
        .parse::<usize>()
        .unwrap_or(DEFAULT_COLORS);
    let texts: Vec<String> = (0..MAX_COLORS)
        .map(|slot| settings.text(CHARTS, &key(slot)))
        .collect();
    Palette::from_texts(count, &texts)
}

/// Keeps `palette` in `settings`: the number of places if it is not eight,
/// the colors that were changed, and an empty entry for the places that
/// have their standard color or that no longer exist. Returns whether
/// everything was saved.
pub fn write(settings: &Settings, palette: &Palette) -> bool {
    let count = if palette.len() == DEFAULT_COLORS {
        String::new()
    } else {
        palette.len().to_string()
    };
    let mut saved = keep(settings, COUNT, &count);
    let texts = palette.texts();
    for slot in 0..MAX_COLORS {
        let value = match texts.get(slot) {
            Some(text) if !palette.is_standard(slot) => text.as_str(),
            _ => "",
        };
        saved &= keep(settings, &key(slot), value);
    }
    saved
}

/// Keeps `value` under `key`, unless it is what the file holds already:
/// every write of a settings entry goes to the file, and a palette of 200
/// places changes one or a few of them at a time.
fn keep(settings: &Settings, key: &str, value: &str) -> bool {
    settings.text(CHARTS, key) == value || settings.set_text(CHARTS, key, value)
}

thread_local! {
    /// The palette the app uses, read from the settings file at the first
    /// use. QML objects live on the GUI thread only (`session.rs`).
    static CURRENT: RefCell<Option<Palette>> = const { RefCell::new(None) };
}

/// The palette the app uses now.
pub fn current() -> Palette {
    CURRENT.with(|current| {
        current
            .borrow_mut()
            .get_or_insert_with(|| read(&Settings::user()))
            .clone()
    })
}

/// Makes `palette` the one the app uses, from now on and at the next start.
/// Returns whether it was saved; it is used until the app closes anyway.
pub fn set_current(palette: Palette) -> bool {
    let saved = write(&Settings::user(), &palette);
    CURRENT.with(|current| *current.borrow_mut() = Some(palette));
    saved
}

#[cfg(test)]
mod tests {
    use kassensturz_report::spec::Rgb;

    use super::*;
    use crate::test_folder::TestFolder;

    #[test]
    fn a_changed_color_is_kept_and_a_standard_one_is_not() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert!(read(&settings).all_standard());

        let mut palette = Palette::standard();
        palette.set(1, Rgb::new(0x12, 0xAB, 0xEF));
        assert!(write(&settings, &palette));
        let again = Settings::at(&file);
        assert_eq!(read(&again), palette);
        let written = std::fs::read_to_string(&file).expect("settings file written");
        assert!(written.contains("Color2=#12ABEF"), "{written}");
        // Only what was changed has a value.
        assert!(!written.contains("#0072B2"), "{written}");

        // Back to the standard color: the value is emptied.
        palette.reset(1);
        assert!(write(&again, &palette));
        assert!(read(&Settings::at(&file)).all_standard());
    }

    #[test]
    fn the_number_of_colors_is_kept_only_when_it_is_not_eight() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);

        let mut palette = Palette::standard();
        assert!(palette.add());
        assert!(palette.add());
        palette.set(9, Rgb::new(1, 2, 3));
        assert!(write(&settings, &palette));
        let written = std::fs::read_to_string(&file).expect("settings file written");
        assert!(written.contains("ColorCount=10"), "{written}");
        assert!(written.contains("Color10=#010203"), "{written}");
        assert_eq!(read(&Settings::at(&file)), palette);

        // Fewer colors again: the places that are gone are emptied, so a
        // later added place starts with its standard color, not a stale one.
        palette.remove(9);
        palette.remove(8);
        assert_eq!(palette.len(), 8);
        assert!(write(&settings, &palette));
        let again = Settings::at(&file);
        assert!(read(&again).all_standard());
        let written = std::fs::read_to_string(&file).expect("settings file written");
        assert!(!written.contains("#010203"), "{written}");
        let mut grown = read(&again);
        grown.add();
        assert!(grown.is_standard(8));
    }

    #[test]
    fn a_file_made_before_the_count_existed_reads_as_before() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        // Eight colors, one changed, no `ColorCount`: what #414 wrote.
        assert!(settings.set_text(CHARTS, "Color1", "#B20A23"));
        let palette = read(&settings);
        assert_eq!(palette.len(), DEFAULT_COLORS);
        assert_eq!(palette.color(0), Rgb::new(0xB2, 0x0A, 0x23));
        assert!(palette.is_standard(1));
        // A count that is no number, or out of range, is made sensible.
        assert!(settings.set_text(CHARTS, COUNT, "many"));
        assert_eq!(read(&settings).len(), DEFAULT_COLORS);
        assert!(settings.set_text(CHARTS, COUNT, "9999"));
        assert_eq!(read(&settings).len(), MAX_COLORS);
        assert!(settings.set_text(CHARTS, COUNT, "0"));
        assert_eq!(read(&settings).len(), kassensturz_charts::MIN_COLORS);
    }

    #[test]
    fn a_text_that_is_not_a_color_reads_as_the_standard_one() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert!(settings.set_text(CHARTS, "Color1", "#fff"));
        assert!(settings.set_text(CHARTS, "Color2", "red"));
        assert!(settings.set_text(CHARTS, "Color3", "#001122"));
        let palette = read(&settings);
        assert!(palette.is_standard(0));
        assert!(palette.is_standard(1));
        assert_eq!(palette.color(2), Rgb::new(0, 0x11, 0x22));
    }
}
