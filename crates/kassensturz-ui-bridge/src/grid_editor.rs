// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The dashboard on the Overview and its grid editor (M6b.3, D-287, D-288,
//! D-311, D-314): which layout the page shows, the rows QML draws, and how a
//! session is saved. No Qt in this file: `grid_editor_model.rs` hands the
//! results to QML.
//!
//! The page shows one of the book's dashboards (M6d.1, D-436; the list and
//! its actions are in `dashboards.rs`), or the built-in Overview (D-314)
//! when there is none or the stored one cannot be read. The editor works on a copy in memory (`kassensturz_report::Editor`) and
//! writes to the book only on Done: `add_view` the first time, `edit_view`
//! after that, and `remove_view` when the user went back to the built-in
//! layout, each one entry in the change history (D-288).

use kassensturz_core::{ViewId, ViewKind};
use kassensturz_report::grid::{self, COLUMNS, MAX_ROWS};
use kassensturz_report::{
    Asked, Component, ComponentType, Document, Edit, Editor, Entry, Filters, Frame, Item,
    ReportError, Setting, templates,
};
use kassensturz_store::{Book, StoreError};

use crate::dashboards;

/// Why the layout couldn't be read, edited or kept.
#[derive(Debug)]
pub enum LayoutError {
    /// The book refused or couldn't be read.
    Store(StoreError),
    /// An edit the grid refused, or a document that doesn't follow the
    /// schema.
    Report(ReportError),
    /// No dashboard has the ID.
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
    /// The stored dashboard this session started from; `None` for the
    /// built-in Overview while the book has no dashboard.
    pub stored: Option<ViewId>,
    /// The name the dashboard has, kept when it is replaced.
    pub name: String,
    /// The stored dashboard couldn't be read, so the built-in layout is
    /// shown; saving a change replaces the stored one.
    pub damaged: bool,
    /// The built-in Overview's name in the user's language, which a stored
    /// copy of it may carry.
    built_in_name: String,
}

/// The built-in Overview's components (D-314).
pub fn built_in() -> Result<Vec<Entry>, ReportError> {
    Ok(Document::default_overview()?.entries)
}

impl Layout {
    /// Reads the dashboard `wanted` from the book, or the one the book
    /// opens at (D-448) when `wanted` is `None` or no longer there, or the
    /// built-in Overview when the book has none. A stored dashboard that
    /// can't be read or checked is not repaired: the built-in layout is
    /// shown and [`Layout::damaged`] says so. `built_in_name` is the
    /// built-in Overview's name in the user's language.
    pub fn open(
        book: &Book,
        wanted: Option<ViewId>,
        built_in_name: &str,
    ) -> Result<Self, LayoutError> {
        let list = dashboards::list(book, built_in_name)?;
        let chosen = list
            .iter()
            .find(|dashboard| wanted.is_some() && dashboard.id == wanted)
            .or_else(|| list.iter().find(|dashboard| dashboard.is_default))
            .and_then(|dashboard| dashboard.id);
        let Some(id) = chosen else {
            let mut document = templates::document(templates::OVERVIEW)?;
            built_in_name.clone_into(&mut document.name);
            return Ok(Self {
                editor: Editor::new(document)?,
                stored: None,
                name: built_in_name.to_owned(),
                damaged: false,
                built_in_name: built_in_name.to_owned(),
            });
        };
        let stored = book.view(id)?;
        let (editor, damaged) = match Document::read(&stored.document).and_then(Editor::new) {
            Ok(editor) => (editor, false),
            Err(_) => (Editor::new(Document::default_overview()?)?, true),
        };
        Ok(Self {
            editor,
            stored: Some(id),
            name: stored.name,
            damaged,
            built_in_name: built_in_name.to_owned(),
        })
    }

    /// Whether the components are the built-in layout's.
    pub fn is_built_in(&self) -> bool {
        // A layout that carries filters is no longer the built-in one: it
        // has to be kept to keep them.
        self.editor.filters() == Filters::default()
            && built_in().is_ok_and(|entries| entries == self.editor.document().entries)
    }

    /// Whether Done may remove the stored copy because it is the built-in
    /// Overview again (D-315, kept for a list of one, D-436); the rule is
    /// [`dashboards::is_plain_overview`].
    fn is_plain_overview(&self) -> bool {
        dashboards::is_plain_overview(self.editor.document(), &self.name, &self.built_in_name)
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
        let alone = book.views(Some(ViewKind::Dashboard))?.len() <= 1;
        match self.stored {
            None if self.is_built_in() => Ok(Saved::Nothing),
            None => {
                book.add_view(&text)?;
                Ok(Saved::Added)
            }
            // With two or more dashboards nothing is removed: their order
            // must be kept (D-436).
            Some(id) if alone && self.is_plain_overview() => {
                book.remove_view(id)?;
                Ok(Saved::Removed)
            }
            Some(id) => {
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
    use kassensturz_report::OVERVIEW_NAME;

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
        let layout = Layout::open(&book(), None, OVERVIEW_NAME).unwrap();
        assert!(layout.is_built_in());
        assert!(!layout.damaged);
        assert!(!layout.editor.has_changes());
        let shown = rows(&layout.editor, &Filters::default());
        assert_eq!(shown.len(), 12);
        // Reading order: the bank warnings, the four cards, the budget bars,
        // then the donut and the table, the line, and the note about
        // transfers.
        let ids: Vec<&str> = shown.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(
            ids,
            [
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
        assert_eq!(total_rows(&layout.editor), 23);
    }

    #[test]
    fn nothing_is_written_until_the_layout_changes() {
        let mut book = book();
        let layout = Layout::open(&book, None, OVERVIEW_NAME).unwrap();
        assert_eq!(layout.save(&mut book).unwrap(), Saved::Nothing);
        assert!(book.views(Some(ViewKind::Dashboard)).unwrap().is_empty());
    }

    #[test]
    fn a_changed_layout_is_added_then_replaced_then_removed_by_a_reset() {
        let mut book = book();
        let mut layout = Layout::open(&book, None, OVERVIEW_NAME).unwrap();
        assert!(layout.editor.nudge("trend", 0, 2).unwrap());
        assert_eq!(layout.save(&mut book).unwrap(), Saved::Added);
        let stored = book.views(Some(ViewKind::Dashboard)).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].name, OVERVIEW_NAME);

        // The page now shows the stored copy, and a later session replaces it.
        let mut layout = Layout::open(&book, None, OVERVIEW_NAME).unwrap();
        assert!(!layout.is_built_in());
        assert!(!layout.editor.has_changes());
        assert!(layout.editor.nudge("trend", 0, 1).unwrap());
        assert_eq!(layout.save(&mut book).unwrap(), Saved::Edited);
        assert_eq!(book.views(Some(ViewKind::Dashboard)).unwrap().len(), 1);

        // Back to the built-in layout removes the stored copy.
        let mut layout = Layout::open(&book, None, OVERVIEW_NAME).unwrap();
        assert!(layout.editor.reset(&built_in().unwrap()).unwrap());
        assert_eq!(layout.save(&mut book).unwrap(), Saved::Removed);
        assert!(book.views(Some(ViewKind::Dashboard)).unwrap().is_empty());
    }

    #[test]
    fn the_filters_a_layout_carries_are_saved_with_it_in_one_entry_and_read_back() {
        let mut book = book();
        let mut layout = Layout::open(&book, None, OVERVIEW_NAME).unwrap();
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
        let reopened = Layout::open(&book, None, OVERVIEW_NAME).unwrap();
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
    fn a_session_is_one_entry_the_books_undo_takes_back() {
        let mut book = book();
        let before = book.history().unwrap().len();
        let mut layout = Layout::open(&book, None, OVERVIEW_NAME).unwrap();
        layout.editor.nudge("trend", 0, 2).unwrap();
        layout.editor.nudge("trend", 0, 1).unwrap();
        layout.editor.duplicate("income").unwrap();
        layout.save(&mut book).unwrap();
        assert_eq!(book.history().unwrap().len(), before + 1);
        book.undo().unwrap();
        assert!(book.views(Some(ViewKind::Dashboard)).unwrap().is_empty());
        assert!(
            Layout::open(&book, None, OVERVIEW_NAME)
                .unwrap()
                .is_built_in()
        );
    }

    #[test]
    fn a_stored_dashboard_that_cannot_be_read_is_not_repaired() {
        let mut book = book();
        // Frames that overlap: a document the reader refuses.
        let text = DEFAULT_WITH_OVERLAP;
        assert!(Document::read(text).is_err());
        book.add_view(text).unwrap();
        let mut layout = Layout::open(&book, None, OVERVIEW_NAME).unwrap();
        assert!(layout.damaged);
        assert!(layout.is_built_in());
        // Changing the built-in layout replaces the damaged copy.
        layout.editor.nudge("trend", 0, 1).unwrap();
        assert_eq!(layout.save(&mut book).unwrap(), Saved::Edited);
        assert!(!Layout::open(&book, None, OVERVIEW_NAME).unwrap().damaged);
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
    fn the_library_offers_the_eighteen_components_and_adds_each_with_its_defaults() {
        let offered = library();
        assert_eq!(offered.len(), 18);
        assert!(offered.contains(&("text.heading", "")));
        let mut layout = Layout::open(&book(), None, OVERVIEW_NAME).unwrap();
        for (kind, _) in offered {
            let id = add(&mut layout.editor, kind).unwrap();
            let row = rows(&layout.editor, &Filters::default())
                .into_iter()
                .find(|row| row.id == id)
                .unwrap();
            assert_eq!(row.component.unwrap().kind().id(), kind);
        }
        assert_eq!(rows(&layout.editor, &Filters::default()).len(), 30);
        assert!(add(&mut layout.editor, "chart.radar").is_err());
    }

    #[test]
    fn a_component_is_added_with_the_dataset_asked_for_when_its_type_can_show_it() {
        let mut layout = Layout::open(&book(), None, OVERVIEW_NAME).unwrap();
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

        let mut layout = Layout::open(&book, None, OVERVIEW_NAME).unwrap();
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
        let mut layout = Layout::open(&book(), None, OVERVIEW_NAME).unwrap();
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
