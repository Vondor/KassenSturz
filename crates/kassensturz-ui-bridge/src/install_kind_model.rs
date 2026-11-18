// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How this copy of KassenSturz was installed, for the testing guide
//! (`TestingGuidePage.qml`, #1030, D-792): a tester sees it next to the
//! version, and it goes into the note about a problem, so whoever reads the
//! note knows which install sheet the tester followed. It is read once when
//! the object is made and never stored.

use core::pin::Pin;

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
        /// How the app was installed: `flatpak`, `snap`, `windows`,
        /// `macos` or `other` (a distribution's package or a build from
        /// source). The page puts it into words.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, kind, READ, CONSTANT)]
        type InstallKind = super::InstallKindRust;
    }

    impl cxx_qt::Initialize for InstallKind {}
}

/// The Rust side of [`qobject::InstallKind`].
#[derive(Default)]
pub struct InstallKindRust {
    kind: QString,
}

impl cxx_qt::Initialize for qobject::InstallKind {
    fn initialize(mut self: Pin<&mut Self>) {
        let kind = kind(
            crate::platform::in_flatpak_sandbox(),
            crate::platform::in_snap_sandbox(),
            System::running(),
        );
        self.as_mut().rust_mut().kind = QString::from(kind);
    }
}

/// The operating system the app was built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum System {
    Linux,
    Windows,
    MacOs,
    Other,
}

impl System {
    /// The system this build runs on.
    fn running() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }
}

/// The install's kind: the sandbox if there is one, otherwise the system.
/// A Flatpak and a snap exist on Linux only; a Linux build outside both is a
/// distribution's package or a build from source, `other`.
fn kind(flatpak: bool, snap: bool, system: System) -> &'static str {
    match system {
        System::Windows => "windows",
        System::MacOs => "macos",
        System::Linux if flatpak => "flatpak",
        System::Linux if snap => "snap",
        System::Linux | System::Other => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sandbox_names_the_linux_install() {
        assert_eq!(kind(true, false, System::Linux), "flatpak");
        assert_eq!(kind(false, true, System::Linux), "snap");
        assert_eq!(kind(false, false, System::Linux), "other");
    }

    #[test]
    fn the_flatpak_wins_when_both_are_seen() {
        // A snap's variables set inside a Flatpak are not a snap.
        assert_eq!(kind(true, true, System::Linux), "flatpak");
    }

    #[test]
    fn windows_and_macos_name_themselves() {
        assert_eq!(kind(false, false, System::Windows), "windows");
        assert_eq!(kind(true, true, System::Windows), "windows");
        assert_eq!(kind(false, false, System::MacOs), "macos");
        assert_eq!(kind(false, false, System::Other), "other");
    }

    #[test]
    fn the_running_system_is_one_of_the_known_kinds() {
        let kind = kind(false, false, System::running());
        assert!(["windows", "macos", "other"].contains(&kind), "{kind}");
    }

    /// The page reads exactly these words; a new kind needs words there.
    #[test]
    fn the_page_puts_every_kind_into_words() {
        let page = include_str!("../qml/TestingGuidePage.qml");
        for kind in ["flatpak", "snap", "windows", "macos"] {
            assert!(
                page.contains(&format!("case \"{kind}\":")),
                "TestingGuidePage.qml needs words for {kind}"
            );
        }
    }
}
