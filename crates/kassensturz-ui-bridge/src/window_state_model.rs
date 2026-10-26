// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The main window's size, place and maximized state as a QML object, so the
//! window opens the way it was closed. Kept in the user's settings file as one
//! line, "width height x y maximized".

use core::pin::Pin;

use cxx_qt::CxxQtType;

use crate::settings::Settings;

#[cxx_qt::bridge]
pub mod qobject {
    extern "RustQt" {
        /// What the window looked like when it was closed last.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, known, READ, CONSTANT)]
        #[qproperty(i32, width, READ, CONSTANT)]
        #[qproperty(i32, height, READ, CONSTANT)]
        #[qproperty(i32, x, READ, CONSTANT)]
        #[qproperty(i32, y, READ, CONSTANT)]
        #[qproperty(bool, maximized, READ, CONSTANT)]
        type WindowState = super::WindowStateRust;
    }

    extern "RustQt" {
        /// Keeps the window's size (the one it has when it isn't maximized),
        /// place and maximized state for the next start. Returns whether it
        /// was saved.
        #[qinvokable]
        fn save(
            self: Pin<&mut WindowState>,
            width: i32,
            height: i32,
            x: i32,
            y: i32,
            maximized: bool,
        ) -> bool;
    }

    impl cxx_qt::Initialize for WindowState {}
}

/// The Rust side of [`qobject::WindowState`].
#[derive(Default)]
pub struct WindowStateRust {
    known: bool,
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    maximized: bool,
}

/// The smallest and largest size and the furthest place kept: anything else
/// in the file is damaged or from another screen setup, and ignored.
const MIN_SIZE: i32 = 200;
const MAX_SIZE: i32 = 20_000;
const MAX_PLACE: i32 = 50_000;

/// What the window looked like.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Geometry {
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    maximized: bool,
}

impl Geometry {
    fn parse(line: &str) -> Option<Self> {
        let mut parts = line.split_whitespace();
        let mut number = || parts.next()?.parse::<i32>().ok();
        let (width, height, x, y) = (number()?, number()?, number()?, number()?);
        let maximized = match parts.next()? {
            "0" => false,
            "1" => true,
            _ => return None,
        };
        let sized =
            (MIN_SIZE..=MAX_SIZE).contains(&width) && (MIN_SIZE..=MAX_SIZE).contains(&height);
        let placed = (-MAX_PLACE..=MAX_PLACE).contains(&x) && (-MAX_PLACE..=MAX_PLACE).contains(&y);
        (parts.next().is_none() && sized && placed).then_some(Self {
            width,
            height,
            x,
            y,
            maximized,
        })
    }

    fn line(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.width,
            self.height,
            self.x,
            self.y,
            u8::from(self.maximized)
        )
    }
}

impl cxx_qt::Initialize for qobject::WindowState {
    fn initialize(mut self: Pin<&mut Self>) {
        if let Some(kept) = Geometry::parse(&Settings::user().window_geometry()) {
            let mut state = self.as_mut().rust_mut();
            state.known = true;
            state.width = kept.width;
            state.height = kept.height;
            state.x = kept.x;
            state.y = kept.y;
            state.maximized = kept.maximized;
        }
    }
}

impl qobject::WindowState {
    fn save(
        self: Pin<&mut Self>,
        width: i32,
        height: i32,
        x: i32,
        y: i32,
        maximized: bool,
    ) -> bool {
        let geometry = Geometry {
            width,
            height,
            x,
            y,
            maximized,
        };
        // A size the next start would ignore isn't worth keeping.
        Geometry::parse(&geometry.line()).is_some()
            && Settings::user().set_window_geometry(&geometry.line())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_folder::TestFolder;

    #[test]
    fn reads_back_what_it_wrote() {
        let geometry = Geometry {
            width: 1200,
            height: 800,
            x: -10,
            y: 40,
            maximized: true,
        };
        assert_eq!(Geometry::parse(&geometry.line()), Some(geometry));
    }

    #[test]
    fn keeps_the_line_in_the_settings_file() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert_eq!(Geometry::parse(&settings.window_geometry()), None);
        assert!(settings.set_window_geometry("900 700 20 30 0"));
        assert_eq!(
            Geometry::parse(&Settings::at(&file).window_geometry()),
            Some(Geometry {
                width: 900,
                height: 700,
                x: 20,
                y: 30,
                maximized: false,
            })
        );
    }

    #[test]
    fn ignores_damaged_lines() {
        for line in [
            "",
            "900 700",
            "900 700 0 0 2",
            "10 700 0 0 0",
            "900 700 0 0 0 5",
            "a b c d e",
            "900 99999999 0 0 0",
        ] {
            assert_eq!(Geometry::parse(line), None, "{line:?}");
        }
    }
}
