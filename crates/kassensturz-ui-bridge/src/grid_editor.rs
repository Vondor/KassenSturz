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

use kassensturz_core::{StoredView, ViewId, ViewKind};
use kassensturz_report::grid::{self, COLUMNS, MAX_ROWS};
use kassensturz_report::{
    Asked, Component, ComponentType, Document, Edit, Editor, Entry, Filters, Frame, Item,
    OVERVIEW_NAME, ReportError, Setting,
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
    /// No saved layout has the ID.
    NoSuchLayout,
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

/// Whether a dashboard's name is the one the Overview shows (D-330); the
/// others in the book are saved layouts. Names are compared the way the
/// book compares them, ignoring case.
fn is_active_name(name: &str) -> bool {
    name.to_lowercase() == OVERVIEW_NAME.to_lowercase()
}

/// A layout the user saved under a name of their own (D-330).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedLayout {
    /// The book's ID of the stored dashboard, as text.
    pub id: String,
    /// The name the user gave it.
    pub name: String,
}

/// The saved layouts, in the order of the book's list; not the one the
/// Overview shows.
pub fn saved_layouts(book: &Book) -> Result<Vec<SavedLayout>, LayoutError> {
    Ok(book
        .views(Some(ViewKind::Dashboard))?
        .into_iter()
        .filter(|view| !is_active_name(&view.name))
        .map(|view| SavedLayout {
            id: view.id.to_string(),
            name: view.name,
        })
        .collect())
}

/// Keeps the editor's components in the book under `name`, as a layout the
/// user can load later. This is its own entry in the change history and does
/// not end the session. A name that is taken, or that the Overview itself
/// has, is refused (`store.view-name-taken`).
pub fn save_as(book: &mut Book, editor: &Editor, name: &str) -> Result<String, LayoutError> {
    let name = name.trim();
    if is_active_name(name) {
        return Err(StoreError::ViewNameTaken {
            name: name.to_owned(),
        }
        .into());
    }
    let mut document = editor.document().clone();
    name.clone_into(&mut document.name);
    let text = document.to_json();
    Document::read(&text)?;
    Ok(book.add_view(&text)?.to_string())
}

/// Puts a saved layout's components into the editor as one edit. Nothing is
/// written until Done.
pub fn load(book: &Book, editor: &mut Editor, id: &str) -> Result<bool, LayoutError> {
    let stored = saved_layout(book, id)?;
    let document = Document::read(&stored.document)?;
    // A saved layout carries its filters with it (D-351).
    let filters = editor.set_filters(&document.filters())?;
    Ok(editor.load(&document.entries)? || filters)
}

/// Deletes a saved layout from the book. The Overview's own layout is not a
/// saved layout and can't be deleted this way.
pub fn delete_saved(book: &mut Book, id: &str) -> Result<(), LayoutError> {
    let stored = saved_layout(book, id)?;
    Ok(book.remove_view(stored.id)?)
}

fn saved_layout(book: &Book, id: &str) -> Result<StoredView, LayoutError> {
    book.views(Some(ViewKind::Dashboard))?
        .into_iter()
        .find(|view| view.id.to_string() == id && !is_active_name(&view.name))
        .ok_or(LayoutError::NoSuchLayout)
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
        let stored = book
            .views(Some(ViewKind::Dashboard))?
            .into_iter()
            .find(|view| is_active_name(&view.name));
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
        // A layout that carries filters is no longer the built-in one: it
        // has to be kept to keep them.
        self.editor.filters() == Filters::default()
            && built_in().is_ok_and(|entries| entries == self.editor.document().entries)
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
    /// What it asks for with the filter bar's choices (M6c.7); `None` for a
    /// component that shows no figures.
    pub asked: Option<Asked>,
}

/// The components in reading order (D-287), which is the order of the
/// screen's Tab key, the screen reader and a narrow window. `bar` is what
/// the filter bar holds now (D-350).
pub fn rows(editor: &Editor, bar: &Filters) -> Vec<Row> {
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
                asked: editor.document().asked(&entry.id, bar),
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

/// Sets the filters the layout carries (D-351): kept with the layout when
/// the session is saved, not an edit on its stack. Gives whether they
/// changed.
pub fn set_filters(editor: &mut Editor, filters: &Filters) -> Result<bool, LayoutError> {
    Ok(editor.set_filters(filters)?)
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

/// The debug-log detail for an added widget of the type `kind_id`: the id
/// with its dot written as an underscore, if it is in the library.
pub fn widget_detail(kind_id: &str) -> Option<&'static str> {
    let wanted = kind_id.replace('.', "_");
    kassensturz_core::vocabulary::DETAILS
        .iter()
        .copied()
        .find(|detail| *detail == wanted)
}

/// Adds a component of the type `kind_id` with its defaults; gives its ID.
pub fn add(editor: &mut Editor, kind_id: &str) -> Result<String, LayoutError> {
    add_with(editor, kind_id, "")
}

/// Adds a component of the type `kind_id` that shows the dataset
/// `dataset_id`, which the schema checks the type can show; an empty one
/// means the type's first (M6c.2). Gives its ID.
pub fn add_with(
    editor: &mut Editor,
    kind_id: &str,
    dataset_id: &str,
) -> Result<String, LayoutError> {
    let kind = ComponentType::from_id(kind_id).ok_or(ReportError::DocumentInvalid {
        reason: "unknown-type",
        path: "type".to_owned(),
    })?;
    let dataset = if dataset_id.is_empty() {
        default_dataset(kind)
    } else {
        dataset_id
    };
    let component = Component::new(kind, dataset)?;
    Ok(editor.add(component)?)
}

/// Adds a copy of a saved view's component (`Opened::component`, JSON text)
/// with the period the view was saved with, so it shows what the Statistics
/// page showed; gives its ID. Later changes to the saved view don't reach
/// the copy.
pub fn add_view(editor: &mut Editor, json: &str, period: &str) -> Result<String, LayoutError> {
    let mut component = Component::from_json(json)?;
    if !period.is_empty() {
        component.set("period", Setting::Word(period.to_owned()))?;
    }
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
    fn every_library_kind_has_a_debug_log_detail() {
        for (kind, _) in library() {
            let detail = widget_detail(kind).unwrap_or_else(|| panic!("no detail for {kind}"));
            assert_eq!(detail, kind.replace('.', "_"));
        }
        assert_eq!(widget_detail("nonsense"), None);
    }

    #[test]
    fn a_book_without_a_dashboard_shows_the_built_in_layout() {
        let layout = Layout::open(&book()).unwrap();
        assert!(layout.is_built_in());
        assert!(!layout.damaged);
        assert!(!layout.editor.has_changes());
        let shown = rows(&layout.editor, &Filters::default());
        assert_eq!(shown.len(), 14);
        // Reading order: the period header and the filter bar, the bank warnings, the four cards, the budget bars,
        // then the donut and the table, the line, and the note about
        // transfers.
        let ids: Vec<&str> = shown.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "period-header",
                "filter-bar",
                "fetch-gaps",
                "worth",
                "income",
                "expenses",
                "left",
                "kept",
                "budgets",
                "spending",
                "biggest",
                "trend",
                "sources",
                "transfers"
            ]
        );
        assert_eq!(total_rows(&layout.editor), 25);
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
    fn the_filters_a_layout_carries_are_saved_with_it_in_one_entry_and_read_back() {
        let mut book = book();
        let mut layout = Layout::open(&book).unwrap();
        assert!(layout.is_built_in());
        let chosen = Filters {
            period: Some("this-year".into()),
            accounts: vec!["a-1".into()],
            categories: vec!["c-1".into()],
        };
        assert!(set_filters(&mut layout.editor, &chosen).unwrap());
        // The built-in layout with filters is no longer the built-in one,
        // so it is kept (D-351).
        assert!(!layout.is_built_in());
        assert_eq!(layout.save(&mut book).unwrap(), Saved::Added);
        let reopened = Layout::open(&book).unwrap();
        assert_eq!(reopened.editor.filters(), chosen);
        assert!(!reopened.editor.has_changes());
        // The component asks for the saved filters of its layout.
        let rows = rows(&reopened.editor, &reopened.editor.filters());
        let income = rows.iter().find(|row| row.id == "income").unwrap();
        assert_eq!(income.asked.as_ref().unwrap().period, "this-year");
        // Taking them out again sends the layout back to being the built-in
        // one, which removes the stored copy.
        let mut layout = reopened;
        assert!(set_filters(&mut layout.editor, &Filters::default()).unwrap());
        assert!(layout.is_built_in());
        assert_eq!(layout.save(&mut book).unwrap(), Saved::Removed);
    }

    #[test]
    fn a_saved_layout_brings_its_filters_when_it_is_loaded() {
        let mut book = book();
        let mut layout = Layout::open(&book).unwrap();
        let chosen = Filters {
            categories: vec!["c-1".into()],
            ..Filters::default()
        };
        set_filters(&mut layout.editor, &chosen).unwrap();
        let id = save_as(&mut book, &layout.editor, "With filters").unwrap();
        let mut other = Layout::open(&book).unwrap();
        other.editor = Editor::new(Document::default_overview().unwrap()).unwrap();
        assert!(load(&book, &mut other.editor, &id).unwrap());
        assert_eq!(other.editor.filters(), chosen);
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
    fn a_saved_layout_is_kept_listed_loaded_and_deleted() {
        let mut book = book();
        let mut layout = Layout::open(&book).unwrap();
        layout.editor.nudge("trend", 0, 2).unwrap();
        let arranged = layout.editor.document().entries.clone();
        let id = save_as(&mut book, &layout.editor, "  Month review ").unwrap();

        // A saved layout is not the one the Overview shows.
        let saved = saved_layouts(&book).unwrap();
        assert_eq!(
            saved,
            [SavedLayout {
                id: id.clone(),
                name: "Month review".to_owned()
            }]
        );
        assert!(Layout::open(&book).unwrap().is_built_in());

        // Loading is one edit in the session that Undo takes back, and
        // writes nothing until Done.
        let mut layout = Layout::open(&book).unwrap();
        let before = layout.editor.document().entries.clone();
        assert!(load(&book, &mut layout.editor, &id).unwrap());
        assert_eq!(layout.editor.document().entries, arranged);
        assert_eq!(layout.editor.next_undo(), Some(Edit::Load));
        assert_eq!(edit_word(layout.editor.next_undo()), "load");
        assert_eq!(layout.editor.undo(), Some(Edit::Load));
        assert_eq!(layout.editor.document().entries, before);
        layout.editor.redo();
        assert_eq!(layout.save(&mut book).unwrap(), Saved::Added);
        let shown = Layout::open(&book).unwrap();
        assert_eq!(shown.editor.document().entries, arranged);
        assert_eq!(saved_layouts(&book).unwrap().len(), 1);

        delete_saved(&mut book, &id).unwrap();
        assert!(saved_layouts(&book).unwrap().is_empty());
        assert!(matches!(
            load(&book, &mut Layout::open(&book).unwrap().editor, &id),
            Err(LayoutError::NoSuchLayout)
        ));
    }

    #[test]
    fn a_layout_name_that_is_taken_or_the_overviews_is_refused() {
        let mut book = book();
        let layout = Layout::open(&book).unwrap();
        save_as(&mut book, &layout.editor, "Month review").unwrap();
        for name in ["month REVIEW", "overview", " Overview "] {
            assert!(matches!(
                save_as(&mut book, &layout.editor, name),
                Err(LayoutError::Store(StoreError::ViewNameTaken { .. }))
            ));
        }
        assert!(save_as(&mut book, &layout.editor, "   ").is_err());
        assert_eq!(saved_layouts(&book).unwrap().len(), 1);
    }

    #[test]
    fn the_overviews_own_layout_is_not_a_saved_layout() {
        let mut book = book();
        let mut layout = Layout::open(&book).unwrap();
        layout.editor.nudge("trend", 0, 2).unwrap();
        layout.save(&mut book).unwrap();
        let active = book.views(Some(ViewKind::Dashboard)).unwrap()[0]
            .id
            .to_string();
        assert!(saved_layouts(&book).unwrap().is_empty());
        assert!(matches!(
            delete_saved(&mut book, &active),
            Err(LayoutError::NoSuchLayout)
        ));
        // Saved layouts next to it don't change which one the page shows.
        save_as(&mut book, &layout.editor, "Other").unwrap();
        let shown = Layout::open(&book).unwrap();
        assert_eq!(shown.name, OVERVIEW_NAME);
        assert_eq!(
            shown.editor.document().entries,
            layout.editor.document().entries
        );
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
    fn the_library_offers_the_nineteen_components_and_adds_each_with_its_defaults() {
        let offered = library();
        assert_eq!(offered.len(), 19);
        assert!(offered.contains(&("text.heading", "")));
        assert!(offered.contains(&("page.period", "")));
        assert!(offered.contains(&("page.filters", "")));
        let mut layout = Layout::open(&book()).unwrap();
        for (kind, _) in offered {
            // The built-in Overview has the period header and the filter
            // bar already, and a page holds one of each (D-474).
            if matches!(kind, "page.period" | "page.filters") {
                assert!(add(&mut layout.editor, kind).is_err());
                continue;
            }
            let id = add(&mut layout.editor, kind).unwrap();
            let row = rows(&layout.editor, &Filters::default())
                .into_iter()
                .find(|row| row.id == id)
                .unwrap();
            assert_eq!(row.component.unwrap().kind().id(), kind);
        }
        assert_eq!(rows(&layout.editor, &Filters::default()).len(), 31);
        assert!(add(&mut layout.editor, "chart.radar").is_err());
    }

    #[test]
    fn a_component_is_added_with_the_dataset_asked_for_when_its_type_can_show_it() {
        let mut layout = Layout::open(&book()).unwrap();
        let shown = |layout: &Layout, id: &str| {
            let row = rows(&layout.editor, &Filters::default())
                .into_iter()
                .find(|row| row.id == id)
                .unwrap();
            let component = row.component.unwrap();
            (component.kind().id(), component.dataset().id)
        };
        for (kind, dataset) in [
            ("chart.line", "net_worth_over_time"),
            ("chart.area", "net_worth_over_time"),
            ("chart.bar", "cash_flow_by_month"),
            ("table.rows", "cash_flow_by_month"),
            ("table.rows", "account_balances"),
            ("kpi.card", "account_balances"),
        ] {
            let id = add_with(&mut layout.editor, kind, dataset).unwrap();
            assert_eq!(shown(&layout, &id), (kind, dataset));
        }
        // No dataset asked for is the type's first, as `add` always did.
        let id = add_with(&mut layout.editor, "kpi.card", "").unwrap();
        assert_eq!(shown(&layout, &id), ("kpi.card", "income_expense_by_month"));
        // A dataset the type cannot show, or does not exist, is refused, and
        // so is a progress bar of the balances (D-370).
        assert!(add_with(&mut layout.editor, "chart.pie", "net_worth_over_time").is_err());
        assert!(add_with(&mut layout.editor, "progress.bar", "account_balances").is_err());
        assert!(add_with(&mut layout.editor, "chart.bar", "no_such_dataset").is_err());
        assert!(add_with(&mut layout.editor, "chart.radar", "").is_err());
    }

    #[test]
    fn a_saved_view_is_added_as_a_copy_with_its_period() {
        use crate::saved_views;

        let mut book = book();
        let mut pie = Component::new(ComponentType::ChartPie, "spending_by_category").unwrap();
        pie.set("title", Setting::Text("Where it went".into()))
            .unwrap();
        let id = saved_views::save(
            &mut book,
            "Spending",
            "this-year",
            &pie.to_value().to_string(),
        )
        .unwrap();
        let opened = saved_views::open(&book, &id).unwrap();

        let mut layout = Layout::open(&book).unwrap();
        let before = rows(&layout.editor, &Filters::default()).len();
        let added = add_view(&mut layout.editor, &opened.component, &opened.period).unwrap();
        let all = rows(&layout.editor, &Filters::default());
        assert_eq!(all.len(), before + 1);
        let value = all
            .into_iter()
            .find(|row| row.id == added)
            .unwrap()
            .component
            .unwrap()
            .to_value();
        assert_eq!(value["type"], "chart.pie");
        assert_eq!(value["props"]["period"], "this-year");
        assert_eq!(value["props"]["title"], "Where it went");

        // A view saved without a period follows the page.
        add_view(&mut layout.editor, &opened.component, "").unwrap();
        assert!(add_view(&mut layout.editor, "{", "").is_err());
        assert!(add_view(&mut layout.editor, &opened.component, "next-week").is_err());
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
        let row = rows(&layout.editor, &Filters::default())
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
