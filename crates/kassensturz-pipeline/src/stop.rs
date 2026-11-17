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
//!
//! An import can be cancelled until it starts to save: [`saving`] says
//! when it got there (B-367). From then on a cancel comes too late and
//! changes nothing, so the import is either saved whole or not at all.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);
/// Where the import under way is: [`RUNNING`], [`CANCELLED`] or
/// [`SAVING`]. One value, so a cancel and the start of the save can't
/// both win.
static STATE: AtomicU8 = AtomicU8::new(RUNNING);
const RUNNING: u8 = 0;
const CANCELLED: u8 = 1;
const SAVING: u8 = 2;
static DONE: AtomicUsize = AtomicUsize::new(0);
static TOTAL: AtomicUsize = AtomicUsize::new(0);

/// Readies the flags for a new import: no cancel, no progress yet. A stop
/// the app was asked for stays.
pub fn start() {
    STATE.store(RUNNING, Ordering::SeqCst);
    DONE.store(0, Ordering::Relaxed);
    TOTAL.store(0, Ordering::Relaxed);
}

/// Asks the import under way to stop because the user cancelled it.
/// Returns whether that came in time: an import that is saving goes on
/// (B-367).
pub fn cancel() -> bool {
    match STATE.compare_exchange(RUNNING, CANCELLED, Ordering::SeqCst, Ordering::SeqCst) {
        Ok(_) => true,
        Err(state) => state == CANCELLED,
    }
}

/// Whether the import under way is saving, so a cancel comes too late.
#[must_use]
pub fn saving() -> bool {
    STATE.load(Ordering::SeqCst) == SAVING
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
    } else if STATE.load(Ordering::SeqCst) == CANCELLED {
        Err(kassensturz_import::ImportError::Cancelled.into())
    } else {
        Ok(())
    }
}

/// The last [`check`] before the import saves: if the user hasn't
/// cancelled by now, the import is saving, and a later cancel changes
/// nothing.
pub(crate) fn check_before_saving() -> Result<(), crate::PipelineError> {
    check()?;
    match STATE.compare_exchange(RUNNING, SAVING, Ordering::SeqCst, Ordering::SeqCst) {
        Err(CANCELLED) => Err(kassensturz_import::ImportError::Cancelled.into()),
        _ => Ok(()),
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
