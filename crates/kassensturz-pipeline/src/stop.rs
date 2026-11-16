// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A request to stop a long import (B-362).
//!
//! An import of tens of thousands of entries runs for seconds on the
//! thread of the window, so the event loop can't hear a SIGTERM (a logout,
//! a shutdown, `pkill`) until it is done: round 7 met an import that the
//! signal didn't stop. The app's signal handler sets this flag, and the
//! import checks it between entries; a stopped import ends with
//! [`kassensturz_import::ImportError::Stopped`], and since an import is one database
//! transaction, the book is as it was before it. Setting the flag is a
//! single atomic store, which a signal handler may do.

use std::sync::atomic::{AtomicBool, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);

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

/// Fails with [`kassensturz_import::ImportError::Stopped`] if a stop was asked for.
pub(crate) fn check() -> Result<(), crate::PipelineError> {
    if requested() {
        Err(kassensturz_import::ImportError::Stopped.into())
    } else {
        Ok(())
    }
}
