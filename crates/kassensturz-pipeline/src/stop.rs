// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A request to stop a long import, and how far it got (B-362).
//!
//! An import of tens of thousands of entries runs for seconds on the
//! thread of the window, so the event loop can't hear a SIGTERM (a logout,
//! a shutdown, `pkill`) until it is done: round 7 met an import that the
//! signal didn't stop. The app's signal handler sets this flag, and the
//! import checks it between entries; a stopped import ends with
//! [`kassensturz_import::ImportError::Stopped`], and since an import is one database
//! transaction, the book is as it was before it. Setting the flag is a
//! single atomic store, which a signal handler may do.
//!
//! The user can also cancel an import the app runs on a worker thread
//! ("Abbrechen"): [`cancel`] ends it the same way, with
//! [`kassensturz_import::ImportError::Cancelled`], and [`start`] takes the
//! cancel back for the next one. [`progress`] says how many entries of the
//! current import were handled, for a progress bar.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);
static CANCELLED: AtomicBool = AtomicBool::new(false);
static DONE: AtomicUsize = AtomicUsize::new(0);
static TOTAL: AtomicUsize = AtomicUsize::new(0);

/// Readies the flags for a new import: no cancel, no progress yet. A stop
/// the app was asked for stays.
pub fn start() {
    CANCELLED.store(false, Ordering::Relaxed);
    DONE.store(0, Ordering::Relaxed);
    TOTAL.store(0, Ordering::Relaxed);
}

/// Asks the import under way to stop because the user cancelled it.
pub fn cancel() {
    CANCELLED.store(true, Ordering::Relaxed);
}

/// How many entries of the current import were handled, and how many it
/// has; both 0 before it counted them.
#[must_use]
pub fn progress() -> (usize, usize) {
    (DONE.load(Ordering::Relaxed), TOTAL.load(Ordering::Relaxed))
}

/// Notes that the import has `total` entries to handle.
pub(crate) fn set_total(total: usize) {
    DONE.store(0, Ordering::Relaxed);
    TOTAL.store(total, Ordering::Relaxed);
}

/// Notes one more entry handled.
pub(crate) fn one_done() {
    DONE.fetch_add(1, Ordering::Relaxed);
}

/// Asks every import under way, and every one that starts later, to stop.
/// Safe to call from a signal handler.
pub fn request() {
    REQUESTED.store(true, Ordering::Relaxed);
}

/// Whether a stop was asked for.
#[must_use]
pub fn requested() -> bool {
    REQUESTED.load(Ordering::Relaxed)
}

/// Fails with [`kassensturz_import::ImportError::Stopped`] if a stop was
/// asked for, and with [`kassensturz_import::ImportError::Cancelled`] if the
/// user cancelled.
pub(crate) fn check() -> Result<(), crate::PipelineError> {
    if requested() {
        Err(kassensturz_import::ImportError::Stopped.into())
    } else if CANCELLED.load(Ordering::Relaxed) {
        Err(kassensturz_import::ImportError::Cancelled.into())
    } else {
        Ok(())
    }
}

/// Fails with [`kassensturz_import::ImportError::Stopped`] if a stop was
/// asked for; a cancel is not looked at, for work the user can't cancel.
pub(crate) fn check_requested() -> Result<(), crate::PipelineError> {
    if requested() {
        Err(kassensturz_import::ImportError::Stopped.into())
    } else {
        Ok(())
    }
}
