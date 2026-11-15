// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The identity of a book (B-328, D-804, D-806).
//!
//! A book's file name is not who it is: two books called `Household.ksz` in
//! different folders are two books. Each book keeps a random 16-byte ID in
//! the one-row table `book_identity` (layout 34), which stays with the book
//! through a move, a rename and a copy of the file. Backups are tied to it:
//! their names carry a short tag of it, so the freshness checks, the
//! pruning, the copies in a chosen folder and the list to restore from all
//! find a book's own backups and nobody else's.

use std::fmt;

use rusqlite::{Connection, OptionalExtension};

/// How many hexadecimal digits of the ID the tag in a backup's name has:
/// 48 bits, so two books of one person never share one by chance.
const TAG_DIGITS: usize = 12;

/// The ID of a book.
#[derive(Clone, PartialEq, Eq)]
pub struct BookId([u8; 16]);

impl BookId {
    /// The ID the book that `conn` has open keeps, or `None` if it has none
    /// yet: a book of a layout before 34, which gets its ID when it is
    /// updated.
    pub(crate) fn try_read(conn: &Connection) -> Option<Self> {
        let bytes: Option<Vec<u8>> = conn
            .query_row(
                "SELECT book_id FROM book_identity WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten();
        Self::from_bytes(&bytes?)
    }

    /// The ID made of `bytes`, if they are 16.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        Some(Self(bytes.try_into().ok()?))
    }

    /// The short tag in the names of this book's backups: 12 lower-case
    /// hexadecimal digits, e.g. `a1b2c3d4e5f6`.
    #[must_use]
    pub fn tag(&self) -> String {
        use fmt::Write;
        let mut tag = String::with_capacity(TAG_DIGITS);
        for byte in &self.0[..TAG_DIGITS / 2] {
            let _ = write!(tag, "{byte:02x}");
        }
        tag
    }
}

impl fmt::Debug for BookId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BookId({})", self.tag())
    }
}
