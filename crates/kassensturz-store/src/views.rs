// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved views and dashboards: JSON documents the book keeps (D-278, D-279;
//! layout 20). The book checks only what its columns copy out of a document
//! (format, version, kind and name) and its size; reading the components and
//! checking them against their property schema is the reporting layer's job
//! (D-281). A document is data: it is stored and returned as text and never
//! run or interpreted here.

use kassensturz_core::{StoredView, ViewId, ViewKind};
use rusqlite::{OptionalExtension, params};

use crate::Action;
use crate::Book;
use crate::error::StoreError;

/// The `format` value every document carries (D-278).
const FORMAT: &str = "kassensturz-dashboard";
/// The largest document the book keeps, in bytes (D-278, D-281).
const MAX_DOCUMENT_BYTES: usize = 1 << 20;
/// The longest name, in characters (D-281).
const MAX_NAME_CHARS: usize = 200;

const COLUMNS: &str = "id, kind, name, position, format_version, document";

fn kind_to_text(kind: ViewKind) -> &'static str {
    match kind {
        ViewKind::View => "view",
        ViewKind::Dashboard => "dashboard",
    }
}

fn read_view(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredView> {
    let kind: String = row.get(1)?;
    Ok(StoredView {
        id: ViewId::from_bytes(row.get(0)?),
        // The table's CHECK allows nothing else.
        kind: if kind == "view" {
            ViewKind::View
        } else {
            ViewKind::Dashboard
        },
        name: row.get(2)?,
        position: row.get(3)?,
        format_version: row.get(4)?,
        document: row.get(5)?,
    })
}

/// What a document says about itself, which the columns copy.
struct Header {
    kind: ViewKind,
    name: String,
    version: u32,
}

fn invalid(reason: &'static str) -> StoreError {
    StoreError::InvalidViewDocument { reason }
}

/// Checks the size and the few keys the book relies on.
fn read_header(book: &Book, document: &str) -> Result<Header, StoreError> {
    if document.len() > MAX_DOCUMENT_BYTES {
        return Err(StoreError::ViewDocumentTooBig {
            max_bytes: MAX_DOCUMENT_BYTES,
        });
    }
    let valid: bool = book
        .conn
        .query_row("SELECT json_valid(?1)", [document], |row| row.get(0))?;
    if !valid {
        return Err(invalid("not-json"));
    }
    // Each value is read only if it has the type it must have, so a number
    // where text belongs is a missing value, not a failed read.
    let (top, format, version, kind, name): (
        String,
        Option<String>,
        Option<i64>,
        Option<String>,
        Option<String>,
    ) = book.conn.query_row(
        "SELECT json_type(?1),
                CASE json_type(?1, '$.format') WHEN 'text' THEN json_extract(?1, '$.format') END,
                CASE json_type(?1, '$.version') WHEN 'integer' THEN json_extract(?1, '$.version') END,
                CASE json_type(?1, '$.kind') WHEN 'text' THEN json_extract(?1, '$.kind') END,
                CASE json_type(?1, '$.name') WHEN 'text' THEN json_extract(?1, '$.name') END",
        [document],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )?;
    if top != "object" {
        return Err(invalid("not-an-object"));
    }
    if format.as_deref() != Some(FORMAT) {
        return Err(invalid("wrong-format"));
    }
    let version = version
        .and_then(|version| u32::try_from(version).ok())
        .filter(|&version| version >= 1)
        .ok_or_else(|| invalid("bad-version"))?;
    let kind = match kind.as_deref() {
        Some("view") => ViewKind::View,
        Some("dashboard") => ViewKind::Dashboard,
        _ => return Err(invalid("bad-kind")),
    };
    let name = name.ok_or_else(|| invalid("bad-name"))?;
    if name.is_empty() || name != name.trim() || name.chars().count() > MAX_NAME_CHARS {
        return Err(invalid("bad-name"));
    }
    Ok(Header {
        kind,
        name,
        version,
    })
}

impl Book {
    /// The saved views and dashboards, those of `kind` or all of them,
    /// dashboards first, each kind in the order of its list.
    pub fn views(&self, kind: Option<ViewKind>) -> Result<Vec<StoredView>, StoreError> {
        let mut statement = self.conn.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM views
             WHERE ?1 IS NULL OR kind = ?1 ORDER BY kind, position"
        ))?;
        let rows = statement
            .query_map([kind.map(kind_to_text)], read_view)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The view or dashboard `id` ([`StoreError::UnknownView`] if there is
    /// none).
    pub fn view(&self, id: ViewId) -> Result<StoredView, StoreError> {
        self.conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM views WHERE id = ?1"),
                [id.to_bytes()],
                read_view,
            )
            .optional()?
            .ok_or(StoreError::UnknownView(id))
    }

    /// Saves a new view or dashboard at the end of its list; the user action
    /// [`Action::AddView`].
    ///
    /// The document must be JSON, at most 1 MiB, an object with `format`
    /// `kassensturz-dashboard`, a whole-number `version` of 1 or more, a
    /// `kind` of `view` or `dashboard` and a `name` of 1 to 200 characters
    /// without spaces around it ([`StoreError::InvalidViewDocument`],
    /// [`StoreError::ViewDocumentTooBig`]). No other view or dashboard of
    /// that kind may have the name, ignoring capitals
    /// ([`StoreError::ViewNameTaken`]).
    pub fn add_view(&mut self, document: &str) -> Result<ViewId, StoreError> {
        self.insert_view(ViewId::generate(), document)
    }

    /// Puts back a view or dashboard with the ID it had, e.g. from a
    /// complete export (D-029): the same checks as [`Book::add_view`], at
    /// the end of its list, so documents are restored in their order.
    ///
    /// # Panics
    ///
    /// Outside [`Book::fill_new`].
    pub fn restore_view(&mut self, id: ViewId, document: &str) -> Result<(), StoreError> {
        assert!(
            self.filling,
            "only a new book being filled takes views this way"
        );
        self.insert_view(id, document).map(|_| ())
    }

    fn insert_view(&mut self, id: ViewId, document: &str) -> Result<ViewId, StoreError> {
        let header = read_header(self, document)?;
        self.as_one_action(Action::AddView, |book| {
            book.check_view_name(None, &header)?;
            let position: u32 = book.conn.query_row(
                "SELECT count(*) FROM views WHERE kind = ?1",
                [kind_to_text(header.kind)],
                |row| row.get(0),
            )?;
            book.conn.execute(
                &format!("INSERT INTO views ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"),
                params![
                    id.to_bytes(),
                    kind_to_text(header.kind),
                    header.name,
                    position,
                    header.version,
                    document
                ],
            )?;
            Ok(id)
        })
    }

    /// Replaces the document of the view or dashboard `id`; the user action
    /// [`Action::EditView`]. The checks of [`Book::add_view`] apply, and the
    /// kind can't change. The document may carry a new name, which the
    /// columns follow. A document that is the same as the saved one changes
    /// nothing and records nothing.
    pub fn edit_view(&mut self, id: ViewId, document: &str) -> Result<(), StoreError> {
        let old = self.view(id)?;
        let header = read_header(self, document)?;
        if header.kind != old.kind {
            return Err(invalid("bad-kind"));
        }
        self.as_one_action(Action::EditView, |book| {
            book.check_view_name(Some(id), &header)?;
            book.conn.execute(
                "UPDATE views SET name = ?2, format_version = ?3, document = ?4 WHERE id = ?1",
                params![id.to_bytes(), header.name, header.version, document],
            )?;
            Ok(())
        })
    }

    /// Gives the view or dashboard `id` another name, in its document too;
    /// the user action [`Action::RenameView`]. The name is kept without
    /// spaces around it and follows the rules of [`Book::add_view`]. The
    /// document is rewritten by SQLite's JSON functions, so it comes back
    /// compact, with the same content.
    pub fn rename_view(&mut self, id: ViewId, name: &str) -> Result<(), StoreError> {
        let old = self.view(id)?;
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
            return Err(invalid("bad-name"));
        }
        // A name can grow by up to six bytes a character once it is escaped.
        if old.document.len() - old.name.len() + 6 * name.len() > MAX_DOCUMENT_BYTES {
            return Err(StoreError::ViewDocumentTooBig {
                max_bytes: MAX_DOCUMENT_BYTES,
            });
        }
        let header = Header {
            kind: old.kind,
            name: name.to_owned(),
            version: old.format_version,
        };
        self.as_one_action(Action::RenameView, |book| {
            book.check_view_name(Some(id), &header)?;
            if old.name == header.name {
                return Ok(());
            }
            book.conn.execute(
                "UPDATE views SET name = ?2, document = json_set(document, '$.name', ?2)
                 WHERE id = ?1",
                params![id.to_bytes(), header.name],
            )?;
            Ok(())
        })
    }

    /// Moves the view or dashboard `id` to `position` in the list of its
    /// kind (counted from 0; a place past the end means the end); the user
    /// action [`Action::MoveView`]. The others keep their order and the
    /// positions stay numbered 0, 1, 2, ….
    pub fn move_view(&mut self, id: ViewId, position: u32) -> Result<(), StoreError> {
        let view = self.view(id)?;
        self.as_one_action(Action::MoveView, |book| {
            let mut ids = book.view_ids_of(view.kind)?;
            ids.retain(|&other| other != id);
            let at = usize::try_from(position)
                .unwrap_or(usize::MAX)
                .min(ids.len());
            ids.insert(at, id);
            book.number_views(&ids)
        })
    }

    /// Deletes the view or dashboard `id`; the user action
    /// [`Action::RemoveView`]. The ones behind it move up by one.
    pub fn remove_view(&mut self, id: ViewId) -> Result<(), StoreError> {
        let view = self.view(id)?;
        self.as_one_action(Action::RemoveView, |book| {
            book.conn
                .execute("DELETE FROM views WHERE id = ?1", [id.to_bytes()])?;
            let ids = book.view_ids_of(view.kind)?;
            book.number_views(&ids)
        })
    }

    /// Refuses a name that another document of the same kind has, ignoring
    /// capitals.
    fn check_view_name(&self, own: Option<ViewId>, header: &Header) -> Result<(), StoreError> {
        let wanted = header.name.to_lowercase();
        let mut statement = self
            .conn
            .prepare_cached("SELECT id, name FROM views WHERE kind = ?1")?;
        let rows = statement
            .query_map([kind_to_text(header.kind)], |row| {
                Ok((ViewId::from_bytes(row.get(0)?), row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if rows
            .iter()
            .any(|(id, name)| Some(*id) != own && name.to_lowercase() == wanted)
        {
            return Err(StoreError::ViewNameTaken {
                name: header.name.clone(),
            });
        }
        Ok(())
    }

    /// The IDs of one kind, in the order of their list.
    fn view_ids_of(&self, kind: ViewKind) -> Result<Vec<ViewId>, StoreError> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT id FROM views WHERE kind = ?1 ORDER BY position")?;
        let ids = statement
            .query_map([kind_to_text(kind)], |row| {
                Ok(ViewId::from_bytes(row.get(0)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// Numbers the documents 0, 1, 2, … in the given order; rows that
    /// already have their number are not touched, so they are not recorded.
    fn number_views(&self, ids: &[ViewId]) -> Result<(), StoreError> {
        for (position, id) in (0_u32..).zip(ids) {
            self.conn.execute(
                "UPDATE views SET position = ?2 WHERE id = ?1 AND position <> ?2",
                params![id.to_bytes(), position],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
