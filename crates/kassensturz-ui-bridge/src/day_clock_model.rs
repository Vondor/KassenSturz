// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The app's clock for "today" as a QML object (#831).
//!
//! The main window asks `check()` once in a while. When the day has changed
//! since the last look, an app left open over midnight, a month end or a
//! suspended laptop, `check()` says so and every screen reads the book again,
//! so "today" and "this month" are never the figures of the old day. The
//! work is in `session::note_today`.

use core::pin::Pin;

use crate::format::Locale;
use crate::number_format::app_locale;
use crate::session;

#[cxx_qt::bridge]
pub mod qobject {
    extern "RustQt" {
        /// Looks at the clock.
        #[qobject]
        #[qml_element]
        type DayClock = super::DayClockRust;
    }

    extern "RustQt" {
        /// Returns whether the day is another than at the last look. The
        /// first look never counts: the screens were just made with it.
        #[qinvokable]
        fn check(self: Pin<&mut DayClock>) -> bool;
    }
}

/// The Rust side of [`qobject::DayClock`]; the date it saw is kept in the
/// session, with the count of changes it moves.
#[derive(Default)]
pub struct DayClockRust;

impl qobject::DayClock {
    fn check(self: Pin<&mut Self>) -> bool {
        session::note_today(app_locale().today())
    }
}
