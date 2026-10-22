// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The book the app has open, shared by every screen's QML object.
//!
//! `BookSession` (`book_session.rs`) opens and closes it; the objects behind
//! the screens reach it through [`with_book`] and [`with_book_mut`], so each
//! screen can have its own Rust file and QML file. QML objects live on the
//! GUI thread only, so the book is kept for that thread.

use std::cell::RefCell;
use std::path::PathBuf;

use kassensturz_store::Book;

use crate::books::OpenBook;

thread_local! {
    static OPEN: RefCell<Option<OpenBook>> = const { RefCell::new(None) };
}

/// Makes `book` the open book. The one open before is closed, which
/// removes its lock file (D-038).
pub fn set(book: OpenBook) {
    // Taken out first, so it is closed outside the borrow.
    let previous = OPEN.with(|open| open.borrow_mut().replace(book));
    drop(previous);
}

/// Closes the open book, if there is one, removing its lock file. The app
/// calls this before it ends, since a thread's values aren't dropped when
/// the process exits.
pub fn close() {
    let previous = OPEN.with(|open| open.borrow_mut().take());
    drop(previous);
}

/// Where the open book is, if one is open.
pub fn path() -> Option<PathBuf> {
    OPEN.with(|open| open.borrow().as_ref().map(|open| open.path.clone()))
}

/// Whether the open book is on a network drive (D-038).
pub fn on_network_drive() -> bool {
    OPEN.with(|open| {
        open.borrow()
            .as_ref()
            .is_some_and(|open| open.on_network_drive)
    })
}

/// Runs `f` with the open book; `None` if no book is open.
pub fn with_book<T>(f: impl FnOnce(&Book) -> T) -> Option<T> {
    OPEN.with(|open| open.borrow().as_ref().map(|open| f(&open.book)))
}

/// Runs `f` with the open book to change it; `None` if no book is open.
///
/// `f` must not open or close books itself.
pub fn with_book_mut<T>(f: impl FnOnce(&mut Book) -> T) -> Option<T> {
    OPEN.with(|open| open.borrow_mut().as_mut().map(|open| f(&mut open.book)))
}
