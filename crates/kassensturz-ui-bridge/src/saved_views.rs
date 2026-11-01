// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The views the user saved in the Statistics workbench (M6a.7, D-278,
//! D-279): a list, and save, open, change, rename and delete. No Qt in this
//! file: `saved_views_model.rs` hands the results to QML.
//!
//! Every change goes through the book's change history, so it is undone with
//! the menu's Undo like any other. A document is read with
//! `kassensturz_report::Document` before anything of it is used (D-281), so a
//! saved view that was damaged or made by a newer KassenSturz is listed as
//! one that can't be opened, and can still be renamed or deleted.

use kassensturz_core::{StoredView, ViewKind};
use kassensturz_report::{Component, Document, Item, ReportError};
use kassensturz_store::{Book, StoreError};

/// Why a saved view couldn't be kept or opened.
#[derive(Debug)]
pub enum ViewsError {
    /// The book refused or couldn't be read.
    Store(StoreError),
    /// The component or the document doesn't follow the schema.
    Report(ReportError),
    /// There is no saved view with this ID; the list on screen is out of date.
    NoSuchView,
}

impl From<StoreError> for ViewsError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<ReportError> for ViewsError {
    fn from(error: ReportError) -> Self {
        Self::Report(error)
    }
}

/// One line of the list of saved views.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The view's ID, as text.
    pub id: String,
    /// The name the user gave it.
    pub name: String,
    /// Whether it can be opened by this version.
    pub usable: bool,
    /// The component's type id (`chart.pie`, ...); empty if not usable.
    pub kind: String,
    /// The component's dataset id; empty if not usable.
    pub dataset: String,
}

/// A saved view opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opened {
    /// The view's ID, as text.
    pub id: String,
    /// Its name.
    pub name: String,
    /// One of the fixed period names, or empty if the view has none.
    pub period: String,
    /// The component as JSON text, which `ComponentProperties.load` takes.
    pub component: String,
}

fn find(book: &Book, id: &str) -> Result<StoredView, ViewsError> {
    book.views(Some(ViewKind::View))?
        .into_iter()
        .find(|view| view.id.to_string() == id)
        .ok_or(ViewsError::NoSuchView)
}

fn component_of(stored: &StoredView) -> Result<(Document, Component), ViewsError> {
    let document = Document::read(&stored.document)?;
    let component = document.entries.iter().find_map(|entry| match &entry.item {
        Item::Component(component) => Some(component.clone()),
        Item::Unknown(_) => None,
    });
    match component {
        Some(component) => Ok((document, component)),
        None => Err(ReportError::DocumentInvalid {
            reason: "unknown-type",
            path: "components.0.type".to_owned(),
        }
        .into()),
    }
}

/// The saved views, in the order of the book's list.
pub fn list(book: &Book) -> Result<Vec<Row>, StoreError> {
    Ok(book
        .views(Some(ViewKind::View))?
        .into_iter()
        .map(|stored| {
            let opened = component_of(&stored).ok();
            Row {
                id: stored.id.to_string(),
                name: stored.name,
                usable: opened.is_some(),
                kind: opened
                    .as_ref()
                    .map_or_else(String::new, |(_, c)| c.kind().id().to_owned()),
                dataset: opened
                    .as_ref()
                    .map_or_else(String::new, |(_, c)| c.dataset().id.to_owned()),
            }
        })
        .collect())
}

fn document(name: &str, period: &str, component: Component) -> Document {
    Document::view(name, (!period.is_empty()).then_some(period), component)
}

/// Saves a new view; returns its ID. The book refuses an empty name or one
/// another view has.
pub fn save(
    book: &mut Book,
    name: &str,
    period: &str,
    component_json: &str,
) -> Result<String, ViewsError> {
    let component = Component::from_json(component_json)?;
    let text = document(name.trim(), period, component).to_json();
    // Read back what is about to be kept, so a period or name the reader
    // refuses never reaches the book.
    Document::read(&text)?;
    Ok(book.add_view(&text)?.to_string())
}

/// Opens a saved view.
pub fn open(book: &Book, id: &str) -> Result<Opened, ViewsError> {
    let stored = find(book, id)?;
    let (document, component) = component_of(&stored)?;
    Ok(Opened {
        id: stored.id.to_string(),
        name: stored.name,
        period: document.period.unwrap_or_default(),
        component: component.to_value().to_string(),
    })
}

/// Replaces what a saved view shows and its period; its name stays, and so
/// does the rest of its document: the accounts and categories it is
/// narrowed to and where it came from (`origin`, D-450), which the page
/// doesn't show (M6-B75).
pub fn update(
    book: &mut Book,
    id: &str,
    period: &str,
    component_json: &str,
) -> Result<(), ViewsError> {
    let stored = find(book, id)?;
    let component = Component::from_json(component_json)?;
    let fresh = document(&stored.name, period, component);
    let text = match Document::read(&stored.document) {
        Ok(mut kept) => {
            kept.period = fresh.period;
            kept.entries = fresh.entries;
            kept.to_json()
        }
        Err(_) => fresh.to_json(),
    };
    Document::read(&text)?;
    book.edit_view(stored.id, &text)?;
    Ok(())
}

/// Gives a saved view another name.
pub fn rename(book: &mut Book, id: &str, name: &str) -> Result<(), ViewsError> {
    let stored = find(book, id)?;
    Ok(book.rename_view(stored.id, name.trim())?)
}

/// Deletes a saved view. Undo brings it back.
pub fn remove(book: &mut Book, id: &str) -> Result<(), ViewsError> {
    let stored = find(book, id)?;
    Ok(book.remove_view(stored.id)?)
}

#[cfg(test)]
mod tests {
    use kassensturz_report::{ComponentType, Setting};

    use super::*;

    fn pie() -> Component {
        let mut pie = Component::new(ComponentType::ChartPie, "spending_by_category").unwrap();
        pie.set("title", Setting::Text("Where it went".into()))
            .unwrap();
        pie
    }

    fn json(component: &Component) -> String {
        component.to_value().to_string()
    }

    #[test]
    fn a_saved_view_opens_with_what_was_saved() {
        let mut book = Book::create_in_memory().unwrap();
        assert!(list(&book).unwrap().is_empty());
        let id = save(&mut book, "  Spending  ", "last-12-months", &json(&pie())).unwrap();
        let rows = list(&book).unwrap();
        assert_eq!(
            rows,
            [Row {
                id: id.clone(),
                name: "Spending".into(),
                usable: true,
                kind: "chart.pie".into(),
                dataset: "spending_by_category".into(),
            }]
        );
        let opened = open(&book, &id).unwrap();
        assert_eq!(opened.name, "Spending");
        assert_eq!(opened.period, "last-12-months");
        // The component text loads back into the same component.
        assert_eq!(Component::from_json(&opened.component).unwrap(), pie());
    }

    #[test]
    fn what_the_schema_or_the_book_refuses_is_not_saved() {
        let mut book = Book::create_in_memory().unwrap();
        assert!(matches!(
            save(&mut book, "A", "forever", &json(&pie())),
            Err(ViewsError::Report(_))
        ));
        assert!(matches!(
            save(&mut book, "A", "", "{"),
            Err(ViewsError::Report(_))
        ));
        assert!(matches!(
            save(&mut book, "   ", "", &json(&pie())),
            Err(ViewsError::Report(_) | ViewsError::Store(_))
        ));
        save(&mut book, "A", "", &json(&pie())).unwrap();
        assert!(matches!(
            save(&mut book, "a", "", &json(&pie())),
            Err(ViewsError::Store(StoreError::ViewNameTaken { .. }))
        ));
        assert_eq!(list(&book).unwrap().len(), 1);
    }

    #[test]
    fn rename_update_and_remove_are_changes_that_undo_takes_back() {
        let mut book = Book::create_in_memory().unwrap();
        let id = save(&mut book, "A", "this-year", &json(&pie())).unwrap();
        rename(&mut book, &id, "B").unwrap();
        let mut donut = pie();
        donut.set("legend", Setting::Flag(false)).unwrap();
        update(&mut book, &id, "last-year", &json(&donut)).unwrap();
        let opened = open(&book, &id).unwrap();
        assert_eq!(
            (opened.name.as_str(), opened.period.as_str()),
            ("B", "last-year")
        );
        remove(&mut book, &id).unwrap();
        assert!(list(&book).unwrap().is_empty());
        // Undo, one change at a time, in the order the history keeps them.
        book.undo().unwrap().expect("the removal");
        assert_eq!(open(&book, &id).unwrap().name, "B");
        book.undo().unwrap().expect("the update");
        assert_eq!(open(&book, &id).unwrap().period, "this-year");
        book.undo().unwrap().expect("the rename");
        assert_eq!(open(&book, &id).unwrap().name, "A");
        book.undo().unwrap().expect("the save");
        assert!(list(&book).unwrap().is_empty());
    }

    /// M6-B75: a view brought from KMyMoney is narrowed to accounts and
    /// knows its report; saving a change on the page keeps both.
    #[test]
    fn saving_changes_keeps_the_accounts_categories_and_origin_of_a_view() {
        let mut book = Book::create_in_memory().unwrap();
        let mut imported = document("From KMyMoney", "this-year", pie());
        imported.accounts = vec!["0190f5f2-7c3a-7000-8000-000000000001".into()];
        imported.categories = vec!["0190f5f2-7c3a-7000-8000-000000000002".into()];
        imported.origin = Some(kassensturz_report::ViewOrigin::kmymoney(
            "R000001",
            "2026-11-01".parse().unwrap(),
        ));
        let id = book.add_view(&imported.to_json()).unwrap().to_string();
        let mut donut = pie();
        donut.set("legend", Setting::Flag(false)).unwrap();
        update(&mut book, &id, "last-year", &json(&donut)).unwrap();
        let stored = find(&book, &id).unwrap();
        let kept = Document::read(&stored.document).unwrap();
        assert_eq!(kept.period.as_deref(), Some("last-year"));
        assert_eq!(kept.accounts, imported.accounts);
        assert_eq!(kept.categories, imported.categories);
        assert_eq!(kept.origin, imported.origin);
        assert_eq!(component_of(&stored).unwrap().1, donut);
    }

    #[test]
    fn an_unknown_view_is_an_error_not_a_panic() {
        let mut book = Book::create_in_memory().unwrap();
        assert!(matches!(open(&book, "nope"), Err(ViewsError::NoSuchView)));
        assert!(rename(&mut book, "nope", "x").is_err());
        assert!(remove(&mut book, "nope").is_err());
    }

    #[test]
    fn a_view_this_version_cannot_open_is_listed_and_can_be_deleted() {
        let mut book = Book::create_in_memory().unwrap();
        // Made by a newer version with a component this one doesn't know,
        // and one with a repeated key.
        let newer = r#"{"format":"kassensturz-dashboard","version":1,"kind":"view","name":"Radar","components":[{"id":"c1","type":"chart.radar","dataset":"x"}]}"#;
        let hostile = r#"{"format":"kassensturz-dashboard","version":1,"kind":"view","name":"Hostile","components":[{"id":"c1","type":"chart.pie","dataset":"spending_by_category","props":{"title":"a","title":"b"}}]}"#;
        let first = book.add_view(newer).unwrap().to_string();
        let second = book.add_view(hostile).unwrap().to_string();
        let rows = list(&book).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| !row.usable && row.kind.is_empty()));
        assert!(open(&book, &first).is_err());
        assert!(open(&book, &second).is_err());
        rename(&mut book, &first, "Old radar").unwrap();
        remove(&mut book, &second).unwrap();
        assert_eq!(list(&book).unwrap().len(), 1);
    }
}
