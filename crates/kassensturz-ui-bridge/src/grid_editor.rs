// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The dashboard on the Overview and its grid editor (M6b.3, D-287, D-288,
//! D-311, D-314): which layout the page shows, the rows QML draws, and how a
//! session is saved. No Qt in this file: `grid_editor_model.rs` hands the
//! results to QML.
//!
//! The page shows the dashboard stored in the book, or the built-in
//! Overview (D-314) when there is none or the stored one cannot be read.
//! The editor works on a copy in memory (`kassensturz_report::Editor`) and
//! writes to the book only on Done: `add_view` the first time, `edit_view`
//! after that, and `remove_view` when the user went back to the built-in
//! layout, each one entry in the change history (D-288).

use kassensturz_core::{ViewId, ViewKind};
use kassensturz_report::grid::{self, COLUMNS, MAX_ROWS};
use kassensturz_report::{
    Component, ComponentType, Document, Edit, Editor, Entry, Frame, Item, OVERVIEW_NAME,
    ReportError,
};
use kassensturz_store::{Book, StoreError};

/// Why the layout couldn't be read, edited or kept.
#[derive(Debug)]
pub enum LayoutError {
    /// The book refused or couldn't be read.
    Store(StoreError),
    /// An edit the grid refused, or a document that doesn't follow the
    /// schema.
    Report(ReportError),
}

impl From<StoreError> for LayoutError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<ReportError> for LayoutError {
    fn from(error: ReportError) -> Self {
        Self::Report(error)
    }
}

/// What saving a session did to the book.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved {
    /// Nothing changed, so nothing was written.
    Nothing,
    /// The dashboard was stored for the first time.
    Added,
    /// The stored dashboard was replaced.
    Edited,
    /// The user went back to the built-in layout, so the stored copy was
    /// removed.
    Removed,
}

/// The dashboard the page shows, with the editor's own undo.
pub struct Layout {
    /// The components as they are now, with the undo and redo of the session.
    pub editor: Editor,
    /// The stored dashboard this session started from, if the book has one.
    stored: Option<ViewId>,
    /// The name the stored dashboard has, kept when it is replaced.
    name: String,
    /// The stored dashboard couldn't be read, so the built-in layout is
    /// shown; saving a change replaces the stored one.
    pub damaged: bool,
}

/// The built-in Overview's components (D-314).
pub fn built_in() -> Result<Vec<Entry>, ReportError> {
    Ok(Document::default_overview()?.entries)
}

impl Layout {
    /// Reads the book's dashboard, or the built-in one. A stored dashboard
    /// that can't be read or checked is not repaired: the built-in layout is
    /// shown and [`Layout::damaged`] says so.
    pub fn open(book: &Book) -> Result<Self, LayoutError> {
        let stored = book.views(Some(ViewKind::Dashboard))?.into_iter().next();
        if let Some(stored) = stored {
            if let Ok(editor) = Document::read(&stored.document).and_then(Editor::new) {
                return Ok(Self {
                    editor,
                    stored: Some(stored.id),
                    name: stored.name,
                    damaged: false,
                });
            }
            return Ok(Self {
                editor: Editor::new(Document::default_overview()?)?,
                stored: Some(stored.id),
                name: stored.name,
                damaged: true,
            });
        }
        Ok(Self {
            editor: Editor::new(Document::default_overview()?)?,
            stored: None,
            name: OVERVIEW_NAME.to_owned(),
            damaged: false,
        })
    }

    /// Whether the components are the built-in layout's.
    pub fn is_built_in(&self) -> bool {
        built_in().is_ok_and(|entries| entries == self.editor.document().entries)
    }

    /// Writes the session to the book as one entry of its change history.
    /// Writes nothing when the session changed nothing.
    pub fn save(&self, book: &mut Book) -> Result<Saved, LayoutError> {
        if !self.editor.has_changes() {
            return Ok(Saved::Nothing);
        }
        let mut document = self.editor.document().clone();
        document.name.clone_from(&self.name);
        let text = document.to_json();
        // Read back what is about to be kept, so a document the reader
        // refuses never reaches the book.
        Document::read(&text)?;
        match (self.stored, self.is_built_in()) {
            (None, true) => Ok(Saved::Nothing),
            (None, false) => {
                book.add_view(&text)?;
                Ok(Saved::Added)
            }
            (Some(id), true) => {
                book.remove_view(id)?;
                Ok(Saved::Removed)
            }
            (Some(id), false) => {
                book.edit_view(id, &text)?;
                Ok(Saved::Edited)
            }
        }
    }
}

/// One component for the screen, in the grid.
pub struct Row {
    /// The component's ID in the document.
    pub id: String,
    /// Where it sits.
    pub frame: Frame,
    /// The smallest it can be made.
    pub least: grid::Size,
    /// The component; `None` for one of a type this version doesn't know.
    pub component: Option<Component>,
}

/// The components in reading order (D-287), which is the order of the
/// screen's Tab key, the screen reader and a narrow window.
pub fn rows(editor: &Editor) -> Vec<Row> {
    let entries = &editor.document().entries;
    grid::reading_order(entries)
        .into_iter()
        .filter_map(|index| {
            let entry = &entries[index];
            Some(Row {
                id: entry.id.clone(),
                frame: entry.frame?,
                least: grid::smallest_size(&entry.item),
                component: match &entry.item {
                    Item::Component(component) => Some(component.clone()),
                    Item::Unknown(_) => None,
                },
            })
        })
        .collect()
}

/// How many rows the grid is high: the bottom edge of the lowest component.
pub fn total_rows(editor: &Editor) -> u32 {
    editor
        .document()
        .entries
        .iter()
        .filter_map(|entry| entry.frame.map(Frame::bottom))
        .max()
        .unwrap_or(0)
}

/// The grid's size, for the screen: columns and the most rows a component
/// can be high.
pub const fn size() -> (u32, u32) {
    (COLUMNS, MAX_ROWS)
}

/// The dataset a new component of this type starts with: the first one it
/// can show, none for a heading.
pub fn default_dataset(kind: ComponentType) -> &'static str {
    kind.datasets().next().map_or("", |dataset| dataset.id)
}

/// What the library offers: the type's id and the dataset a new one shows.
pub fn library() -> Vec<(&'static str, &'static str)> {
    ComponentType::LIBRARY
        .into_iter()
        .map(|kind| (kind.id(), default_dataset(kind)))
        .collect()
}

/// Adds a component of the type `kind_id` with its defaults; gives its ID.
pub fn add(editor: &mut Editor, kind_id: &str) -> Result<String, LayoutError> {
    let kind = ComponentType::from_id(kind_id).ok_or(ReportError::DocumentInvalid {
        reason: "unknown-type",
        path: "type".to_owned(),
    })?;
    let component = Component::new(kind, default_dataset(kind))?;
    Ok(editor.add(component)?)
}

/// Replaces a component with one the properties panel made, as JSON text.
/// The schema checks it; a frame that is too small for its new type grows.
pub fn set_component(editor: &mut Editor, id: &str, json: &str) -> Result<bool, LayoutError> {
    let new = Component::from_json(json)?;
    Ok(editor.change(id, |component| {
        *component = new;
        Ok(())
    })?)
}

/// The word the screens know an edit by, for "Undo …".
pub const fn edit_word(edit: Option<Edit>) -> &'static str {
    match edit {
        Some(edit) => edit.id(),
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> Book {
        Book::create_in_memory().unwrap()
    }

    #[test]
    fn a_book_without_a_dashboard_shows_the_built_in_layout() {
        let layout = Layout::open(&book()).unwrap();
        assert!(layout.is_built_in());
        assert!(!layout.damaged);
        assert!(!layout.editor.has_changes());
        let shown = rows(&layout.editor);
        assert_eq!(shown.len(), 7);
        // Reading order: the four cards, then the donut and the table, then
        // the line.
        let ids: Vec<&str> = shown.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "income", "expenses", "left", "kept", "spending", "biggest", "trend"
            ]
        );
        assert_eq!(total_rows(&layout.editor), 11);
    }

    #[test]
    fn nothing_is_written_until_the_layout_changes() {
        let mut book = book();
        let layout = Layout::open(&book).unwrap();
        assert_eq!(layout.save(&mut book).unwrap(), Saved::Nothing);
        assert!(book.views(Some(ViewKind::Dashboard)).unwrap().is_empty());
    }

    #[test]
    fn a_changed_layout_is_added_then_replaced_then_removed_by_a_reset() {
        let mut book = book();
        let mut layout = Layout::open(&book).unwrap();
        assert!(layout.editor.nudge("trend", 0, 2).unwrap());
        assert_eq!(layout.save(&mut book).unwrap(), Saved::Added);
        let stored = book.views(Some(ViewKind::Dashboard)).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].name, OVERVIEW_NAME);

        // The page now shows the stored copy, and a later session replaces it.
        let mut layout = Layout::open(&book).unwrap();
        assert!(!layout.is_built_in());
        assert!(!layout.editor.has_changes());
        assert!(layout.editor.nudge("trend", 0, 1).unwrap());
        assert_eq!(layout.save(&mut book).unwrap(), Saved::Edited);
        assert_eq!(book.views(Some(ViewKind::Dashboard)).unwrap().len(), 1);

        // Back to the built-in layout removes the stored copy.
        let mut layout = Layout::open(&book).unwrap();
        assert!(layout.editor.reset(&built_in().unwrap()).unwrap());
        assert_eq!(layout.save(&mut book).unwrap(), Saved::Removed);
        assert!(book.views(Some(ViewKind::Dashboard)).unwrap().is_empty());
    }

    #[test]
    fn a_session_is_one_entry_the_books_undo_takes_back() {
        let mut book = book();
        let before = book.history().unwrap().len();
        let mut layout = Layout::open(&book).unwrap();
        layout.editor.nudge("trend", 0, 2).unwrap();
        layout.editor.nudge("trend", 0, 1).unwrap();
        layout.editor.duplicate("income").unwrap();
        layout.save(&mut book).unwrap();
        assert_eq!(book.history().unwrap().len(), before + 1);
        book.undo().unwrap();
        assert!(book.views(Some(ViewKind::Dashboard)).unwrap().is_empty());
        assert!(Layout::open(&book).unwrap().is_built_in());
    }

    #[test]
    fn a_stored_dashboard_that_cannot_be_read_is_not_repaired() {
        let mut book = book();
        // Frames that overlap: a document the reader refuses.
        let text = DEFAULT_WITH_OVERLAP;
        assert!(Document::read(text).is_err());
        book.add_view(text).unwrap();
        let mut layout = Layout::open(&book).unwrap();
        assert!(layout.damaged);
        assert!(layout.is_built_in());
        // Changing the built-in layout replaces the damaged copy.
        layout.editor.nudge("trend", 0, 1).unwrap();
        assert_eq!(layout.save(&mut book).unwrap(), Saved::Edited);
        assert!(!Layout::open(&book).unwrap().damaged);
    }

    const DEFAULT_WITH_OVERLAP: &str = r#"{
      "format": "kassensturz-dashboard", "version": 1, "kind": "dashboard", "name": "Overview",
      "components": [
        {"id": "a", "type": "kpi.card", "dataset": "income_expense_by_month",
         "params": {"measure": "income", "compare": "previous_period"},
         "frame": {"col": 0, "row": 0, "cols": 3, "rows": 2}},
        {"id": "b", "type": "kpi.card", "dataset": "income_expense_by_month",
         "params": {"measure": "income", "compare": "previous_period"},
         "frame": {"col": 1, "row": 0, "cols": 3, "rows": 2}}
      ]
    }"#;

    #[test]
    fn the_library_offers_the_ten_components_and_adds_each_with_its_defaults() {
        let offered = library();
        assert_eq!(offered.len(), 10);
        assert!(offered.contains(&("text.heading", "")));
        let mut layout = Layout::open(&book()).unwrap();
        for (kind, _) in offered {
            let id = add(&mut layout.editor, kind).unwrap();
            let row = rows(&layout.editor)
                .into_iter()
                .find(|row| row.id == id)
                .unwrap();
            assert_eq!(row.component.unwrap().kind().id(), kind);
        }
        assert_eq!(rows(&layout.editor).len(), 17);
        assert!(add(&mut layout.editor, "chart.radar").is_err());
    }

    #[test]
    fn the_panels_component_replaces_the_one_in_the_grid() {
        let mut layout = Layout::open(&book()).unwrap();
        let id = add(&mut layout.editor, "text.heading").unwrap();
        let mut heading = Component::new(ComponentType::Heading, "").unwrap();
        heading
            .set("text", kassensturz_report::Setting::Text("Spending".into()))
            .unwrap();
        assert!(set_component(&mut layout.editor, &id, &heading.to_value().to_string()).unwrap());
        let row = rows(&layout.editor)
            .into_iter()
            .find(|row| row.id == id)
            .unwrap();
        assert_eq!(row.component, Some(heading));
        // The same thing again changes nothing and keeps no step.
        assert!(
            !set_component(
                &mut layout.editor,
                &id,
                &row.component.unwrap().to_value().to_string()
            )
            .unwrap()
        );
        // A component the schema refuses changes nothing.
        assert!(set_component(&mut layout.editor, &id, "{\"type\": \"chart.radar\"}").is_err());
    }
}
