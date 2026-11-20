// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Theme plugins (#791 step 6, D-756, D-925): the user picks one in
//! Settings › Look, the choice is kept as the plugin's ID under
//! `Look/Theme` (D-924), and it applies from the next start.
//!
//! A plugin only describes the colours; KassenSturz draws with them. From a
//! theme's background, text and accent this module works out every colour
//! a KDE colour scheme has, and keeps each text colour readable on its
//! background ([`MIN_CONTRAST`]), so the warnings and the red of a negative
//! amount stay readable whatever the theme. The scheme goes to a file that
//! Kirigami's style reads (`KDE_COLOR_SCHEME_PATH`), and the same colours
//! become Qt's palette for the widgets. Only the theme's light or dark
//! look is used, after the system's setting at start.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use cxx_qt_lib::QString;
use kassensturz_plugin_api::{Colours, ExtensionPoint, MIN_CONTRAST, Package, Rgb, Theme};

use crate::settings::Settings;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;
    }

    #[namespace = "kassensturz::theme"]
    unsafe extern "C++" {
        include!("kassensturz-ui-bridge/cpp/theme.h");

        #[cxx_name = "systemPrefersDark"]
        fn system_prefers_dark() -> bool;

        #[cxx_name = "applyTheme"]
        fn apply_theme(scheme_path: &QString, palette: &QString, font: &QString);
    }
}

/// The settings group and key of the chosen theme (D-924).
const GROUP: &str = "Look";
const KEY: &str = "Theme";
/// The file in the app's data folder that holds the colour scheme made
/// from the chosen theme; it is written again at every start.
const SCHEME_FILE: &str = "theme.colors";

/// A theme plugin in the plugin folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// Its ID, which the settings keep.
    pub id: String,
    /// Its name, as its manifest has it.
    pub name: String,
    /// Its colours.
    pub theme: Theme,
}

/// Every usable theme plugin in `folder`, sorted by name.
pub fn installed(folder: &Path) -> Vec<Installed> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut themes: Vec<Installed> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let bytes = crate::plugins::read(&entry.path()).ok()?;
            let package = Package::read(&bytes).ok()?;
            if !package
                .manifest
                .extension_points
                .contains(&ExtensionPoint::Theme)
            {
                return None;
            }
            Some(Installed {
                id: package.manifest.id.as_str().to_owned(),
                name: package.manifest.name,
                theme: package.theme?,
            })
        })
        .collect();
    themes.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    themes
}

/// The ID of the chosen theme, or `None` for KassenSturz's own look.
pub fn chosen(settings: &Settings) -> Option<String> {
    Some(settings.text(GROUP, KEY)).filter(|id| !id.is_empty())
}

/// Chooses the theme with `id`, or KassenSturz's own look for `None`, from
/// the next start on. Returns whether it was saved.
pub fn choose(settings: &Settings, id: Option<&str>) -> bool {
    settings.set_text(GROUP, KEY, id.unwrap_or_default())
}

/// What applies since this start: the theme's ID and its corner radius.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InEffect {
    /// The theme's ID, `None` for KassenSturz's own look.
    pub id: Option<String>,
    /// Its corner radius, if it gives one.
    pub corner_radius: Option<u8>,
}

static IN_EFFECT: OnceLock<InEffect> = OnceLock::new();

/// The theme that applies since this start.
pub fn in_effect() -> InEffect {
    IN_EFFECT.get().cloned().unwrap_or_default()
}

/// Applies the chosen theme, if there is one and it can still be used.
/// Call it after the QApplication exists and before the first window.
/// Returns whether a theme applies, so the caller leaves the system's
/// colours alone then.
pub fn apply_at_start() -> bool {
    let id = chosen(&Settings::user());
    let found = id.as_deref().and_then(|id| {
        let folder = crate::plugins::folder()?;
        installed(&folder).into_iter().find(|theme| theme.id == id)
    });
    let Some(found) = found else {
        let _ = IN_EFFECT.set(InEffect::default());
        return false;
    };
    let colours = if ffi::system_prefers_dark() {
        found.theme.dark
    } else {
        found.theme.light
    };
    let roles = Roles::of(&colours);
    let Some(path) = write_scheme(&roles) else {
        let _ = IN_EFFECT.set(InEffect::default());
        return false;
    };
    ffi::apply_theme(
        &QString::from(path.to_string_lossy().as_ref()),
        &QString::from(roles.palette().as_str()),
        &QString::from(found.theme.font.as_deref().unwrap_or_default()),
    );
    let _ = IN_EFFECT.set(InEffect {
        id: Some(found.id),
        corner_radius: found.theme.corner_radius,
    });
    true
}

/// Writes the colour scheme to the app's data folder. Returns its path.
fn write_scheme(roles: &Roles) -> Option<PathBuf> {
    let folder = crate::platform::app_folder()?;
    fs::create_dir_all(&folder).ok()?;
    let path = folder.join(SCHEME_FILE);
    fs::write(&path, roles.scheme()).ok()?;
    Some(path)
}

/// `a` moved `percent` of the way to `b`, rounded, per channel.
fn mix(a: Rgb, b: Rgb, percent: u8) -> Rgb {
    let percent = u16::from(percent.min(100));
    let channel = |x: u8, y: u8| {
        let mixed = (u16::from(x) * (100 - percent) + u16::from(y) * percent + 50) / 100;
        u8::try_from(mixed).unwrap_or(u8::MAX)
    };
    Rgb {
        red: channel(a.red, b.red),
        green: channel(a.green, b.green),
        blue: channel(a.blue, b.blue),
    }
}

/// `colour`, or the first colour on the way from it to `text` that is
/// readable on `background`. `text` itself is readable there (the theme was
/// checked), so there always is one.
fn readable(colour: Rgb, text: Rgb, background: Rgb) -> Rgb {
    (0..=20)
        .map(|step| mix(colour, text, step * 5))
        .find(|candidate| candidate.contrasts_with(background, MIN_CONTRAST))
        .unwrap_or(text)
}

/// `background` moved up to `percent` of the way to `text`, as far as
/// `text` stays readable on it.
fn backdrop(background: Rgb, text: Rgb, percent: u8) -> Rgb {
    (0..=percent)
        .rev()
        .map(|step| mix(background, text, step))
        .find(|candidate| text.contrasts_with(*candidate, MIN_CONTRAST))
        .unwrap_or(background)
}

/// Black or white, whichever stands out more from `background`.
fn on(background: Rgb) -> Rgb {
    let black = Rgb {
        red: 0,
        green: 0,
        blue: 0,
    };
    let white = Rgb {
        red: 255,
        green: 255,
        blue: 255,
    };
    // White's contrast is (1.05) / (L + 0.05), black's (L + 0.05) / 0.05;
    // white wins while L + 0.05 is below √(1.05 · 0.05) ≈ 0.2291.
    if background.luminance() < 1_791_287_847 {
        white
    } else {
        black
    }
}

/// Every colour the app draws with, worked out from one look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Roles {
    /// Behind lists, fields and pages.
    pub view: Rgb,
    /// Behind the window's frame, toolbars and headers.
    pub window: Rgb,
    /// Behind buttons.
    pub button: Rgb,
    /// Every other row of a list.
    pub alternate: Rgb,
    /// Text.
    pub text: Rgb,
    /// Secondary text, still readable.
    pub inactive: Rgb,
    /// Selections, focus and links.
    pub accent: Rgb,
    /// Text on the accent.
    pub on_accent: Rgb,
    /// A link already visited.
    pub visited: Rgb,
    /// Problems and amounts below zero.
    pub negative: Rgb,
    /// Warnings.
    pub neutral: Rgb,
    /// Things that went well.
    pub positive: Rgb,
}

impl Roles {
    /// The roles of `colours`. The warning colours are KassenSturz's own,
    /// made readable on this background.
    pub fn of(colours: &Colours) -> Self {
        let Colours {
            background,
            text,
            accent,
        } = *colours;
        let dark = background.luminance() < text.luminance();
        let hex = |text: &str| Rgb::parse(text).unwrap_or(colours.text);
        let (negative, neutral, positive) = if dark {
            (hex("#ff8a80"), hex("#ffcc80"), hex("#a5d6a7"))
        } else {
            (hex("#c62828"), hex("#8a5300"), hex("#2e7d32"))
        };
        let window = backdrop(background, text, 4);
        let button = backdrop(background, text, 8);
        // Text sits on every background, so it must be readable on the
        // darkest of them as well as the lightest.
        let check = |colour: Rgb| readable(readable(colour, text, background), text, button);
        Self {
            view: background,
            window,
            button,
            alternate: mix(background, text, 3),
            text,
            inactive: check(mix(text, background, 40)),
            accent,
            on_accent: on(accent),
            visited: check(mix(accent, text, 30)),
            negative: check(negative),
            neutral: check(neutral),
            positive: check(positive),
        }
    }

    /// The roles as Qt palette roles, `Role=#rrggbb` separated by `;`, as
    /// `applyTheme` reads them.
    pub fn palette(&self) -> String {
        [
            ("Window", self.window),
            ("WindowText", self.text),
            ("Base", self.view),
            ("AlternateBase", self.alternate),
            ("Text", self.text),
            ("Button", self.button),
            ("ButtonText", self.text),
            ("Highlight", self.accent),
            ("HighlightedText", self.on_accent),
            ("Link", self.accent),
            ("LinkVisited", self.visited),
            ("ToolTipBase", self.window),
            ("ToolTipText", self.text),
            ("PlaceholderText", self.inactive),
            ("Accent", self.accent),
        ]
        .iter()
        .map(|(role, colour)| format!("{role}={}", colour.hex()))
        .collect::<Vec<_>>()
        .join(";")
    }

    /// The roles as a KDE colour scheme file.
    pub fn scheme(&self) -> String {
        let rgb = |colour: Rgb| format!("{},{},{}", colour.red, colour.green, colour.blue);
        let mut file = String::new();
        // How disabled and inactive parts are drawn: the usual KDE effects
        // (numbers only), so a disabled control looks disabled.
        file.push_str(
            "[ColorEffects:Disabled]\nColorAmount=0\nColorEffect=0\nContrastAmount=0.65\n\
             ContrastEffect=1\nIntensityAmount=0.1\nIntensityEffect=2\n\n\
             [ColorEffects:Inactive]\nChangeSelectionColor=false\nEnable=false\n\n",
        );
        let set = |file: &mut String, name: &str, background: Rgb, alternate: Rgb, text: Rgb| {
            let inactive = if text == self.text {
                self.inactive
            } else {
                text
            };
            let entries = [
                ("BackgroundAlternate", alternate),
                ("BackgroundNormal", background),
                ("DecorationFocus", self.accent),
                ("DecorationHover", self.accent),
                (
                    "ForegroundActive",
                    if text == self.text { self.accent } else { text },
                ),
                ("ForegroundInactive", inactive),
                (
                    "ForegroundLink",
                    if text == self.text { self.accent } else { text },
                ),
                (
                    "ForegroundNegative",
                    if text == self.text {
                        self.negative
                    } else {
                        text
                    },
                ),
                (
                    "ForegroundNeutral",
                    if text == self.text {
                        self.neutral
                    } else {
                        text
                    },
                ),
                ("ForegroundNormal", text),
                (
                    "ForegroundPositive",
                    if text == self.text {
                        self.positive
                    } else {
                        text
                    },
                ),
                (
                    "ForegroundVisited",
                    if text == self.text {
                        self.visited
                    } else {
                        text
                    },
                ),
            ];
            file.push_str(&format!("[Colors:{name}]\n"));
            for (key, colour) in entries {
                file.push_str(&format!("{key}={}\n", rgb(colour)));
            }
            file.push('\n');
        };
        set(&mut file, "Button", self.button, self.window, self.text);
        set(
            &mut file,
            "Complementary",
            self.window,
            self.alternate,
            self.text,
        );
        set(&mut file, "Header", self.window, self.alternate, self.text);
        set(
            &mut file,
            "Selection",
            self.accent,
            self.accent,
            self.on_accent,
        );
        set(&mut file, "Tooltip", self.window, self.alternate, self.text);
        set(&mut file, "View", self.view, self.alternate, self.text);
        set(&mut file, "Window", self.window, self.alternate, self.text);
        file.push_str("[General]\nColorScheme=KassenSturzTheme\nName=KassenSturz theme\n\n[KDE]\ncontrast=4\n");
        file
    }
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
