// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colors of the charts (M7a.10, D-589, D-588) as a QML object: the Settings
//! page shows and changes them, and the charts read them from `colors`.
//!
//! A color is `#RRGGBB` text in both directions (D-065): QML never hands
//! over numbers for a color, and Rust checks every value (`Rgb`, each part
//! 0 to 255). The page owns every label and sentence (D-034): this object
//! answers with words such as "ok" or "format" and with the places of the
//! warnings.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_charts::{MAX_COLORS, MIN_COLORS, Palette, PaletteWarning, STANDARD_COLORS};
use kassensturz_report::spec::{ColorError, Rgb};

use crate::chart_palette;
use crate::variant::{list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        /// A value of any Qt type.
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The app's chart colors: eight at the start, 2 to 16 once changed.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, colors, READ, NOTIFY)]
        // The standard color of every place that can exist (16), not only of
        // the places there are now.
        #[qproperty(QVariant, standard, READ, CONSTANT)]
        #[qproperty(i32, min_colors, READ, CONSTANT)]
        #[qproperty(i32, max_colors, READ, CONSTANT)]
        #[qproperty(bool, changed, READ, NOTIFY)]
        type ChartPaletteSettings = super::ChartPaletteSettingsRust;
    }

    extern "RustQt" {
        /// Gives place `slot` (0 to 15) the color in `entry`, written as
        /// `#rrggbb` or as red, green and blue from 0 to 255, for the app
        /// from now on and at the next start. Returns "ok", "not-saved"
        /// (used until the app closes), "format" or "part" (the text is not
        /// a color, or a part is above 255) or "slot" (no such place).
        #[qinvokable]
        #[cxx_name = "setColor"]
        fn set_color(self: Pin<&mut ChartPaletteSettings>, slot: i32, entry: &QString) -> QString;

        /// Puts place `slot` back to its standard color. Returns "ok",
        /// "not-saved" or "slot".
        #[qinvokable]
        fn reset(self: Pin<&mut ChartPaletteSettings>, slot: i32) -> QString;

        /// Puts every place back to its standard color. Returns "ok" or
        /// "not-saved".
        #[qinvokable]
        #[cxx_name = "resetAll"]
        fn reset_all(self: Pin<&mut ChartPaletteSettings>) -> QString;

        /// Adds a place at the end with its standard color. Returns "ok",
        /// "not-saved" or "limit" (there are 16 already).
        #[qinvokable]
        #[cxx_name = "addColor"]
        fn add_color(self: Pin<&mut ChartPaletteSettings>) -> QString;

        /// Removes place `slot`; the places after it move up. Returns "ok",
        /// "not-saved", "limit" (there are only two) or "slot" (no such
        /// place).
        #[qinvokable]
        #[cxx_name = "removeColor"]
        fn remove_color(self: Pin<&mut ChartPaletteSettings>, slot: i32) -> QString;

        /// The warnings for the colors that were changed, on a background
        /// of `background` (`#rrggbb`): an array of objects with `kind`
        /// ("contrast" for a color hard to see, "alike" for a color too
        /// close to the next place), `slot` and `other` (the next place for
        /// "alike", else -1).
        #[qinvokable]
        fn warnings(self: &ChartPaletteSettings, background: &QString) -> QVariant;
    }

    impl cxx_qt::Initialize for ChartPaletteSettings {}
}

/// The Rust side of [`qobject::ChartPaletteSettings`].
#[derive(Default)]
pub struct ChartPaletteSettingsRust {
    colors: QVariant,
    standard: QVariant,
    min_colors: i32,
    max_colors: i32,
    changed: bool,
}

impl cxx_qt::Initialize for qobject::ChartPaletteSettings {
    fn initialize(mut self: Pin<&mut Self>) {
        {
            let mut model = self.as_mut().rust_mut();
            model.standard = list(STANDARD_COLORS.iter().map(|color| text(&color.to_hex())));
            model.min_colors = count_of(MIN_COLORS);
            model.max_colors = count_of(MAX_COLORS);
        }
        self.show(&chart_palette::current());
    }
}

/// A number of places as the `i32` QML reads (it is at most 16).
fn count_of(count: usize) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}

/// The colors as an array of `#RRGGBB` texts.
fn texts(palette: &Palette) -> QVariant {
    list(palette.texts().iter().map(|color| text(color)))
}

/// The warnings of `palette` on the background `background` (`#rrggbb`) as
/// QML reads them. A background that is not a color gets no check, not a
/// wrong one.
fn warnings_object(palette: &Palette, background: &str) -> QVariant {
    let Some(background) = Rgb::parse(background) else {
        return list(std::iter::empty());
    };
    list(
        palette
            .warnings(background)
            .into_iter()
            .map(|warning| match warning {
                PaletteWarning::LowContrast { slot } => map([
                    ("kind", text("contrast")),
                    ("slot", QVariant::from(&i32::from(slot))),
                    ("other", QVariant::from(&-1_i32)),
                ]),
                PaletteWarning::TooAlike { slot, other } => map([
                    ("kind", text("alike")),
                    ("slot", QVariant::from(&i32::from(slot))),
                    ("other", QVariant::from(&i32::from(other))),
                ]),
            }),
    )
}

/// What a change returns: "ok", or "not-saved" when the file couldn't be
/// written.
fn saved_word(saved: bool) -> &'static str {
    if saved { "ok" } else { "not-saved" }
}

/// The place `slot` of QML, if it is one of the sixteen that can exist
/// (whether it exists now is for the palette to say).
fn slot_of(slot: i32) -> Option<usize> {
    usize::try_from(slot).ok().filter(|slot| *slot < MAX_COLORS)
}

impl qobject::ChartPaletteSettings {
    fn set_color(self: Pin<&mut Self>, slot: i32, entry: &QString) -> QString {
        let Some(slot) = slot_of(slot) else {
            return QString::from("slot");
        };
        let color = match Rgb::parse_entry(&entry.to_string()) {
            Ok(color) => color,
            Err(ColorError::Format) => return QString::from("format"),
            Err(ColorError::Part) => return QString::from("part"),
        };
        let mut palette = chart_palette::current();
        if !palette.set(slot, color) {
            return QString::from("slot");
        }
        QString::from(self.change(palette))
    }

    fn reset(self: Pin<&mut Self>, slot: i32) -> QString {
        let Some(slot) = slot_of(slot) else {
            return QString::from("slot");
        };
        let mut palette = chart_palette::current();
        if !palette.reset(slot) {
            return QString::from("slot");
        }
        QString::from(self.change(palette))
    }

    fn add_color(self: Pin<&mut Self>) -> QString {
        let mut palette = chart_palette::current();
        if !palette.add() {
            return QString::from("limit");
        }
        QString::from(self.change(palette))
    }

    fn remove_color(self: Pin<&mut Self>, slot: i32) -> QString {
        let Some(slot) = slot_of(slot) else {
            return QString::from("slot");
        };
        let mut palette = chart_palette::current();
        if slot >= palette.len() {
            return QString::from("slot");
        }
        if !palette.remove(slot) {
            return QString::from("limit");
        }
        QString::from(self.change(palette))
    }

    fn reset_all(self: Pin<&mut Self>) -> QString {
        let mut palette = chart_palette::current();
        palette.reset_all();
        QString::from(self.change(palette))
    }

    fn warnings(&self, background: &QString) -> QVariant {
        warnings_object(&chart_palette::current(), &background.to_string())
    }

    /// Makes `palette` the app's, saves it and shows it; returns the word
    /// the invokable answers with.
    fn change(self: Pin<&mut Self>, palette: Palette) -> &'static str {
        let saved = chart_palette::set_current(palette.clone());
        self.show(&palette);
        saved_word(saved)
    }

    /// Shows `palette`.
    fn show(mut self: Pin<&mut Self>, palette: &Palette) {
        let mut model = self.as_mut().rust_mut();
        model.colors = texts(palette);
        model.changed = !palette.all_standard();
        self.as_mut().colors_changed();
        self.as_mut().changed_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::read_back::{elements, number_of, text_of};

    fn standard_texts() -> Vec<String> {
        STANDARD_COLORS[..8]
            .iter()
            .map(|color| color.to_hex())
            .collect()
    }

    fn texts_of(value: &QVariant) -> Vec<String> {
        elements(value)
            .expect("an array")
            .iter()
            .map(|color| color.value::<QString>().expect("text").to_string())
            .collect()
    }

    #[test]
    fn the_colors_reach_qml_as_an_array_of_text() {
        assert_eq!(texts_of(&texts(&Palette::standard())), standard_texts());
        let mut palette = Palette::standard();
        palette.set(0, Rgb::new(1, 2, 255));
        let shown = texts_of(&texts(&palette));
        assert_eq!(shown[0], "#0102FF");
        assert_eq!(shown.len(), 8);
        // A palette that grew shows every color it has.
        palette.add();
        palette.add();
        assert_eq!(texts_of(&texts(&palette)).len(), 10);
    }

    #[test]
    fn a_place_is_one_of_the_sixteen_that_can_exist() {
        assert_eq!(slot_of(0), Some(0));
        assert_eq!(slot_of(7), Some(7));
        assert_eq!(slot_of(15), Some(15));
        assert_eq!(slot_of(16), None);
        assert_eq!(slot_of(-1), None);
        assert_eq!(slot_of(i32::MAX), None);
    }

    /// The custom chart page names the palette's places by the words a
    /// description writes, in the palette's order (Q-410).
    #[test]
    fn the_custom_chart_page_lists_the_words_of_the_palette_in_order() {
        use kassensturz_report::spec::PaletteName;

        let words: Vec<String> = PaletteName::ALL
            .iter()
            .map(|name| format!("\"{}\"", name.word()))
            .collect();
        let line = format!("[{}]", words.join(", "));
        assert!(
            include_str!("../qml/SpecPage.qml").contains(&line),
            "SpecPage.qml paletteWords must be {line}"
        );
        // The words name the eight places of the standard palette.
        assert_eq!(PaletteName::ALL.len(), kassensturz_charts::DEFAULT_COLORS);
    }

    /// The standard colors of the QML singleton are the ones Rust has.
    #[test]
    fn the_standard_colors_of_the_qml_palette_are_the_ones_of_rust() {
        let source = include_str!("../qml/ChartPalette.qml");
        // The singleton lists the eight of the standard palette; the added
        // places' colors come from Rust (`standard`).
        for color in &STANDARD_COLORS[..kassensturz_charts::DEFAULT_COLORS] {
            assert!(
                source.contains(&format!("\"{}\"", color.to_hex())),
                "ChartPalette.qml lacks {}",
                color.to_hex()
            );
        }
    }

    /// Where a chart's free colors start is a number both sides know.
    #[test]
    fn the_free_color_base_of_the_qml_palette_is_the_one_of_rust() {
        let source = include_str!("../qml/ChartPalette.qml");
        let line = format!(
            "readonly property int extraColorBase: {}",
            kassensturz_charts::EXTRA_COLOR_BASE
        );
        assert!(source.contains(&line), "ChartPalette.qml lacks {line}");
    }

    #[test]
    fn the_words_of_a_change() {
        assert_eq!(saved_word(true), "ok");
        assert_eq!(saved_word(false), "not-saved");
    }

    #[test]
    fn a_warning_reads_back_with_its_kind_and_places() {
        let mut palette = Palette::standard();
        // Nearly the color of place 0 in place 1, and a color that is hard
        // to see on white in place 4.
        palette.set(1, Rgb::new(0x00, 0x74, 0xB4));
        palette.set(4, Rgb::new(250, 250, 245));
        let warnings = elements(&warnings_object(&palette, "#FFFFFF")).expect("an array");
        let kinds: Vec<(String, i32, i32)> = warnings
            .iter()
            .map(|one| {
                (
                    text_of(one, "kind"),
                    number_of(one, "slot"),
                    number_of(one, "other"),
                )
            })
            .collect();
        assert_eq!(
            kinds,
            [("contrast".to_owned(), 4, -1), ("alike".to_owned(), 0, 1),]
        );
        // A background that is not `#rrggbb` is not checked against.
        for background in ["", "white", "#fff", "rgb(1, 2, 3)"] {
            assert!(
                elements(&warnings_object(&palette, background))
                    .expect("an array")
                    .is_empty()
            );
        }
        // The standard palette has nothing to warn about.
        assert!(
            elements(&warnings_object(&Palette::standard(), "#FFFFFF"))
                .expect("an array")
                .is_empty()
        );
    }
}
