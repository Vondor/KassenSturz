// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The book the app has open, shared by every screen's QML object.
//!
//! `BookSession` (`book_session.rs`) opens and closes it; the objects behind
//! the screens reach it through [`with_book`] and [`with_book_mut`], so each
//! screen can have its own Rust file and QML file. QML objects live on the
//! GUI thread only, so the book is kept for that thread.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use kassensturz_report::{Period, ReportError, Source};
use kassensturz_store::Book;

use crate::books::OpenBook;

thread_local! {
    static OPEN: RefCell<Option<OpenBook>> = const { RefCell::new(None) };
    /// Counts every time the open book may have changed: a book opened or
    /// closed, or handed out to be changed. What was read from the book
    /// under an older count is out of date (see [`cached_source`]).
    static CHANGES: Cell<u64> = const { Cell::new(0) };
    static SOURCES: RefCell<SourceCache> = const { RefCell::new(SourceCache::new()) };
}

/// How many readings of the book [`cached_source`] keeps. The Overview asks
/// for a handful of different spans and periods; a register or a report the
/// user opens next replaces the oldest.
const KEPT_SOURCES: usize = 8;

/// What was read from the open book, for the count of [`CHANGES`] it was
/// read under.
struct SourceCache {
    changes: u64,
    kept: Vec<(Period, bool, Rc<Source>)>,
}

impl SourceCache {
    const fn new() -> Self {
        Self {
            changes: 0,
            kept: Vec::new(),
        }
    }
}

fn changed() {
    CHANGES.with(|changes| changes.set(changes.get().wrapping_add(1)));
}

/// Makes `book` the open book. The one open before is closed, which
/// removes its lock file (D-038).
pub fn set(book: OpenBook) {
    changed();
    // Taken out first, so it is closed outside the borrow.
    let previous = OPEN.with(|open| open.borrow_mut().replace(book));
    drop(previous);
}

/// Closes the open book, if there is one, removing its lock file. The app
/// calls this before it ends, since a thread's values aren't dropped when
/// the process exits.
pub fn close() {
    changed();
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
    changed();
    OPEN.with(|open| open.borrow_mut().as_mut().map(|open| f(&mut open.book)))
}

/// What `read` makes of the open book (a [`Source`] of `span`, with the
/// accounts' balances if `balances`), kept until the book changes (M6-B30).
///
/// The Overview shows a dozen components that each ask for the same few
/// readings of the book; reading it once per component made opening it take
/// a second with years of history. Only the book this session has open is
/// cached, and only while nothing has changed it since: a book opened or
/// closed, or handed out by [`with_book_mut`], throws every reading away.
/// For any other `book` (a test's, or a second reader's) `read` runs every
/// time. Everything a [`Source`] holds comes from the book and `span`, so the
/// time of day or the number format never make a kept one out of date.
pub fn cached_source(
    book: &Book,
    span: Period,
    balances: bool,
    read: impl FnOnce() -> Result<Source, ReportError>,
) -> Result<Rc<Source>, ReportError> {
    // The book `with_book` hands out is the one in `OPEN`; while it is
    // handed out to be changed, `OPEN` is borrowed and nothing is kept.
    let is_open = OPEN.with(|open| {
        open.try_borrow().is_ok_and(|open| {
            open.as_ref()
                .is_some_and(|open| core::ptr::eq(&open.book, book))
        })
    });
    if !is_open {
        return read().map(Rc::new);
    }
    let changes = CHANGES.with(Cell::get);
    let kept = SOURCES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.changes != changes {
            cache.kept.clear();
            cache.changes = changes;
        }
        cache
            .kept
            .iter()
            .find(|(kept_span, with, _)| *kept_span == span && *with == balances)
            .map(|(.., source)| Rc::clone(source))
    });
    if let Some(source) = kept {
        return Ok(source);
    }
    let source = Rc::new(read()?);
    SOURCES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.changes == changes {
            if cache.kept.len() == KEPT_SOURCES {
                cache.kept.remove(0);
            }
            cache.kept.push((span, balances, Rc::clone(&source)));
        }
    });
    Ok(source)
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use super::*;

    fn period() -> Period {
        let day = kassensturz_core::Date::new(2026, 10, 1).expect("a date");
        Period::new(day, day).expect("a period")
    }

    fn open(book: Book) {
        set(OpenBook {
            book,
            path: PathBuf::new(),
            on_network_drive: false,
        });
    }

    fn read_count(span: Period, balances: bool, reads: &Cell<usize>) -> Rc<Source> {
        with_book(|book| {
            cached_source(book, span, balances, || {
                reads.set(reads.get() + 1);
                Source::read(book, span)
            })
            .expect("read")
        })
        .expect("a book is open")
    }

    #[test]
    fn the_open_book_is_read_once_until_it_changes() {
        let reads = Cell::new(0);
        open(Book::create_in_memory().expect("book"));
        let first = read_count(period(), false, &reads);
        let again = read_count(period(), false, &reads);
        assert_eq!(reads.get(), 1);
        assert!(Rc::ptr_eq(&first, &again));

        // Another span, or with balances, is another reading.
        let later = kassensturz_core::Date::new(2026, 10, 2).expect("a date");
        read_count(Period::new(later, later).expect("a period"), false, &reads);
        read_count(period(), true, &reads);
        assert_eq!(reads.get(), 3);

        // A book handed out to be changed, even if nothing is, throws the
        // readings away.
        with_book_mut(|_| ()).expect("a book is open");
        let after = read_count(period(), false, &reads);
        assert_eq!(reads.get(), 4);
        assert!(!Rc::ptr_eq(&first, &after));

        // So does opening another book, though its readings look the same.
        open(Book::create_in_memory().expect("book"));
        read_count(period(), false, &reads);
        assert_eq!(reads.get(), 5);
        close();
    }

    #[test]
    fn a_book_that_is_not_the_open_one_is_read_every_time() {
        let reads = Cell::new(0);
        open(Book::create_in_memory().expect("book"));
        let other = Book::create_in_memory().expect("book");
        for _ in 0..2 {
            cached_source(&other, period(), false, || {
                reads.set(reads.get() + 1);
                Source::read(&other, period())
            })
            .expect("read");
        }
        assert_eq!(reads.get(), 2);
        close();
    }

    #[test]
    fn the_oldest_reading_makes_room_for_a_new_one() {
        let reads = Cell::new(0);
        open(Book::create_in_memory().expect("book"));
        let day = |number: i8| {
            let day = kassensturz_core::Date::new(2026, 10, number).expect("a date");
            Period::new(day, day).expect("a period")
        };
        for number in 1..=i8::try_from(KEPT_SOURCES).expect("small") + 1 {
            read_count(day(number), false, &reads);
        }
        let all = reads.get();
        // The newest is still kept; the first was dropped.
        read_count(
            day(i8::try_from(KEPT_SOURCES).expect("small") + 1),
            false,
            &reads,
        );
        assert_eq!(reads.get(), all);
        read_count(day(1), false, &reads);
        assert_eq!(reads.get(), all + 1);
        close();
    }
}
