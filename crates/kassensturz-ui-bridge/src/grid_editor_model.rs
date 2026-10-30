// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Overview's dashboard and its grid editor as a QML object (M6b.3,
//! D-287, D-288, D-311), on the book the app has open (`session.rs`). The
//! work is in `grid_editor.rs`; this file turns it into plain values.
//!
//! `view` holds `editing`, `damaged` (the stored layout couldn't be read, so
//! the built-in one is shown), `hasChanges`, `canUndo`, `canRedo`,
//! `undoEdit` and `redoEdit` (the edit's fixed word, or empty), `builtIn`
//! (the layout is the built-in one), `columns`, `maxRows`, `rows` (how many
//! rows the grid is high), `library` (`type` and `dataset` of each
//! component the editor can add) and `items` in reading order. An item has
//! `id`, `unknown`, `type`, `col`, `row`, `cols`, `rows`, `minCols`,
//! `minRows` and `subject`: the component as `ComponentProperties.view`
//! shows it, which `ComponentPreview` and the properties panel take.
//!
//! The calls that edit return an empty object (or `id` of a new component)
//! when they worked, or an object with the error's `code` and parameters. A
//! refused edit changes nothing. Nothing is written to the book until
//! `done`, which is one entry of the change history (D-288).

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_core::Report;
use kassensturz_report::{NumberForm, grid};
use kassensturz_store::Book;

use crate::errors::{ErrorText, report_text};
use crate::formulas;
use crate::grid_editor::{self, Layout, LayoutError};
use crate::locale::QtLocale;
use crate::number_format::{UserLocale, app_locale};
use crate::properties_model::view_object as component_view;
use crate::session;
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The Overview's dashboard, shown or being arranged.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type DashboardLayout = super::DashboardLayoutRust;
    }

    extern "RustQt" {
        /// Reads the layout from the open book again, e.g. after Undo. A
        /// session that is under way is left alone.
        #[qinvokable]
        fn refresh(self: Pin<&mut DashboardLayout>);

        /// Starts a session: arranging the layout, with an undo of its own.
        #[qinvokable]
        fn edit(self: Pin<&mut DashboardLayout>);

        /// Throws the session away.
        #[qinvokable]
        fn cancel(self: Pin<&mut DashboardLayout>);

        /// Keeps the session in the book as one change. Gives the error, or
        /// an empty object.
        #[qinvokable]
        fn done(self: Pin<&mut DashboardLayout>) -> QVariant;

        /// Adds a component of this type (`chart.line`, ...) with its
        /// defaults. Gives its `id`, or the error.
        #[qinvokable]
        fn add(self: Pin<&mut DashboardLayout>, kind: &QString) -> QVariant;

        /// Adds a copy of a saved view's component (what `SavedViews.open`
        /// gives as `component` and `period`). Gives its `id`, or the error.
        #[qinvokable]
        #[cxx_name = "addView"]
        fn add_view(
            self: Pin<&mut DashboardLayout>,
            component: &QString,
            period: &QString,
        ) -> QVariant;

        /// Moves a component so its top left corner is at this cell.
        #[qinvokable]
        #[cxx_name = "moveTo"]
        fn move_to(self: Pin<&mut DashboardLayout>, id: &QString, col: i32, row: i32) -> QVariant;

        /// Moves a component by whole cells.
        #[qinvokable]
        fn nudge(self: Pin<&mut DashboardLayout>, id: &QString, cols: i32, rows: i32) -> QVariant;

        /// Gives a component this size, in cells.
        #[qinvokable]
        #[cxx_name = "resizeTo"]
        fn resize_to(
            self: Pin<&mut DashboardLayout>,
            id: &QString,
            cols: i32,
            rows: i32,
        ) -> QVariant;

        /// Makes a component wider, narrower, taller or lower by cells.
        #[qinvokable]
        fn grow(self: Pin<&mut DashboardLayout>, id: &QString, cols: i32, rows: i32) -> QVariant;

        /// Copies a component. Gives the copy's `id`, or the error.
        #[qinvokable]
        fn duplicate(self: Pin<&mut DashboardLayout>, id: &QString) -> QVariant;

        /// Removes a component.
        #[qinvokable]
        fn remove(self: Pin<&mut DashboardLayout>, id: &QString) -> QVariant;

        /// Moves a component one place earlier in reading order.
        #[qinvokable]
        #[cxx_name = "moveEarlier"]
        fn move_earlier(self: Pin<&mut DashboardLayout>, id: &QString) -> QVariant;

        /// Moves a component one place later in reading order.
        #[qinvokable]
        #[cxx_name = "moveLater"]
        fn move_later(self: Pin<&mut DashboardLayout>, id: &QString) -> QVariant;

        /// Replaces a component with one the properties panel made (its
        /// `view.json`).
        #[qinvokable]
        #[cxx_name = "setComponent"]
        fn set_component(self: Pin<&mut DashboardLayout>, id: &QString, json: &QString)
        -> QVariant;

        /// Takes the last edit of the session back; gives its word, or
        /// empty.
        #[qinvokable]
        fn undo(self: Pin<&mut DashboardLayout>) -> QString;

        /// Makes the last edit taken back again; gives its word, or empty.
        #[qinvokable]
        fn redo(self: Pin<&mut DashboardLayout>) -> QString;

        /// Puts the built-in layout back, as an edit Undo takes back.
        #[qinvokable]
        fn reset(self: Pin<&mut DashboardLayout>) -> QVariant;

        /// The cell nearest to an offset in pixels, for cells of `cell`
        /// pixels.
        #[qinvokable]
        fn snap(self: &DashboardLayout, offset: i32, cell: i32) -> i32;

        /// Whether a page of this width, with this gap between columns, is
        /// too narrow for the columns.
        #[qinvokable]
        fn narrow(self: &DashboardLayout, width: i32, gap: i32, unit: i32) -> bool;
    }

    extern "RustQt" {
        /// The layout was kept in the book through this object.
        #[qsignal]
        fn changed(self: Pin<&mut DashboardLayout>);
    }
}

/// The Rust side of [`qobject::DashboardLayout`].
#[derive(Default)]
pub struct DashboardLayoutRust {
    view: QVariant,
    layout: Option<Layout>,
    editing: bool,
}

fn number(value: u32) -> QVariant {
    QVariant::from(&i32::try_from(value).unwrap_or(i32::MAX))
}

fn item_object(row: &grid_editor::Row, form: NumberForm) -> QVariant {
    let kind = row
        .component
        .as_ref()
        .map_or("", |component| component.kind().id());
    map([
        ("id", text(&row.id)),
        ("unknown", QVariant::from(&row.component.is_none())),
        ("type", text(kind)),
        ("col", number(row.frame.col)),
        ("row", number(row.frame.row)),
        ("cols", number(row.frame.cols)),
        ("rows", number(row.frame.rows)),
        ("minCols", number(row.least.cols)),
        ("minRows", number(row.least.rows)),
        (
            "subject",
            row.component.as_ref().map_or_else(
                || map([]),
                |component| component_view(Some(component), None, form),
            ),
        ),
    ])
}

/// What QML gets for the layout; empty when no book is open.
fn view_object(layout: Option<&Layout>, editing: bool, form: NumberForm) -> QVariant {
    let Some(layout) = layout else {
        return map([]);
    };
    let (columns, max_rows) = grid_editor::size();
    let editor = &layout.editor;
    map([
        ("editing", QVariant::from(&editing)),
        ("damaged", QVariant::from(&layout.damaged)),
        ("hasChanges", QVariant::from(&editor.has_changes())),
        ("canUndo", QVariant::from(&editor.next_undo().is_some())),
        ("canRedo", QVariant::from(&editor.next_redo().is_some())),
        ("undoEdit", text(grid_editor::edit_word(editor.next_undo()))),
        ("redoEdit", text(grid_editor::edit_word(editor.next_redo()))),
        ("builtIn", QVariant::from(&layout.is_built_in())),
        ("columns", number(columns)),
        ("maxRows", number(max_rows)),
        ("rows", number(grid_editor::total_rows(editor))),
        (
            "library",
            list(
                grid_editor::library()
                    .into_iter()
                    .map(|(kind, dataset)| map([("type", text(kind)), ("dataset", text(dataset))])),
            ),
        ),
        (
            "items",
            list(
                grid_editor::rows(editor)
                    .iter()
                    .map(|row| item_object(row, form)),
            ),
        ),
    ])
}

/// The error as QML gets it.
fn error_text(book: &Book, error: &LayoutError, locale: &UserLocale<QtLocale>) -> ErrorText {
    match error {
        LayoutError::Store(error) => report_text(book, &error.report(), locale),
        LayoutError::Report(error) => report_text(book, &error.report(), locale),
    }
}

fn read_book<T>(f: impl FnOnce(&Book) -> Result<T, LayoutError>) -> Result<T, ErrorText> {
    let locale = app_locale();
    session::with_book(|book| f(book).map_err(|error| error_text(book, &error, &locale)))
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")))
}

fn change_book<T>(f: impl FnOnce(&mut Book) -> Result<T, LayoutError>) -> Result<T, ErrorText> {
    let locale = app_locale();
    session::with_book_mut(|book| f(book).map_err(|error| error_text(book, &error, &locale)))
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")))
}

/// An error put into words with the open book, if there is one.
fn plain_error(error: &LayoutError) -> ErrorText {
    let locale = app_locale();
    session::with_book(|book| error_text(book, error, &locale))
        .unwrap_or_else(|| ErrorText::plain("input.no-such-entry"))
}

impl qobject::DashboardLayout {
    fn show(mut self: Pin<&mut Self>) {
        let form = formulas::form(&app_locale());
        let view = view_object(self.rust().layout.as_ref(), self.rust().editing, form);
        self.as_mut().rust_mut().view = view;
        self.as_mut().view_changed();
    }

    /// Reads the layout from the book and shows it, ending any session.
    fn load(mut self: Pin<&mut Self>, editing: bool) {
        let layout = read_book(Layout::open).ok();
        let mut this = self.as_mut().rust_mut();
        this.layout = layout;
        this.editing = editing && this.layout.is_some();
        self.show();
    }

    fn refresh(self: Pin<&mut Self>) {
        if !self.rust().editing {
            self.load(false);
        }
    }

    fn edit(self: Pin<&mut Self>) {
        self.load(true);
    }

    fn cancel(self: Pin<&mut Self>) {
        self.load(false);
    }

    fn done(mut self: Pin<&mut Self>) -> QVariant {
        let Some(layout) = self.rust().layout.as_ref() else {
            return map([]);
        };
        match change_book(|book| layout.save(book)) {
            Ok(saved) => {
                self.as_mut().load(false);
                if saved != grid_editor::Saved::Nothing {
                    self.as_mut().changed();
                }
                map([])
            }
            Err(error) => error_object(&error),
        }
    }

    /// Makes one edit on the session. Shows the result and gives what
    /// `answer` makes of it, or the error.
    fn edited<T>(
        mut self: Pin<&mut Self>,
        edit: impl FnOnce(&mut Layout) -> Result<T, LayoutError>,
        answer: impl FnOnce(T) -> QVariant,
    ) -> QVariant {
        let result = {
            let mut rust = self.as_mut().rust_mut();
            rust.layout.as_mut().map(edit)
        };
        match result {
            None => map([]),
            Some(Ok(done)) => {
                self.as_mut().show();
                answer(done)
            }
            Some(Err(error)) => error_object(&plain_error(&error)),
        }
    }

    fn add(self: Pin<&mut Self>, kind: &QString) -> QVariant {
        let kind = kind.to_string();
        self.edited(
            |layout| grid_editor::add(&mut layout.editor, &kind),
            |id| map([("id", text(&id))]),
        )
    }

    fn add_view(self: Pin<&mut Self>, component: &QString, period: &QString) -> QVariant {
        let (component, period) = (component.to_string(), period.to_string());
        self.edited(
            |layout| grid_editor::add_view(&mut layout.editor, &component, &period),
            |id| map([("id", text(&id))]),
        )
    }

    fn move_to(self: Pin<&mut Self>, id: &QString, col: i32, row: i32) -> QVariant {
        let id = id.to_string();
        self.edited(
            |layout| Ok(layout.editor.move_to(&id, col.into(), row.into())?),
            |_| map([]),
        )
    }

    fn nudge(self: Pin<&mut Self>, id: &QString, cols: i32, rows: i32) -> QVariant {
        let id = id.to_string();
        self.edited(
            |layout| Ok(layout.editor.nudge(&id, cols.into(), rows.into())?),
            |_| map([]),
        )
    }

    fn resize_to(self: Pin<&mut Self>, id: &QString, cols: i32, rows: i32) -> QVariant {
        let id = id.to_string();
        self.edited(
            |layout| Ok(layout.editor.resize_to(&id, cols.into(), rows.into())?),
            |_| map([]),
        )
    }

    fn grow(self: Pin<&mut Self>, id: &QString, cols: i32, rows: i32) -> QVariant {
        let id = id.to_string();
        self.edited(
            |layout| Ok(layout.editor.grow(&id, cols.into(), rows.into())?),
            |_| map([]),
        )
    }

    fn duplicate(self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        self.edited(
            |layout| Ok(layout.editor.duplicate(&id)?),
            |copy| map([("id", text(&copy))]),
        )
    }

    fn remove(self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        self.edited(|layout| Ok(layout.editor.delete(&id)?), |_| map([]))
    }

    fn move_earlier(self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        self.edited(|layout| Ok(layout.editor.move_earlier(&id)?), |_| map([]))
    }

    fn move_later(self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        self.edited(|layout| Ok(layout.editor.move_later(&id)?), |_| map([]))
    }

    fn set_component(self: Pin<&mut Self>, id: &QString, json: &QString) -> QVariant {
        let (id, json) = (id.to_string(), json.to_string());
        self.edited(
            |layout| grid_editor::set_component(&mut layout.editor, &id, &json),
            |_| map([]),
        )
    }

    fn undo(mut self: Pin<&mut Self>) -> QString {
        let word = self
            .as_mut()
            .rust_mut()
            .layout
            .as_mut()
            .and_then(|layout| layout.editor.undo());
        self.as_mut().show();
        QString::from(grid_editor::edit_word(word))
    }

    fn redo(mut self: Pin<&mut Self>) -> QString {
        let word = self
            .as_mut()
            .rust_mut()
            .layout
            .as_mut()
            .and_then(|layout| layout.editor.redo());
        self.as_mut().show();
        QString::from(grid_editor::edit_word(word))
    }

    fn reset(self: Pin<&mut Self>) -> QVariant {
        self.edited(
            |layout| Ok(layout.editor.reset(&grid_editor::built_in()?)?),
            |_| map([]),
        )
    }

    fn snap(&self, offset: i32, cell: i32) -> i32 {
        i32::try_from(grid::snap(offset.into(), cell.into())).unwrap_or(0)
    }

    fn narrow(&self, width: i32, gap: i32, unit: i32) -> bool {
        grid::is_narrow(width.into(), gap.into(), unit.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::read_back::{flag_of, keys, list_of, object_of, text_of};

    fn layout() -> Layout {
        Layout::open(&Book::create_in_memory().unwrap()).unwrap()
    }

    #[test]
    fn the_layout_reads_back_the_way_qml_reads_it() {
        let layout = layout();
        let view = view_object(Some(&layout), true, NumberForm::DecimalComma);
        assert_eq!(
            keys(&view),
            [
                "builtIn",
                "canRedo",
                "canUndo",
                "columns",
                "damaged",
                "editing",
                "hasChanges",
                "items",
                "library",
                "maxRows",
                "redoEdit",
                "rows",
                "undoEdit"
            ]
        );
        assert!(flag_of(&view, "editing"));
        assert!(flag_of(&view, "builtIn"));
        assert!(!flag_of(&view, "hasChanges"));
        assert_eq!(text_of(&view, "undoEdit"), "");
        let items = list_of(&view, "items");
        assert_eq!(items.len(), 11);
        let first = &items[2];
        assert_eq!(
            keys(first),
            [
                "col", "cols", "id", "minCols", "minRows", "row", "rows", "subject", "type",
                "unknown"
            ]
        );
        assert_eq!(text_of(first, "id"), "income");
        assert_eq!(text_of(first, "type"), "kpi.card");
        assert!(!flag_of(first, "unknown"));
        // The subject is what ComponentPreview and the panel read.
        let subject = object_of(first, "subject");
        assert_eq!(text_of(&subject, "dataset"), "income_expense_by_month");
        let preview = object_of(&subject, "preview");
        assert_eq!(text_of(&preview, "shows"), "card");
        assert_eq!(text_of(&preview, "measure"), "income");
        let library = list_of(&view, "library");
        assert_eq!(library.len(), 16);
        assert_eq!(keys(&library[0]), ["dataset", "type"]);
    }

    #[test]
    fn an_edit_shows_in_the_view_and_the_undo_words() {
        let mut layout = layout();
        layout.editor.nudge("trend", 0, 1).unwrap();
        let view = view_object(Some(&layout), true, NumberForm::DecimalComma);
        assert!(flag_of(&view, "hasChanges"));
        assert!(flag_of(&view, "canUndo"));
        assert!(!flag_of(&view, "canRedo"));
        assert!(!flag_of(&view, "builtIn"));
        assert_eq!(text_of(&view, "undoEdit"), "move");
    }

    #[test]
    fn a_heading_is_an_item_with_no_data() {
        let mut layout = layout();
        let id = grid_editor::add(&mut layout.editor, "text.heading").unwrap();
        let view = view_object(Some(&layout), true, NumberForm::DecimalComma);
        let items = list_of(&view, "items");
        let heading = items
            .iter()
            .find(|item| text_of(item, "id") == id)
            .expect("the heading");
        assert_eq!(text_of(heading, "type"), "text.heading");
        assert_eq!(
            text_of(
                &object_of(&object_of(heading, "subject"), "preview"),
                "shows"
            ),
            "heading"
        );
    }

    #[test]
    fn errors_have_a_code_a_screen_can_put_into_words() {
        let locale = app_locale();
        let book = Book::create_in_memory().unwrap();
        let mut layout = Layout::open(&book).unwrap();
        let refused = layout.editor.delete("nothing").unwrap_err();
        let shown = error_text(&book, &LayoutError::Report(refused), &locale);
        assert_eq!(shown.code, "report.grid-edit-refused");
        assert_eq!(shown.params, [("reason", "unknown-component".to_owned())]);
    }

    #[test]
    fn snapping_and_the_narrow_test_are_the_grids() {
        assert_eq!(grid::snap(149, 100), 1);
        assert_eq!(grid::snap(150, 100), 2);
        assert!(grid::is_narrow(300, 8, 18));
        assert!(!grid::is_narrow(1200, 8, 18));
    }
}
