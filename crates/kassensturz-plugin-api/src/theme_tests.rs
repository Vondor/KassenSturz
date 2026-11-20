// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::{Value, json};

use crate::*;

fn green() -> Value {
    json!({
        "theme_version": 1,
        "light": {"background": "#F4FAF4", "text": "#1B1F1B", "accent": "#2E7D32"},
        "dark": {"background": "#121A13", "text": "#E3EDE3", "accent": "#81C784"},
        "font": "Example Sans",
        "corner_radius": 8,
    })
}

fn parse(value: &Value) -> Result<Theme, PluginError> {
    Theme::parse(value.to_string().as_bytes())
}

fn with(key: &str, value: Value) -> Value {
    let mut theme = green();
    theme[key] = value;
    theme
}

fn without(key: &str) -> Value {
    let mut theme = green();
    theme.as_object_mut().unwrap().remove(key);
    theme
}

fn invalid(reason: &'static str, key: &str) -> Result<Theme, PluginError> {
    Err(PluginError::ThemeInvalid {
        reason,
        key: key.into(),
    })
}

#[test]
fn reads_a_theme() {
    let theme = parse(&green()).unwrap();
    assert_eq!(
        theme.light.accent,
        Rgb {
            red: 0x2E,
            green: 0x7D,
            blue: 0x32
        }
    );
    assert_eq!(theme.dark.background.hex(), "#121a13");
    assert_eq!(theme.font.as_deref(), Some("Example Sans"));
    assert_eq!(theme.corner_radius, Some(8));

    let plain = parse(&without("font")).unwrap();
    assert_eq!(plain.font, None);
    let mut plain = without("font");
    plain.as_object_mut().unwrap().remove("corner_radius");
    assert_eq!(parse(&plain).unwrap().corner_radius, None);
}

#[test]
fn reads_only_six_digit_colours() {
    assert_eq!(
        Rgb::parse("#00ff7F").map(Rgb::hex).as_deref(),
        Some("#00ff7f")
    );
    for text in [
        "00ff7f",
        "#0f7",
        "#00ff7f00",
        "#00gg7f",
        "#+0ff7f",
        " #00ff7f",
        "#00ff7f ",
    ] {
        assert_eq!(Rgb::parse(text), None, "{text}");
    }
    let mut theme = green();
    theme["light"]["accent"] = json!("green");
    assert_eq!(parse(&theme), invalid("malformed", "light.accent"));
}

#[test]
fn luminance_and_contrast_follow_wcag() {
    let black = Rgb::parse("#000000").unwrap();
    let white = Rgb::parse("#ffffff").unwrap();
    assert_eq!(black.luminance(), 0);
    assert_eq!(white.luminance(), 10_000_000_000);
    // Black on white is 21 to 1, the most there is.
    assert!(white.contrasts_with(black, (21, 1)));
    assert!(!white.contrasts_with(black, (211, 10)));
    // #767676 on white is 4.54 to 1, the classic lightest grey that passes.
    let grey = Rgb::parse("#767676").unwrap();
    assert!(grey.contrasts_with(white, MIN_CONTRAST));
    assert!(white.contrasts_with(grey, MIN_CONTRAST));
    // #777777 is 4.48 to 1 and doesn't.
    assert!(
        !Rgb::parse("#777777")
            .unwrap()
            .contrasts_with(white, MIN_CONTRAST)
    );
}

#[test]
fn refuses_text_or_accent_that_is_hard_to_read() {
    let mut theme = green();
    theme["light"]["text"] = json!("#888888");
    assert_eq!(
        parse(&theme),
        Err(PluginError::ThemeLowContrast {
            key: "light.text".into()
        })
    );
    let mut theme = green();
    theme["dark"]["accent"] = json!("#2E7D32");
    assert_eq!(
        parse(&theme),
        Err(PluginError::ThemeLowContrast {
            key: "dark.accent".into()
        })
    );
}

#[test]
fn light_is_light_and_dark_is_dark() {
    let mut theme = green();
    theme["light"] = green()["dark"].clone();
    assert_eq!(parse(&theme), invalid("wrong-way", "light"));
    let mut theme = green();
    theme["dark"] = green()["light"].clone();
    assert_eq!(parse(&theme), invalid("wrong-way", "dark"));
}

#[test]
fn refuses_anything_it_does_not_know() {
    assert_eq!(Theme::parse(b"not json"), Err(PluginError::ThemeMalformed));
    assert_eq!(Theme::parse(b"[1]"), Err(PluginError::ThemeMalformed));
    assert_eq!(
        parse(&with("theme_version", json!(2))),
        Err(PluginError::UnsupportedThemeVersion { version: 2 })
    );
    assert_eq!(
        parse(&without("theme_version")),
        invalid("missing", "theme_version")
    );
    assert_eq!(
        parse(&with("theme_version", json!("1"))),
        invalid("wrong-type", "theme_version")
    );
    assert_eq!(
        parse(&with("script", json!("x"))),
        invalid("unknown-key", "script")
    );
    assert_eq!(parse(&without("dark")), invalid("missing", "dark"));
    assert_eq!(
        parse(&with("light", json!("#ffffff"))),
        invalid("wrong-type", "light")
    );
    let mut theme = green();
    theme["light"]["link"] = json!("#000000");
    assert_eq!(parse(&theme), invalid("unknown-key", "light.link"));
    let mut theme = green();
    theme["dark"].as_object_mut().unwrap().remove("text");
    assert_eq!(parse(&theme), invalid("missing", "dark.text"));
    let mut theme = green();
    theme["dark"]["text"] = json!(16_777_215);
    assert_eq!(parse(&theme), invalid("wrong-type", "dark.text"));
}

#[test]
fn checks_the_font_and_the_corner_radius() {
    assert_eq!(parse(&with("font", json!(""))), invalid("empty", "font"));
    assert_eq!(
        parse(&with("font", json!(" Sans"))),
        invalid("bad-characters", "font")
    );
    assert_eq!(
        parse(&with("font", json!("Sans\u{202E}"))),
        invalid("bad-characters", "font")
    );
    assert_eq!(
        parse(&with("font", json!("x".repeat(MAX_FONT_CHARS + 1)))),
        invalid("too-long", "font")
    );
    assert_eq!(
        parse(&with("font", json!(12))),
        invalid("wrong-type", "font")
    );
    assert_eq!(
        parse(&with("corner_radius", json!(MAX_CORNER_RADIUS)))
            .unwrap()
            .corner_radius,
        Some(MAX_CORNER_RADIUS)
    );
    assert_eq!(
        parse(&with("corner_radius", json!(0)))
            .unwrap()
            .corner_radius,
        Some(0)
    );
    for radius in [json!(MAX_CORNER_RADIUS + 1), json!(300)] {
        assert_eq!(
            parse(&with("corner_radius", radius)),
            invalid("out-of-range", "corner_radius")
        );
    }
    for radius in [json!(-1), json!(4.5), json!("4")] {
        assert_eq!(
            parse(&with("corner_radius", radius)),
            invalid("wrong-type", "corner_radius")
        );
    }
}

#[test]
fn refuses_a_theme_that_is_too_big() {
    let big = with("font", json!(" ".repeat(MAX_THEME_BYTES)));
    assert_eq!(
        parse(&big),
        Err(PluginError::ThemeTooBig {
            max_bytes: MAX_THEME_BYTES
        })
    );
}

#[test]
fn the_linear_table_matches_the_srgb_curve_at_known_points() {
    // Spot values of IEC 61966-2-1's curve, times 10⁶.
    let luminance = |hex: &str| Rgb::parse(hex).unwrap().luminance();
    // 0x80 = 128 is 0.215861 linear; grey has all three channels.
    assert_eq!(luminance("#808080"), 215_861 * 10_000);
    // 0x0A = 10 sits below the curve's knee: 10 / 255 / 12.92.
    assert_eq!(luminance("#0a0a0a"), 3_035 * 10_000);
}
