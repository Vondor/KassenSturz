// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::{Value, json};

use super::*;
use crate::test_folder::TestFolder;
use crate::test_plugins;

fn green() -> Value {
    json!({
        "theme_version": 1,
        "light": {"background": "#F4FAF4", "text": "#1B1F1B", "accent": "#2E7D32"},
        "dark": {"background": "#121A13", "text": "#E3EDE3", "accent": "#81C784"},
        "corner_radius": 10,
    })
}

fn colours(background: &str, text: &str, accent: &str) -> Colours {
    Colours {
        background: Rgb::parse(background).unwrap(),
        text: Rgb::parse(text).unwrap(),
        accent: Rgb::parse(accent).unwrap(),
    }
}

/// Every text colour of `roles` on every background it sits on.
fn assert_readable(roles: &Roles) {
    for background in [roles.view, roles.window, roles.button] {
        for (name, colour) in [
            ("text", roles.text),
            ("inactive", roles.inactive),
            ("visited", roles.visited),
            ("negative", roles.negative),
            ("neutral", roles.neutral),
            ("positive", roles.positive),
        ] {
            assert!(
                colour.contrasts_with(background, MIN_CONTRAST),
                "{name} {} on {}",
                colour.hex(),
                background.hex()
            );
        }
    }
    assert!(roles.on_accent.contrasts_with(roles.accent, MIN_CONTRAST));
}

#[test]
fn mixes_in_whole_steps() {
    let black = Rgb::parse("#000000").unwrap();
    let white = Rgb::parse("#ffffff").unwrap();
    assert_eq!(mix(black, white, 0), black);
    assert_eq!(mix(black, white, 100), white);
    assert_eq!(mix(black, white, 50).hex(), "#808080");
    assert_eq!(mix(black, white, 250), white);
}

#[test]
fn text_on_the_accent_is_black_or_white_whichever_reads_better() {
    for accent in [
        "#2e7d32", "#81c784", "#ffeb3b", "#0d47a1", "#777777", "#808080",
    ] {
        let accent = Rgb::parse(accent).unwrap();
        let chosen = on(accent);
        let other = if chosen.red == 0 {
            Rgb::parse("#ffffff").unwrap()
        } else {
            Rgb::parse("#000000").unwrap()
        };
        // The chosen one is at least as readable as the other.
        let ratio_of = |colour: Rgb| {
            let (a, b) = (colour.luminance(), accent.luminance());
            let (high, low) = if a >= b { (a, b) } else { (b, a) };
            (high + 500_000_000) * 1000 / (low + 500_000_000)
        };
        assert!(ratio_of(chosen) >= ratio_of(other), "{}", accent.hex());
        assert!(
            chosen.contrasts_with(accent, MIN_CONTRAST),
            "{}",
            accent.hex()
        );
    }
}

#[test]
fn every_text_colour_stays_readable_whatever_the_theme() {
    // The green example, and themes at the edge of the contrast check:
    // the warning colours and the secondary text must still be readable.
    for look in [
        colours("#F4FAF4", "#1B1F1B", "#2E7D32"),
        colours("#121A13", "#E3EDE3", "#81C784"),
        colours("#ffffff", "#767676", "#767676"),
        colours("#000000", "#757575", "#757575"),
        colours("#ff8a80", "#000000", "#000000"),
        colours("#2e7d32", "#ffffff", "#ffffff"),
        colours("#c62828", "#ffffff", "#ffffff"),
        colours("#ffffff", "#000000", "#c62828"),
    ] {
        let roles = Roles::of(&look);
        assert_readable(&roles);
        assert_eq!(roles.text, look.text);
        assert_eq!(roles.accent, look.accent);
        assert_eq!(roles.view, look.background);
    }
}

#[test]
fn the_scheme_has_every_colour_set_kirigami_reads() {
    let roles = Roles::of(&colours("#F4FAF4", "#1B1F1B", "#2E7D32"));
    let scheme = roles.scheme();
    for set in [
        "Button",
        "Complementary",
        "Header",
        "Selection",
        "Tooltip",
        "View",
        "Window",
    ] {
        let header = format!("[Colors:{set}]\n");
        let at = scheme.find(&header).unwrap_or_else(|| panic!("{set}"));
        let body = &scheme[at + header.len()..];
        let body = &body[..body.find("\n\n").unwrap()];
        assert_eq!(body.lines().count(), 12, "{set}");
        for key in [
            "BackgroundNormal",
            "ForegroundNormal",
            "ForegroundNegative",
            "DecorationFocus",
        ] {
            assert!(body.contains(&format!("{key}=")), "{set} {key}");
        }
    }
    assert!(scheme.contains("[Colors:View]\nBackgroundAlternate="));
    assert!(scheme.contains("BackgroundNormal=244,250,244\n"));
    assert!(scheme.contains("ForegroundNormal=27,31,27\n"));
    // On the selection, everything is the text on the accent.
    let selection = &scheme[scheme.find("[Colors:Selection]").unwrap()..];
    let selection = &selection[..selection.find("\n\n").unwrap()];
    assert!(selection.contains("BackgroundNormal=46,125,50\n"));
    assert!(
        selection.contains("ForegroundNegative=255,255,255\n"),
        "{selection}"
    );
    assert!(scheme.contains("[ColorEffects:Disabled]\n"));
    assert!(scheme.contains("[General]\nColorScheme=KassenSturzTheme\n"));
}

#[test]
fn the_palette_names_qt_roles() {
    let roles = Roles::of(&colours("#F4FAF4", "#1B1F1B", "#2E7D32"));
    let palette = roles.palette();
    let entries: Vec<&str> = palette.split(';').collect();
    assert_eq!(entries.len(), 15);
    assert!(entries.contains(&"Base=#f4faf4"));
    assert!(entries.contains(&"Text=#1b1f1b"));
    assert!(entries.contains(&"Highlight=#2e7d32"));
    assert!(entries.contains(&"HighlightedText=#ffffff"));
    assert!(entries.iter().all(|entry| {
        let (role, colour) = entry.split_once('=').unwrap();
        !role.is_empty() && Rgb::parse(colour).is_some()
    }));
}

#[test]
fn finds_the_theme_plugins_in_the_folder() {
    let folder = TestFolder::new();
    std::fs::write(
        folder.join("b.wasm"),
        test_plugins::theme_file("org.example.green", "Green", &green()),
    )
    .unwrap();
    std::fs::write(
        folder.join("a.wasm"),
        test_plugins::theme_file("org.example.amber", "amber", &green()),
    )
    .unwrap();
    // Not a theme, and not a plugin: both left out.
    std::fs::write(
        folder.join("c.wasm"),
        test_plugins::file(
            "org.example.density",
            "Compact",
            &["ui_density"],
            "(module)",
        ),
    )
    .unwrap();
    std::fs::write(folder.join("d.wasm"), b"not a plugin").unwrap();
    let found = installed(folder.path());
    let ids: Vec<&str> = found.iter().map(|theme| theme.id.as_str()).collect();
    assert_eq!(ids, ["org.example.amber", "org.example.green"]);
    assert_eq!(found[1].name, "Green");
    assert_eq!(found[1].theme.corner_radius, Some(10));
    assert_eq!(found[1].theme.light.accent.hex(), "#2e7d32");
}

#[test]
fn the_choice_is_kept_under_look_theme() {
    let folder = TestFolder::new();
    let file = folder.join("kassensturzrc");
    let settings = Settings::at(&file);
    assert_eq!(chosen(&settings), None);
    assert!(choose(&settings, Some("org.example.green")));
    assert_eq!(
        chosen(&Settings::at(&file)),
        Some("org.example.green".into())
    );
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(
        written.contains("[Look]\nTheme=org.example.green"),
        "{written}"
    );
    assert!(choose(&settings, None));
    assert_eq!(chosen(&Settings::at(&file)), None);
}
