// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the user's palette (M7a.10, D-589).

use kassensturz_report::spec::Rgb;
use proptest::prelude::*;

use super::*;

const WHITE: Rgb = Rgb::new(255, 255, 255);
const BLACK: Rgb = Rgb::new(0, 0, 0);
/// The dark theme's window color, about as dark as Breeze Dark's.
const DARK: Rgb = Rgb::new(0x23, 0x26, 0x29);

#[test]
fn the_standard_palette_is_okabe_ito_with_grey_in_place_of_black() {
    let palette = Palette::standard();
    assert_eq!(
        palette.texts(),
        [
            "#0072B2", "#D55E00", "#009E73", "#CC79A7", "#E69F00", "#56B4E9", "#F0E442", "#999999"
        ]
    );
    assert_eq!(STANDARD_COLORS.len(), usize::from(PALETTE_SIZE));
    assert!(palette.all_standard());
    // The standard colors raise no warning on either theme: they were chosen
    // on purpose (D-283), and nothing is checked until the user changes one.
    assert!(palette.warnings(WHITE).is_empty());
    assert!(palette.warnings(DARK).is_empty());
}

#[test]
fn the_standard_neighbours_are_far_apart() {
    for pair in STANDARD_COLORS.windows(2) {
        assert!(!too_alike(pair[0], pair[1]), "{pair:?}");
    }
}

#[test]
fn a_place_is_changed_and_put_back_alone_or_with_the_rest() {
    let mut palette = Palette::standard();
    assert!(palette.set(2, Rgb::new(1, 2, 3)));
    assert!(palette.set(5, Rgb::new(4, 5, 6)));
    assert!(!palette.set(8, Rgb::new(9, 9, 9)));
    assert_eq!(palette.color(2), Rgb::new(1, 2, 3));
    assert!(!palette.is_standard(2));
    assert!(palette.is_standard(3));
    assert!(!palette.all_standard());
    assert!(palette.reset(2));
    assert_eq!(palette.color(2), STANDARD_COLORS[2]);
    assert!(!palette.reset(8));
    assert!(!palette.all_standard());
    palette.reset_all();
    assert!(palette.all_standard());
    // A place that doesn't exist reads as the standard color of the place
    // it wraps to, as a chart with more series than colors does.
    assert_eq!(palette.color(9), STANDARD_COLORS[1]);
}

#[test]
fn what_the_settings_file_holds_is_read_leniently() {
    let texts = [
        "#112233", "", "#abcdef", "blue", "#fff", "#00FF00", "#000000", "#999999",
    ];
    let palette = Palette::from_texts(&texts);
    assert_eq!(
        palette.texts(),
        [
            "#112233", "#D55E00", "#ABCDEF", "#CC79A7", "#E69F00", "#00FF00", "#000000", "#999999"
        ]
    );
    // Fewer or more texts than places: the rest stays standard, the extra is ignored.
    let short = Palette::from_texts(&["#112233"]);
    assert_eq!(short.color(0), Rgb::new(0x11, 0x22, 0x33));
    assert!(short.is_standard(1));
    let long = Palette::from_texts(&["#000001"; 12]);
    assert_eq!(long.color(7), Rgb::new(0, 0, 1));
    let none: [&str; 0] = [];
    assert!(Palette::from_texts(&none).all_standard());
    // Round trip through the texts.
    assert_eq!(Palette::from_texts(&palette.texts()), palette);
}

#[test]
fn contrast_follows_wcag_in_whole_numbers() {
    // Black on white is 21 to 1, the same color is 1 to 1.
    assert!(contrast_is_enough(BLACK, WHITE));
    assert!(contrast_is_enough(WHITE, BLACK));
    assert!(!contrast_is_enough(WHITE, WHITE));
    // Known WCAG values: #767676 on white is 4.54 to 1; #949494 is 3.03 to 1
    // and #959595 is 2.99 to 1 (the 3 to 1 line for graphics).
    assert!(contrast_is_enough(Rgb::new(0x76, 0x76, 0x76), WHITE));
    assert!(contrast_is_enough(Rgb::new(0x94, 0x94, 0x94), WHITE));
    assert!(!contrast_is_enough(Rgb::new(0x96, 0x96, 0x96), WHITE));
    // The standard yellow is bright on a dark theme and weak on a light one.
    assert!(contrast_is_enough(STANDARD_COLORS[6], DARK));
    assert!(!contrast_is_enough(STANDARD_COLORS[6], WHITE));
}

#[test]
fn a_changed_color_that_is_hard_to_see_is_named() {
    let mut palette = Palette::standard();
    palette.set(3, Rgb::new(250, 250, 245));
    assert_eq!(
        palette.warnings(WHITE),
        [PaletteWarning::LowContrast { slot: 3 }]
    );
    assert!(palette.warnings(DARK).is_empty());
    // An unchanged weak color (the yellow on white) is not named.
    let mut other = Palette::standard();
    other.set(0, Rgb::new(0, 0, 0));
    assert!(other.warnings(WHITE).is_empty());
    assert_eq!(
        other.warnings(DARK),
        [PaletteWarning::LowContrast { slot: 0 }]
    );
}

#[test]
fn a_changed_color_too_close_to_its_neighbour_is_named_for_the_pair() {
    let mut palette = Palette::standard();
    // Slot 1 is changed to nearly slot 0's color: the pair is named once,
    // and the pair after it is fine.
    palette.set(1, Rgb::new(0x00, 0x74, 0xB4));
    assert_eq!(
        palette.warnings(WHITE),
        [PaletteWarning::TooAlike { slot: 0, other: 1 }]
    );
    // Setting the places to the same color names every pair they make.
    let mut same = Palette::standard();
    for slot in 2..=4 {
        same.set(slot, Rgb::new(10, 20, 30));
    }
    let pairs: Vec<(u8, u8)> = same
        .warnings(DARK)
        .into_iter()
        .filter_map(|w| match w {
            PaletteWarning::TooAlike { slot, other } => Some((slot, other)),
            PaletteWarning::LowContrast { .. } => None,
        })
        .collect();
    assert_eq!(pairs, [(2, 3), (3, 4)]);
    // The last place has no neighbour after it and the first none before it.
    let mut ends = Palette::standard();
    ends.set(0, Rgb::new(0x00, 0x72, 0xB2));
    ends.set(7, Rgb::new(0x99, 0x99, 0x99));
    assert!(ends.warnings(WHITE).is_empty());
}

proptest! {
    #[test]
    fn closeness_is_symmetric_and_a_color_is_alike_itself(
        a: (u8, u8, u8), b: (u8, u8, u8),
    ) {
        let (one, other) = (Rgb::new(a.0, a.1, a.2), Rgb::new(b.0, b.1, b.2));
        prop_assert_eq!(too_alike(one, other), too_alike(other, one));
        prop_assert!(too_alike(one, one));
        prop_assert_eq!(contrast_is_enough(one, other), contrast_is_enough(other, one));
        prop_assert!(!contrast_is_enough(one, one));
    }

    #[test]
    fn any_texts_make_a_palette_whose_texts_read_back(
        texts in proptest::collection::vec(any::<String>(), 0..12),
    ) {
        let palette = Palette::from_texts(&texts);
        prop_assert_eq!(Palette::from_texts(&palette.texts()), palette);
        // Whatever the palette, the check runs and names only real places.
        for warning in palette.warnings(WHITE).into_iter().chain(palette.warnings(DARK)) {
            match warning {
                PaletteWarning::LowContrast { slot } => prop_assert!(slot < PALETTE_SIZE),
                PaletteWarning::TooAlike { slot, other } => {
                    prop_assert!(slot < PALETTE_SIZE);
                    prop_assert_eq!(other, slot + 1);
                }
            }
        }
    }
}
