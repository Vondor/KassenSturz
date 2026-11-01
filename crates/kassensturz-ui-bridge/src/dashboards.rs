// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Several dashboards (M6d.1, D-436, D-438, D-448): the list the Overview
//! page switches between, which one a book opens at, and the actions of the
//! dashboard menu. No Qt in this file: `grid_editor_model.rs` hands the
//! results to QML.
//!
//! The dashboards are the book's documents of kind `dashboard`, in the
//! book's order. A book with none has one, the built-in Overview, which is
//! not stored. An action that needs the built-in Overview in the book
//! (renaming it, marking it, putting a second dashboard beside it) stores it
//! first, inside the same entry of the change history (`as_one_action`), so
//! one Undo takes the whole action back. Nothing here adds a layout: every
//! change goes through `add_view`, `edit_view`, `rename_view`, `move_view`
//! and `remove_view`.
//!
//! Names shown to the user come from the screen: the built-in Overview's
//! name in the user's language and the words of a copy's name are passed
//! in, never written here in English (D-034).

use kassensturz_core::{StoredView, ViewId, ViewKind};
use kassensturz_report::{Document, Editor, Frame, Item, OVERVIEW_NAME, templates};
use kassensturz_store::{Action, Book};

use crate::grid_editor::LayoutError;

/// The character a copy's numbered name carries where its number goes, as
/// the screen writes it (`Spending (copy \u{1})`).
pub const NUMBER_MARK: char = '\u{1}';

/// One dashboard of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dashboard {
    /// The book's ID; `None` for the built-in Overview, which is not stored.
    pub id: Option<ViewId>,
    /// The name, as stored, or the built-in Overview's name in the user's
    /// language.
    pub name: String,
    /// The template it was made from (`template`, D-439), with a dashboard
    /// stored before M6d that was the Overview counted as `overview`.
    pub template: Option<String>,
    /// Whether the user marked it (`"default": true`, D-448).
    pub marked: bool,
    /// Whether the book opens at it: the one marked, else the standard
    /// dashboard (D-448).
    pub is_default: bool,
    /// The stored document couldn't be read; the page shows the built-in
    /// layout for it.
    pub damaged: bool,
}

/// Whether `name` is the built-in Overview's, in English or in the user's
/// language, ignoring capitals the way the book compares names.
fn is_overview_name(name: &str, built_in_name: &str) -> bool {
    let name = name.to_lowercase();
    name == OVERVIEW_NAME.to_lowercase() || name == built_in_name.to_lowercase()
}

/// The book's dashboards as they are stored, in their order.
fn stored(book: &Book) -> Result<Vec<StoredView>, LayoutError> {
    Ok(book.views(Some(ViewKind::Dashboard))?)
}

/// The dashboards in the book's order, or the built-in Overview alone, with
/// the one the book opens at marked. `built_in_name` is the built-in
/// Overview's name in the user's language.
pub fn list(book: &Book, built_in_name: &str) -> Result<Vec<Dashboard>, LayoutError> {
    let views = stored(book)?;
    if views.is_empty() {
        return Ok(vec![Dashboard {
            id: None,
            name: built_in_name.to_owned(),
            template: Some(templates::OVERVIEW.to_owned()),
            marked: false,
            is_default: true,
            damaged: false,
        }]);
    }
    let mut dashboards: Vec<Dashboard> = views
        .into_iter()
        .enumerate()
        .map(|(index, view)| {
            let document = Document::read(&view.document).ok();
            let template = document
                .as_ref()
                .and_then(|document| document.template.clone())
                .or_else(|| {
                    // The Overview of M6c, stored before dashboards had a
                    // template: the first one, named Overview (section 3).
                    (index == 0 && is_overview_name(&view.name, built_in_name))
                        .then(|| templates::OVERVIEW.to_owned())
                });
            Dashboard {
                id: Some(view.id),
                marked: document.as_ref().is_some_and(|document| document.default),
                damaged: document.is_none(),
                name: view.name,
                template,
                is_default: false,
            }
        })
        .collect();
    let chosen = default_index(&dashboards);
    dashboards[chosen].is_default = true;
    Ok(dashboards)
}

/// Which of `dashboards` the book opens at (D-448): the first one marked,
/// else the first made from the `overview` template, else the first.
fn default_index(dashboards: &[Dashboard]) -> usize {
    dashboards
        .iter()
        .position(|dashboard| dashboard.marked)
        .or_else(|| {
            dashboards
                .iter()
                .position(|dashboard| dashboard.template.as_deref() == Some(templates::OVERVIEW))
        })
        .unwrap_or(0)
}

/// The name of the dashboard the book would open at once `id` is deleted:
/// what the question before deleting says (D-448).
pub fn default_without(
    book: &Book,
    id: ViewId,
    built_in_name: &str,
) -> Result<String, LayoutError> {
    let mut rest = list(book, built_in_name)?;
    rest.retain(|dashboard| dashboard.id != Some(id));
    if rest.is_empty() {
        return Ok(built_in_name.to_owned());
    }
    Ok(rest[default_index(&rest)].name.clone())
}

/// The stored dashboard `id`, given as the text QML holds.
pub fn find(book: &Book, id: &str) -> Result<ViewId, LayoutError> {
    stored(book)?
        .into_iter()
        .find(|view| view.id.to_string() == id)
        .map(|view| view.id)
        .ok_or(LayoutError::NoSuchLayout)
}

/// Stores the built-in Overview, as it is shown, if the book has no
/// dashboard yet; gives the ID the action works on. `shown` is the stored
/// dashboard the page shows, `None` for the built-in one. Called inside an
/// action, so storing is part of its one entry.
fn stored_or_built_in(
    book: &mut Book,
    shown: Option<ViewId>,
    built_in_name: &str,
) -> Result<ViewId, LayoutError> {
    if let Some(id) = shown {
        return Ok(id);
    }
    if let Some(first) = stored(book)?.first() {
        // The page showed the built-in Overview while a dashboard was
        // stored: only possible if the book changed underneath it.
        return Ok(first.id);
    }
    let mut document = templates::document(templates::OVERVIEW)?;
    built_in_name.trim().clone_into(&mut document.name);
    Ok(book.add_view(&checked(&document)?)?)
}

/// The document as text, read back first so a document the reader refuses
/// never reaches the book.
fn checked(document: &Document) -> Result<String, LayoutError> {
    let text = document.to_json();
    Document::read(&text)?;
    Ok(text)
}

/// A new dashboard from the template `word` (one of
/// [`templates::WORDS`]), named `name`, at the end of the list (*New
/// dashboard…*). `heading` is the template's heading in the user's language
/// (D-440), put in once now; a template without a heading ignores it. The
/// built-in Overview is stored first if it is the only one, so it keeps its
/// place in front. One entry of the change history; gives the new ID.
pub fn add(
    book: &mut Book,
    word: &str,
    name: &str,
    heading: &str,
    built_in_name: &str,
) -> Result<ViewId, LayoutError> {
    let mut document = templates::document(word)?;
    templates::fill_headings(&mut document, heading);
    name.trim().clone_into(&mut document.name);
    let text = checked(&document)?;
    book.as_one_action(Action::AddView, |book| {
        stored_or_built_in(book, None, built_in_name)?;
        Ok(book.add_view(&text)?)
    })
}

/// What the *New dashboard* dialog draws for a template (D-440): each
/// component's type id and frame, in the template's order. Empty for
/// `empty`; an unknown word gives nothing.
pub fn picture(word: &str) -> Vec<(&'static str, Frame)> {
    templates::document(word)
        .map(|document| {
            document
                .entries
                .iter()
                .filter_map(|entry| match (&entry.item, entry.frame) {
                    (Item::Component(component), Some(frame)) => {
                        Some((component.kind().id(), frame))
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Keeps what the editor shows as a new dashboard named `name`, at the end
/// of the list (*Save as a new dashboard…*, D-330's behaviour): its own
/// entry in the change history, and the session goes on. It keeps the
/// template of the one it was copied from, never the default mark.
pub fn save_as(
    book: &mut Book,
    editor: &Editor,
    name: &str,
    built_in_name: &str,
) -> Result<ViewId, LayoutError> {
    let mut document = editor.document().clone();
    name.trim().clone_into(&mut document.name);
    document.default = false;
    let text = checked(&document)?;
    book.as_one_action(Action::AddView, |book| {
        stored_or_built_in(book, None, built_in_name)?;
        Ok(book.add_view(&text)?)
    })
}

/// Gives the dashboard the page shows another name (*Rename…*); the
/// built-in Overview is stored first. One entry; gives its ID.
pub fn rename(
    book: &mut Book,
    shown: Option<ViewId>,
    name: &str,
    built_in_name: &str,
) -> Result<ViewId, LayoutError> {
    book.as_one_action(Action::RenameView, |book| {
        let id = stored_or_built_in(book, shown, built_in_name)?;
        book.rename_view(id, name)?;
        Ok(id)
    })
}

/// The first of `first`, then `numbered` with 2, 3, … in place of
/// [`NUMBER_MARK`], that no dashboard has, ignoring capitals.
fn free_name(taken: &[String], first: &str, numbered: &str) -> String {
    let taken: Vec<String> = taken.iter().map(|name| name.to_lowercase()).collect();
    let free = |name: &str| !taken.contains(&name.to_lowercase());
    let first = first.trim();
    if free(first) {
        return first.to_owned();
    }
    (2_u32..)
        .map(|number| {
            numbered
                .replace(NUMBER_MARK, &number.to_string())
                .trim()
                .to_owned()
        })
        .find(|name| free(name))
        .unwrap_or_default()
}

/// A copy of the dashboard the page shows right after it (*Duplicate*),
/// named `first` (*Name (copy)*) or, if that is taken, `numbered` with the
/// first free number for [`NUMBER_MARK`]. It keeps the template and never
/// the default mark. The built-in Overview is stored first. One entry;
/// gives the copy's ID.
pub fn duplicate(
    book: &mut Book,
    shown: Option<ViewId>,
    first: &str,
    numbered: &str,
    built_in_name: &str,
) -> Result<ViewId, LayoutError> {
    book.as_one_action(Action::AddView, |book| {
        let id = stored_or_built_in(book, shown, built_in_name)?;
        let original = book.view(id)?;
        let mut document = Document::read(&original.document)?;
        let taken: Vec<String> = stored(book)?.into_iter().map(|view| view.name).collect();
        document.name = free_name(&taken, first, numbered);
        document.default = false;
        let copy = book.add_view(&checked(&document)?)?;
        book.move_view(copy, original.position.saturating_add(1))?;
        Ok(copy)
    })
}

/// Marks the dashboard the page shows as the one the book opens at (*Make
/// this the default*, D-448) and takes the mark off any other. The built-in
/// Overview is stored first. One entry; gives its ID.
pub fn make_default(
    book: &mut Book,
    shown: Option<ViewId>,
    built_in_name: &str,
) -> Result<ViewId, LayoutError> {
    book.as_one_action(Action::EditView, |book| {
        let id = stored_or_built_in(book, shown, built_in_name)?;
        for view in stored(book)? {
            // A document that can't be read keeps what it has: it is not
            // repaired here, and the list treats it as not marked.
            let Ok(mut document) = Document::read(&view.document) else {
                if view.id == id {
                    return Err(LayoutError::Report(
                        Document::read(&view.document).unwrap_err(),
                    ));
                }
                continue;
            };
            let wanted = view.id == id;
            if document.default != wanted {
                document.default = wanted;
                book.edit_view(view.id, &checked(&document)?)?;
            }
        }
        Ok(id)
    })
}

/// Moves the stored dashboard `id` by `by` places in the list (*Move
/// earlier* is -1, *Move later* 1); a place before the first or past the
/// last means the first or the last. One entry; gives whether it moved.
pub fn move_by(book: &mut Book, id: ViewId, by: i32) -> Result<bool, LayoutError> {
    let views = stored(book)?;
    let at = views
        .iter()
        .position(|view| view.id == id)
        .ok_or(LayoutError::NoSuchLayout)?;
    let last = views.len() - 1;
    let to = i64::try_from(at)
        .unwrap_or(0)
        .saturating_add(i64::from(by))
        .clamp(0, i64::try_from(last).unwrap_or(0));
    let to = usize::try_from(to).unwrap_or(at);
    if to == at {
        return Ok(false);
    }
    book.move_view(id, u32::try_from(to).unwrap_or(u32::MAX))?;
    Ok(true)
}

/// Deletes the stored dashboard `id` (*Delete…*). Deleting the last one
/// leaves the built-in Overview, so the page is never empty. One entry.
pub fn delete(book: &mut Book, id: ViewId) -> Result<(), LayoutError> {
    Ok(book.remove_view(id)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid_editor::{Layout, Saved};
    use kassensturz_store::StoreError;

    const BUILT_IN: &str = "Übersicht";

    fn book() -> Book {
        Book::create_in_memory().unwrap()
    }

    fn names(book: &Book) -> Vec<String> {
        list(book, BUILT_IN)
            .unwrap()
            .into_iter()
            .map(|dashboard| dashboard.name)
            .collect()
    }

    fn default_name(book: &Book) -> String {
        list(book, BUILT_IN)
            .unwrap()
            .into_iter()
            .find(|dashboard| dashboard.is_default)
            .unwrap()
            .name
    }

    #[test]
    fn a_book_without_a_dashboard_has_the_built_in_overview_alone() {
        let shown = list(&book(), BUILT_IN).unwrap();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].id, None);
        assert_eq!(shown[0].name, BUILT_IN);
        assert!(shown[0].is_default);
        assert_eq!(shown[0].template.as_deref(), Some("overview"));
    }

    #[test]
    fn a_new_dashboard_stores_the_overview_first_in_one_entry_undo_takes_back() {
        let mut book = book();
        let before = book.history().unwrap().len();
        let id = add(&mut book, "empty", " Holidays ", "", BUILT_IN).unwrap();
        assert_eq!(names(&book), [BUILT_IN, "Holidays"]);
        assert_eq!(book.history().unwrap().len(), before + 1);
        // The built-in Overview, stored as it was shown, opens the book.
        assert_eq!(default_name(&book), BUILT_IN);
        let stored = Document::read(&book.view(id).unwrap().document).unwrap();
        assert_eq!(stored.template.as_deref(), Some("empty"));
        assert!(stored.entries.is_empty());
        book.undo().unwrap();
        assert!(book.views(Some(ViewKind::Dashboard)).unwrap().is_empty());
        assert_eq!(names(&book), [BUILT_IN]);
    }

    #[test]
    fn a_second_new_dashboard_does_not_store_the_overview_again() {
        let mut book = book();
        add(&mut book, "empty", "One", "", BUILT_IN).unwrap();
        add(&mut book, "overview", "Two", "", BUILT_IN).unwrap();
        assert_eq!(names(&book), [BUILT_IN, "One", "Two"]);
        assert!(matches!(
            add(&mut book, "empty", "one", "", BUILT_IN),
            Err(LayoutError::Store(StoreError::ViewNameTaken { .. }))
        ));
        assert!(add(&mut book, "radar", "Three", "", BUILT_IN).is_err());
        assert!(add(&mut book, "empty", "  ", "", BUILT_IN).is_err());
        assert_eq!(names(&book).len(), 3);
    }

    #[test]
    fn renaming_the_built_in_overview_stores_it_in_one_entry() {
        let mut book = book();
        let before = book.history().unwrap().len();
        let id = rename(&mut book, None, "Home", BUILT_IN).unwrap();
        assert_eq!(names(&book), ["Home"]);
        assert_eq!(book.history().unwrap().len(), before + 1);
        // Still the standard dashboard: it was made from the Overview.
        assert_eq!(default_name(&book), "Home");
        rename(&mut book, Some(id), "Start", BUILT_IN).unwrap();
        assert_eq!(names(&book), ["Start"]);
        book.undo().unwrap();
        book.undo().unwrap();
        assert_eq!(names(&book), [BUILT_IN]);
    }

    #[test]
    fn a_duplicate_comes_right_after_with_a_free_name_and_no_mark() {
        let mut book = book();
        let first = "Übersicht (Kopie)";
        let numbered = "Übersicht (Kopie \u{1})";
        let copy = duplicate(&mut book, None, first, numbered, BUILT_IN).unwrap();
        add(&mut book, "empty", "Later", "", BUILT_IN).unwrap();
        let overview = list(&book, BUILT_IN).unwrap()[0].id;
        make_default(&mut book, overview, BUILT_IN).unwrap();
        duplicate(&mut book, overview, first, numbered, BUILT_IN).unwrap();
        assert_eq!(
            names(&book),
            [BUILT_IN, "Übersicht (Kopie 2)", first, "Later"]
        );
        let shown = list(&book, BUILT_IN).unwrap();
        assert!(shown[0].marked);
        assert!(shown.iter().skip(1).all(|dashboard| !dashboard.marked));
        // The copy keeps the template.
        let copied = Document::read(&book.view(copy).unwrap().document).unwrap();
        assert_eq!(copied.template.as_deref(), Some("overview"));
    }

    #[test]
    fn the_default_is_the_marked_one_else_the_overview_else_the_first() {
        let mut book = book();
        let holidays = add(&mut book, "empty", "Holidays", "", BUILT_IN).unwrap();
        assert_eq!(default_name(&book), BUILT_IN);
        let before = book.history().unwrap().len();
        make_default(&mut book, Some(holidays), BUILT_IN).unwrap();
        assert_eq!(book.history().unwrap().len(), before + 1);
        assert_eq!(default_name(&book), "Holidays");
        // Marking another takes the mark off the first, in one entry.
        let overview = list(&book, BUILT_IN).unwrap()[0].id.unwrap();
        make_default(&mut book, Some(overview), BUILT_IN).unwrap();
        let shown = list(&book, BUILT_IN).unwrap();
        assert_eq!(shown.iter().filter(|dashboard| dashboard.marked).count(), 1);
        assert!(shown[0].marked);
        book.undo().unwrap();
        assert_eq!(default_name(&book), "Holidays");
        // Deleting the marked one says which opens instead, then opens it.
        assert_eq!(
            default_without(&book, holidays, BUILT_IN).unwrap(),
            BUILT_IN
        );
        delete(&mut book, holidays).unwrap();
        assert_eq!(default_name(&book), BUILT_IN);
        // Without an Overview, the first of the list.
        delete(&mut book, overview).unwrap();
        add(&mut book, "empty", "A", "", BUILT_IN).unwrap();
        let second = add(&mut book, "empty", "B", "", BUILT_IN).unwrap();
        // The new Overview was stored first, so delete it to test the rule.
        let overview = list(&book, BUILT_IN).unwrap()[0].id.unwrap();
        delete(&mut book, overview).unwrap();
        assert_eq!(names(&book), ["A", "B"]);
        assert_eq!(default_name(&book), "A");
        assert_eq!(default_without(&book, second, BUILT_IN).unwrap(), "A");
    }

    #[test]
    fn a_mark_found_twice_takes_the_first_and_the_next_marking_clears_the_others() {
        let mut book = book();
        for name in ["One", "Two"] {
            let mut document = templates::document("empty").unwrap();
            document.name = name.to_owned();
            document.default = true;
            book.add_view(&document.to_json()).unwrap();
        }
        assert_eq!(default_name(&book), "One");
        let two = list(&book, BUILT_IN).unwrap()[1].id;
        make_default(&mut book, two, BUILT_IN).unwrap();
        let shown = list(&book, BUILT_IN).unwrap();
        assert!(!shown[0].marked);
        assert!(shown[1].marked);
    }

    #[test]
    fn an_m6c_overview_and_its_saved_layouts_become_the_list_in_their_order() {
        let mut book = book();
        // As M6c stored them: no template key, the Overview first.
        let mut overview = Document::default_overview().unwrap();
        overview.entries.pop();
        book.add_view(&overview.to_json()).unwrap();
        let mut saved = Document::default_overview().unwrap();
        saved.name = "Month review".to_owned();
        book.add_view(&saved.to_json()).unwrap();
        let shown = list(&book, BUILT_IN).unwrap();
        assert_eq!(names(&book), ["Overview", "Month review"]);
        assert_eq!(shown[0].template.as_deref(), Some("overview"));
        assert_eq!(shown[1].template, None);
        assert!(shown[0].is_default);
    }

    #[test]
    fn dashboards_move_earlier_and_later_and_stop_at_the_ends() {
        let mut book = book();
        let one = add(&mut book, "empty", "One", "", BUILT_IN).unwrap();
        add(&mut book, "empty", "Two", "", BUILT_IN).unwrap();
        assert!(move_by(&mut book, one, -1).unwrap());
        assert_eq!(names(&book), ["One", BUILT_IN, "Two"]);
        assert!(!move_by(&mut book, one, -1).unwrap());
        assert!(move_by(&mut book, one, 5).unwrap());
        assert_eq!(names(&book), [BUILT_IN, "Two", "One"]);
        book.undo().unwrap();
        assert_eq!(names(&book), ["One", BUILT_IN, "Two"]);
    }

    #[test]
    fn deleting_the_last_stored_dashboard_leaves_the_built_in_overview() {
        let mut book = book();
        let id = rename(&mut book, None, "Home", BUILT_IN).unwrap();
        delete(&mut book, id).unwrap();
        assert_eq!(names(&book), [BUILT_IN]);
        assert!(matches!(
            find(&book, &id.to_string()),
            Err(LayoutError::NoSuchLayout)
        ));
        book.undo().unwrap();
        assert_eq!(find(&book, &id.to_string()).unwrap(), id);
    }

    #[test]
    fn saving_as_a_new_dashboard_keeps_the_overview_and_the_session() {
        let mut book = book();
        let mut layout = Layout::open(&book, None, BUILT_IN).unwrap();
        layout.editor.nudge("trend", 0, 2).unwrap();
        let id = save_as(&mut book, &layout.editor, "Arranged", BUILT_IN).unwrap();
        assert_eq!(names(&book), [BUILT_IN, "Arranged"]);
        // The built-in Overview was stored as it is, without the session's
        // change; the session goes on and is kept on the Overview by Done.
        let overview = list(&book, BUILT_IN).unwrap()[0].id;
        let stored = Document::read(&book.view(overview.unwrap()).unwrap().document).unwrap();
        assert_eq!(
            stored.entries,
            Document::default_overview().unwrap().entries
        );
        let saved = Document::read(&book.view(id).unwrap().document).unwrap();
        assert_eq!(saved.entries, layout.editor.document().entries);
        assert!(layout.editor.has_changes());
        // The page's session now works on the stored Overview.
        layout.stored = overview;
        assert_eq!(layout.save(&mut book).unwrap(), Saved::Edited);
        assert_eq!(names(&book), [BUILT_IN, "Arranged"]);
        let stored = Document::read(&book.view(overview.unwrap()).unwrap().document).unwrap();
        assert_eq!(stored.entries, layout.editor.document().entries);
    }

    #[test]
    fn free_names_count_up_ignoring_capitals() {
        let taken = ["A (copy)".to_owned(), "a (COPY 2)".to_owned()];
        assert_eq!(free_name(&taken, "B (copy)", "B (copy \u{1})"), "B (copy)");
        assert_eq!(
            free_name(&taken, "A (copy)", "A (copy \u{1})"),
            "A (copy 3)"
        );
    }

    #[test]
    fn every_template_makes_a_dashboard_with_its_heading_in_the_users_words() {
        let mut book = book();
        for word in templates::WORDS {
            let id = add(&mut book, word, word, "Wohin das Geld geht", BUILT_IN).unwrap();
            let text = book.view(id).unwrap().document;
            let stored = Document::read(&text).unwrap();
            assert_eq!(stored.template.as_deref(), Some(word));
            assert_eq!(stored.name, word);
            assert!(!text.contains(templates::HEADING_KEY), "{word}");
            let headings = text.matches("\"text.heading\"").count();
            assert_eq!(
                text.matches("Wohin das Geld geht").count(),
                headings,
                "{word}"
            );
            // The picture shows what was stored, in the same places.
            let drawn: Vec<_> = picture(word).into_iter().map(|(_, frame)| frame).collect();
            let made: Vec<_> = stored
                .entries
                .iter()
                .filter_map(|entry| entry.frame)
                .collect();
            assert_eq!(drawn, made, "{word}");
        }
        assert_eq!(names(&book).len(), templates::WORDS.len() + 1);
    }

    #[test]
    fn the_accounts_picture_holds_the_update_all_button() {
        let kinds: Vec<_> = picture(templates::ACCOUNTS)
            .into_iter()
            .map(|(kind, _)| kind)
            .collect();
        assert!(kinds.contains(&"bank.update_all"), "{kinds:?}");
        assert!(picture(templates::EMPTY).is_empty());
        assert!(picture("radar").is_empty());
    }
}
