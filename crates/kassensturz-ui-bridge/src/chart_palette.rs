// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colors of the charts the user chose (M7a.10, D-589): the app's
//! palette, kept with KConfig like the number format (D-002, D-112), per
//! computer and never in a book. The work is in `kassensturz-charts`
//! (`Palette`); this file reads and writes the settings file and holds the
//! palette the app uses now.
//!
//! The settings file keeps a place's color as `#RRGGBB` text under `Color1`
//! to `Color8` in the group `Charts`, and nothing for a place that has its
//! standard color, so a later change of the standard colors reaches everyone
//! who never changed them. A text that is not exactly `#rrggbb` reads as the
//! standard color, so a file edited by hand never breaks a chart.

use std::cell::Cell;

use kassensturz_charts::{PALETTE_SIZE, Palette};

use crate::settings::Settings;

/// The group of the settings file that holds the chart colors.
const CHARTS: &str = "Charts";

/// The key of place `slot` (0 to 7): `Color1` to `Color8`.
fn key(slot: usize) -> String {
    format!("Color{}", slot + 1)
}

/// The palette kept in `settings`.
pub fn read(settings: &Settings) -> Palette {
    let texts: Vec<String> = (0..usize::from(PALETTE_SIZE))
        .map(|slot| settings.text(CHARTS, &key(slot)))
        .collect();
    Palette::from_texts(&texts)
}

/// Keeps `palette` in `settings`: the colors that were changed, and an
/// empty entry for the places that have their standard color. Returns
/// whether everything was saved.
pub fn write(settings: &Settings, palette: &Palette) -> bool {
    let mut saved = true;
    for (slot, text) in palette.texts().iter().enumerate() {
        let value = if palette.is_standard(slot) { "" } else { text };
        saved &= settings.set_text(CHARTS, &key(slot), value);
    }
    saved
}

thread_local! {
    /// The palette the app uses, read from the settings file at the first
    /// use. QML objects live on the GUI thread only (`session.rs`).
    static CURRENT: Cell<Option<Palette>> = const { Cell::new(None) };
}

/// The palette the app uses now.
pub fn current() -> Palette {
    CURRENT.with(|current| {
        current.get().unwrap_or_else(|| {
            let palette = read(&Settings::user());
            current.set(Some(palette));
            palette
        })
    })
}

/// Makes `palette` the one the app uses, from now on and at the next start.
/// Returns whether it was saved; it is used until the app closes anyway.
pub fn set_current(palette: Palette) -> bool {
    CURRENT.with(|current| current.set(Some(palette)));
    write(&Settings::user(), &palette)
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
