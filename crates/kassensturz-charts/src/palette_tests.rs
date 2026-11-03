// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the user's palette (M7a.10, D-589; its number of colors, D-588).

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
    assert_eq!(palette.len(), DEFAULT_COLORS);
    assert_eq!(STANDARD_COLORS.len(), 16);
    assert!(palette.all_standard());
    // The standard colors raise no warning on either theme: they were chosen
    // on purpose (D-283), and nothing is checked until the user changes one.
    assert!(palette.warnings(WHITE).is_empty());
    assert!(palette.warnings(DARK).is_empty());
}

#[test]
fn the_standard_neighbours_are_far_apart() {
    // Every place that can exist, so a palette that grows never starts with
    // a warning, and the last place against the first (where a chart that
    // needs more colors starts again).
    for slot in 0..MAX_COLORS - 1 {
        let (one, other) = (
            Palette::standard_color(slot),
            Palette::standard_color(slot + 1),
        );
        assert!(
            !too_alike(one, other),
            "places {slot} and {}: {one:?} {other:?}",
            slot + 1
        );
    }
    assert!(!too_alike(
        Palette::standard_color(MAX_COLORS - 1),
        Palette::standard_color(0)
    ));
}

#[test]
fn the_standard_colors_of_all_the_places_are_different() {
    let all: Vec<Rgb> = (0..MAX_COLORS).map(Palette::standard_color).collect();
    for (at, one) in all.iter().enumerate() {
        for (later, other) in all.iter().enumerate().skip(at + 1) {
            assert_ne!(one, other, "places {at} and {later}");
        }
    }
}

/// The whole-number HSL conversion gives the known colors.
#[test]
fn the_generated_colors_follow_the_hsl_conversion() {
    // Place 16 is hue 30, saturation 65, lightness 45: HSL(30, 65%, 45%) is
    // #BD7328 by the usual formula (checked against Python's colorsys).
    let color = Palette::standard_color(16);
    assert_eq!(color.to_hex(), "#BD7328");
    // Place 17: hue 167, lightness 62 (HSL(167, 65%, 62%), #5FDDC2 by the same check).
    assert_eq!(Palette::standard_color(17).to_hex(), "#5FDDC2");
}

#[test]
fn a_place_is_changed_and_put_back_alone_or_with_the_rest() {
    let mut palette = Palette::standard();
    assert!(palette.set(2, Rgb::new(1, 2, 3)));
    assert!(palette.set(5, Rgb::new(4, 5, 6)));
    assert!(
        !palette.set(8, Rgb::new(9, 9, 9)),
        "the standard palette has eight places"
    );
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
    // A place that doesn't exist wraps round, as a chart with more series
    // than colors does.
    assert_eq!(palette.color(9), STANDARD_COLORS[1]);
}

#[test]
fn colors_are_added_and_removed_within_two_and_the_most() {
    let mut palette = Palette::standard();
    // Adding gives the standard color of the new place.
    assert!(palette.add());
    assert_eq!(palette.len(), 9);
    assert_eq!(palette.color(8), STANDARD_COLORS[8]);
    assert!(palette.is_standard(8));
    assert!(
        !palette.all_standard(),
        "nine places is not the standard palette"
    );
    while palette.add() {}
    assert_eq!(palette.len(), MAX_COLORS);
    assert!(!palette.add(), "the most is the most");
    // A chart with one part more than the palette has starts again at the
    // first color.
    assert_eq!(palette.color(MAX_COLORS), palette.color(0));
    // Removing a place moves the later ones up.
    palette.set(4, Rgb::new(1, 2, 3));
    assert!(palette.remove(2));
    assert_eq!(palette.len(), MAX_COLORS - 1);
    assert_eq!(palette.color(3), Rgb::new(1, 2, 3));
    assert!(
        !palette.is_standard(3),
        "the moved color is not the standard of its new place"
    );
    assert!(!palette.remove(MAX_COLORS - 1), "no such place");
    while palette.remove(0) {}
    assert_eq!(palette.len(), MIN_COLORS, "two is the fewest");
    assert!(!palette.remove(0));
    // Every chart can still be colored with two.
    assert_eq!(palette.color(2), palette.color(0));
    palette.reset_all();
    assert!(palette.all_standard());
    assert_eq!(palette.len(), DEFAULT_COLORS);
}

#[test]
fn what_the_settings_file_holds_is_read_leniently() {
    let texts = [
        "#112233", "", "#abcdef", "blue", "#fff", "#00FF00", "#000000", "#999999",
    ];
    let palette = Palette::from_texts(8, &texts);
    assert_eq!(
        palette.texts(),
        [
            "#112233", "#D55E00", "#ABCDEF", "#CC79A7", "#E69F00", "#00FF00", "#000000", "#999999"
        ]
    );
    // Fewer texts than places: the rest stays standard; more are ignored.
    let short = Palette::from_texts(8, &["#112233"]);
    assert_eq!(short.color(0), Rgb::new(0x11, 0x22, 0x33));
    assert!(short.is_standard(1));
    let long = Palette::from_texts(8, &["#000001"; 12]);
    assert_eq!(long.len(), 8);
    assert_eq!(long.color(7), Rgb::new(0, 0, 1));
    let none: [&str; 0] = [];
    assert!(Palette::from_texts(8, &none).all_standard());
    // The number of places is kept between two and the most, and the places
    // above eight start with their own standard colors.
    assert_eq!(Palette::from_texts(0, &none).len(), MIN_COLORS);
    assert_eq!(Palette::from_texts(9999, &none).len(), MAX_COLORS);
    assert_eq!(Palette::from_texts(10, &none).color(9), STANDARD_COLORS[9]);
    // A file of the first version, eight colors and no count, reads as before.
    assert_eq!(
        Palette::from_texts(DEFAULT_COLORS, &texts).texts(),
        palette.texts()
    );
    // Round trip through the texts.
    assert_eq!(
        Palette::from_texts(palette.len(), &palette.texts()),
        palette
    );
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
    // A palette that grew or shrank is checked the same way.
    let mut longer = Palette::standard();
    longer.add();
    longer.set(8, Rgb::new(0x99, 0x99, 0x9A));
    assert_eq!(
        longer.warnings(DARK),
        [PaletteWarning::TooAlike { slot: 7, other: 8 }]
    );
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
        count in 0_usize..40,
        texts in proptest::collection::vec(any::<String>(), 0..20),
    ) {
        let palette = Palette::from_texts(count, &texts);
        prop_assert!((MIN_COLORS..=MAX_COLORS).contains(&palette.len()));
        prop_assert_eq!(Palette::from_texts(palette.len(), &palette.texts()), palette.clone());
        // Whatever the palette, the check runs and names only real places.
        for warning in palette.warnings(WHITE).into_iter().chain(palette.warnings(DARK)) {
            match warning {
                PaletteWarning::LowContrast { slot } => prop_assert!(usize::from(slot) < palette.len()),
                PaletteWarning::TooAlike { slot, other } => {
                    prop_assert!(usize::from(other) < palette.len());
                    prop_assert_eq!(other, slot + 1);
                }
            }
        }
    }

    #[test]
    fn adding_and_removing_keeps_the_palette_inside_its_limits(
        steps in proptest::collection::vec((any::<bool>(), 0_usize..20), 0..60),
    ) {
        let mut palette = Palette::standard();
        for (add, slot) in steps {
            if add {
                palette.add();
            } else {
                palette.remove(slot);
            }
            prop_assert!((MIN_COLORS..=MAX_COLORS).contains(&palette.len()));
            // A color is always there for any chart place.
            let _ = palette.color(slot);
        }
    }
}
