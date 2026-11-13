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
        #[qproperty(bool, bank_log_on, cxx_name = "bankLogOn", READ, CONSTANT)]
        #[qproperty(bool, bank_log_detailed, cxx_name = "bankLogDetailed", READ, CONSTANT)]
        type DeveloperMode = super::DeveloperModeRust;
    }

    extern "RustQt" {
        /// The log's folder as a `file:` URL, for the banner's button that
        /// shows it; empty when there is none.
        #[qinvokable]
        #[cxx_name = "folderUrl"]
        fn folder_url(self: &DeveloperMode) -> QString;

        /// The bank log's folder as a `file:` URL, for the bank log banner's
        /// button, also while the debug log runs too and has a folder of
        /// its own; empty when the bank log is off.
        #[qinvokable]
        #[cxx_name = "bankLogFolderUrl"]
        fn bank_log_folder_url(self: &DeveloperMode) -> QString;

        /// Deletes the bank log only. Returns whether that worked.
        #[qinvokable]
        #[cxx_name = "clearBankLog"]
        fn clear_bank_log(self: &DeveloperMode) -> bool;

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

        /// The user asked for a screen and the app starts making it:
        /// logged in the developer mode, to time how long screens take.
        #[qinvokable]
        #[cxx_name = "screenRequested"]
        fn screen_requested(self: &DeveloperMode);

        /// The screen QML names has its content and was drawn: logged in
        /// the developer mode. A name outside the fixed list is not logged.
        #[qinvokable]
        #[cxx_name = "screenReady"]
        fn screen_ready(self: &DeveloperMode, name: &QString);

        /// The user did what the action QML names (a button, menu entry,
        /// switch, choice or confirmation, e.g. `RulesPage_addRule`): logged
        /// in the developer mode. A name outside the fixed list of
        /// `kassensturz_core::vocabulary::ACTIONS` is not logged.
        #[qinvokable]
        #[cxx_name = "actionTaken"]
        fn action_taken(self: &DeveloperMode, name: &QString);
    }

    impl cxx_qt::Initialize for DeveloperMode {}
}

/// The Rust side of [`qobject::DeveloperMode`].
#[derive(Default)]
pub struct DeveloperModeRust {
    on: bool,
    bank_log_on: bool,
    bank_log_detailed: bool,
}

impl cxx_qt::Initialize for qobject::DeveloperMode {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().on = is_on();
        self.as_mut().rust_mut().bank_log_on = bank_log_is_on();
        self.as_mut().rust_mut().bank_log_detailed = bank_log_is_detailed();
    }
}

impl qobject::DeveloperMode {
    fn folder_url(&self) -> QString {
        #[cfg(feature = "debug-log")]
        if let Some(folder) =
            kassensturz_core::bank_log::folder().filter(|_| self.bank_log_on && !self.on)
        {
            return QString::from(format!("file://{}", folder.display()).as_str());
        }
        #[cfg(feature = "debug-log")]
        if let Some(folder) = log().folder().filter(|_| self.on) {
            return QString::from(format!("file://{}", folder.display()).as_str());
        }
        QString::default()
    }

    fn bank_log_folder_url(&self) -> QString {
        #[cfg(feature = "debug-log")]
        if let Some(folder) = bank_log_folder(self.bank_log_on) {
            return QString::from(format!("file://{}", folder.display()).as_str());
        }
        QString::default()
    }

    fn clear_bank_log(&self) -> bool {
        #[cfg(feature = "debug-log")]
        return self.bank_log_on && kassensturz_core::bank_log::clear();
        #[cfg(not(feature = "debug-log"))]
        false
    }

    fn clear_log(&self) -> bool {
        // Both logs may run at once; "Delete the log" clears every one that
        // runs, and says it worked only if all of them did.
        #[cfg(feature = "debug-log")]
        return (self.on || self.bank_log_on)
            && (!self.bank_log_on || kassensturz_core::bank_log::clear())
            && (!self.on || log().clear().is_ok());
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

impl qobject::DeveloperMode {
    fn screen_requested(&self) {
        crate::note!(ScreenRequested);
    }

    fn action_taken(&self, name: &QString) {
        // The list is long: look the name up only when something is written.
        if is_on() {
            let action = kassensturz_core::vocabulary::action(&name.to_string());
            crate::note!(ActionTaken, action, None);
        }
    }

    fn screen_ready(&self, name: &QString) {
        let screen = kassensturz_core::vocabulary::screen(&name.to_string());
        if screen.is_some() {
            crate::note!(ScreenReady, screen, None);
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

/// Whether the bank log runs (D-603): started from a terminal with
/// `KASSENSTURZ_BANK_LOG=1`, apart from the debug log. Never in a build
/// without the feature `debug-log`.
pub fn bank_log_is_on() -> bool {
    #[cfg(feature = "debug-log")]
    return kassensturz_core::bank_log::is_on();
    #[cfg(not(feature = "debug-log"))]
    false
}

/// The bank log's folder while the log runs, made if no bank connection
/// wrote to it yet, so "Show the log" opens it (M6-B243). `None` while the
/// log is off or the folder can't be made.
#[cfg(feature = "debug-log")]
fn bank_log_folder(on: bool) -> Option<std::path::PathBuf> {
    made(kassensturz_core::bank_log::folder().filter(|_| on))
}

/// `folder`, made if it isn't there yet; `None` if it can't be.
#[cfg_attr(not(feature = "debug-log"), allow(dead_code))]
fn made(folder: Option<std::path::PathBuf>) -> Option<std::path::PathBuf> {
    let folder = folder?;
    std::fs::create_dir_all(&folder).ok()?;
    Some(folder)
}

/// Whether the bank log also keeps the bank's reply texts, redacted
/// (level 2, D-619).
pub fn bank_log_is_detailed() -> bool {
    #[cfg(feature = "debug-log")]
    return kassensturz_core::bank_log::is_detailed();
    #[cfg(not(feature = "debug-log"))]
    false
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
#[cfg(feature = "ai")]
pub fn transcribe(kind: &'static str, fields: &[(&'static str, &str)]) {
    #[cfg(feature = "debug-log")]
    log().transcript(kind, fields);
    #[cfg(not(feature = "debug-log"))]
    let _ = (kind, fields);
}

/// Notes a step as [`transcribe`] does, under `turn` instead of the current
/// question's: for the decision on changes one of the user's own AI tools
/// prepared, which follows from no question of the chat.
#[cfg(feature = "ai")]
pub fn transcribe_in(turn: &'static str, kind: &'static str, fields: &[(&'static str, &str)]) {
    #[cfg(feature = "debug-log")]
    log().transcript_in(turn, kind, fields);
    #[cfg(not(feature = "debug-log"))]
    let _ = (turn, kind, fields);
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

    /// "Show the log" of the bank log did nothing before a bank connection
    /// had written the folder (M6-B243, US6 P3 VM round 1, A6a).
    #[test]
    fn the_bank_log_folder_is_made_when_it_is_asked_for() {
        let folder = crate::test_folder::TestFolder::new();
        let wanted = folder.join("data/bank-log");
        assert!(!wanted.exists());
        assert_eq!(made(Some(wanted.clone())), Some(wanted.clone()));
        assert!(wanted.is_dir());
        // There already: the same folder, nothing removed.
        fs::write(wanted.join("bank-log.jsonl"), "{}\n").expect("written");
        assert_eq!(made(Some(wanted.clone())), Some(wanted.clone()));
        assert!(wanted.join("bank-log.jsonl").exists());
        assert_eq!(made(None), None);
        // A file in the way: no folder.
        let blocked = folder.join("file");
        fs::write(&blocked, "x").expect("written");
        assert_eq!(made(Some(blocked.join("bank-log"))), None);
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

    /// QML reads a property by the name the object exports, and cxx-qt
    /// exports the Rust name as written: a property of several words needs
    /// its camelCase `cxx_name`, or QML gets `undefined` (M6-B177). The
    /// check reads every bridge source, and the QML that uses the name.
    #[test]
    fn a_property_of_several_words_is_exported_in_camel_case() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut checked = 0;
        for entry in fs::read_dir(src).expect("src folder") {
            let path = entry.expect("entry").path();
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let text = fs::read_to_string(&path).expect("source");
            for declaration in text.split(concat!("#[qprop", "erty(")).skip(1) {
                let Some((declaration, _)) = declaration.split_once(")]") else {
                    continue;
                };
                let declaration = declaration.split_whitespace().collect::<Vec<_>>().join(" ");
                let name = declaration.split(',').nth(1).unwrap_or("").trim();
                if name.contains('_') {
                    checked += 1;
                    assert!(
                        declaration.contains("cxx_name"),
                        "{}: property `{name}` needs cxx_name in camelCase",
                        path.display()
                    );
                }
            }
        }
        assert!(checked >= 3, "the check found only {checked} properties");
    }

    /// The bank log's banner shows whenever the bank log runs, also with
    /// the debug log: its banner doesn't say a second log is written
    /// (#828). Its buttons act on the bank log only.
    #[test]
    fn the_bank_log_banner_shows_while_the_debug_log_runs_too() {
        let qml = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("qml/Main.qml"))
            .expect("Main.qml");
        let banner = qml
            .split("visible: developerMode.bankLogOn")
            .nth(1)
            .expect("the bank log banner");
        assert!(
            banner.starts_with('\n'),
            "the bank log banner must not depend on the debug log"
        );
        assert!(banner.contains("developerMode.bankLogFolderUrl()"));
        assert!(banner.contains("developerMode.clearBankLog()"));
    }

    #[test]
    fn the_developer_mode_is_off_unless_started_with_the_variable() {
        if std::env::var_os("KASSENSTURZ_DEBUG_LOG").is_none() {
            assert!(!is_on());
        }
    }
}
