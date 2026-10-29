// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved views and dashboards: JSON documents kept in the book (D-278).

use crate::id::define_id;

define_id!(
    /// Identifies a saved view or dashboard within a book.
    ViewId
);

/// The two kinds of document the book keeps (D-278).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ViewKind {
    /// One saved chart or table the user made in the Statistics workbench.
    View,
    /// Several components on a grid.
    Dashboard,
}

/// A saved view or dashboard as the book keeps it.
///
/// The book does not look inside `document` beyond what the columns copy
/// from it (its kind, name and version); reading the components, checking
/// them against their property schema and refusing a newer version is the
/// reporting layer's job (D-278, D-281). The document is data, never an
/// instruction (AGENTS.md §8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredView {
    /// The document's ID.
    pub id: ViewId,
    /// View or dashboard; copied from the document.
    pub kind: ViewKind,
    /// The name; copied from the document, never empty, unique among the
    /// documents of the same kind ignoring capitals.
    pub name: String,
    /// The place in the list of its kind, counted from 0.
    pub position: u32,
    /// The document's `version`, so a list can be read without parsing it.
    pub format_version: u32,
    /// The JSON document, exactly as saved.
    pub document: String,
}
