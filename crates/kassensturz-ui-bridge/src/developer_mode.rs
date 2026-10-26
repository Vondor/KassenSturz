// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The developer mode (D-173, D-227) as a QML object, and the screen the
//! user is on.
//!
//! The developer mode writes a debug log of fixed event names, screens and
//! error codes (`kassensturz_core::debug_log`), and a transcript of the
//! chat in a file of its own (D-235). It starts only when the app
//! is started from a terminal with `KASSENSTURZ_DEBUG_LOG=1`: Settings has
//! no switch for it, and nothing in the app mentions it except the banner
//! and the window title that show while it runs, since the user's privacy
//! can't be ensured then. It is documented in the repository only. A build
//! without the feature `debug-log` holds no log code; the object exists
//! there too, never `on`, so the screens are the same in both builds.
//!
//! The screen the user is on is kept in every build, for the issue drafts
//! of the chat (D-228), by the fixed names of `kassensturz_core::vocabulary`.

use core::pin::Pin;
use std::sync::Mutex;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// The developer mode: whether it runs, and where its log is.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, on, READ, CONSTANT)]
        type DeveloperMode = super::DeveloperModeRust;
    }

    extern "RustQt" {
        /// The log's folder as a `file:` URL, for the banner's button that
        /// shows it; empty when there is none.
        #[qinvokable]
        #[cxx_name = "folderUrl"]
        fn folder_url(self: &DeveloperMode) -> QString;

        /// Deletes everything in the log. Returns whether that worked.
        #[qinvokable]
        #[cxx_name = "clearLog"]
        fn clear_log(self: &DeveloperMode) -> bool;

        /// The screen QML names (e.g. `OverviewPage_QMLTYPE_12(0x…)`) came
        /// up: kept for issue drafts, and logged in the developer mode.
        /// A name outside the fixed list counts as no screen.
        #[qinvokable]
        #[cxx_name = "screenShown"]
        fn screen_shown(self: &DeveloperMode, name: &QString);
    }

    impl cxx_qt::Initialize for DeveloperMode {}
}

/// The Rust side of [`qobject::DeveloperMode`].
#[derive(Default)]
pub struct DeveloperModeRust {
    on: bool,
}

impl cxx_qt::Initialize for qobject::DeveloperMode {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().on = is_on();
    }
}

impl qobject::DeveloperMode {
    fn folder_url(&self) -> QString {
        #[cfg(feature = "debug-log")]
        if let Some(folder) = log().folder().filter(|_| self.on) {
            return QString::from(format!("file://{}", folder.display()).as_str());
        }
        QString::default()
    }

    fn clear_log(&self) -> bool {
        #[cfg(feature = "debug-log")]
        return self.on && log().clear().is_ok();
        #[cfg(not(feature = "debug-log"))]
        false
    }

    fn screen_shown(&self, name: &QString) {
        let screen = kassensturz_core::vocabulary::screen(&name.to_string());
        set_current_screen(screen);
        if screen.is_some() {
            crate::note!(ScreenShown, screen, None);
        }
    }
}

/// The screen the user is on, by its fixed name.
static CURRENT_SCREEN: Mutex<Option<&'static str>> = Mutex::new(None);

fn set_current_screen(screen: Option<&'static str>) {
    if let Ok(mut current) = CURRENT_SCREEN.lock() {
        *current = screen;
    }
}

/// The screen the user is on, by its fixed name, if one is known: for the
/// chat's issue drafts, so only in a build with AI.
#[cfg(feature = "ai")]
pub fn current_screen() -> Option<&'static str> {
    CURRENT_SCREEN.lock().ok().and_then(|current| *current)
}

/// Whether the developer mode runs.
pub fn is_on() -> bool {
    #[cfg(feature = "debug-log")]
    return log().is_on();
    #[cfg(not(feature = "debug-log"))]
    false
}

/// The developer mode's log, as the app was started: the variable is read
/// once.
#[cfg(feature = "debug-log")]
pub fn log() -> &'static kassensturz_core::debug_log::DebugLog {
    static LOG: std::sync::OnceLock<kassensturz_core::debug_log::DebugLog> =
        std::sync::OnceLock::new();
    LOG.get_or_init(kassensturz_core::debug_log::DebugLog::user)
}

/// Notes a step in the developer mode's transcript of the chat (D-235); a
/// call that does nothing outside the developer mode and in a build
/// without the debug log. `fields` are the step's texts.
pub fn transcribe(kind: &'static str, fields: &[(&'static str, &str)]) {
    #[cfg(feature = "debug-log")]
    log().transcript(kind, fields);
    #[cfg(not(feature = "debug-log"))]
    let _ = (kind, fields);
}

/// Notes the event `Event::$event` of `kassensturz_core::debug_log` in the
/// developer mode's log, with a detail and an error code (both
/// `Option<&'static str>`); nothing outside the developer mode, and no code
/// at all in a build without the debug log.
#[macro_export]
macro_rules! note {
    ($event:ident) => {
        $crate::note!($event, None, None)
    };
    ($event:ident, $detail:expr, $code:expr) => {{
        #[cfg(feature = "debug-log")]
        $crate::developer_mode::log().record(
            kassensturz_core::debug_log::Event::$event,
            $detail,
            $code,
        );
        #[cfg(not(feature = "debug-log"))]
        let _: (Option<&'static str>, Option<&'static str>) = ($detail, $code);
    }};
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    #[cfg(feature = "ai")]
    #[test]
    fn the_current_screen_is_one_of_the_fixed_names() {
        set_current_screen(kassensturz_core::vocabulary::screen(
            "RegisterPage_QMLTYPE_7(0x1)",
        ));
        assert_eq!(current_screen(), Some("RegisterPage"));
        set_current_screen(kassensturz_core::vocabulary::screen("Moon Base Rent"));
        assert_eq!(current_screen(), None);
    }

    #[test]
    fn every_page_has_a_fixed_name() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("qml");
        for entry in fs::read_dir(folder).expect("qml folder") {
            let name = entry.expect("entry").file_name();
            let name = name.to_string_lossy();
            if let Some(page) = name.strip_suffix("Page.qml") {
                let page = format!("{page}Page");
                assert_eq!(
                    kassensturz_core::vocabulary::screen(&page),
                    Some(page.as_str()),
                    "add {page} to kassensturz_core::vocabulary::DETAILS"
                );
            }
        }
    }

    #[test]
    fn the_developer_mode_is_off_unless_started_with_the_variable() {
        if std::env::var_os("KASSENSTURZ_DEBUG_LOG").is_none() {
            assert!(!is_on());
        }
    }
}
