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
//! Since M6d.1 the object shows one of the book's dashboards (D-436):
//! `view` also holds `dashboards` (each with `id`, `name`, `isDefault` and
//! `shown`), `name` of the one shown, `stored`, `isDefault` and `instead`,
//! and the dashboard menu's calls each make one change in the history.
//!
//! The calls that edit return an empty object (or `id` of a new component)
//! when they worked, or an object with the error's `code` and parameters. A
//! refused edit changes nothing. Nothing is written to the book until
//! `done`, which is one entry of the change history (D-288).

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_core::Report;
use kassensturz_core::{ViewId, ViewKind};
use kassensturz_report::{
    Filters, NumberForm, OVERVIEW_NAME, Origin, ReportError, grid, templates,
};
use kassensturz_store::Book;

use crate::dashboards::{self, Dashboard};
use crate::errors::{ErrorText, report_text};
use crate::formulas;
use crate::grid_editor::{self, Layout, LayoutError};
use crate::locale::QtLocale;
use crate::number_format::{UserLocale, app_locale};
use crate::properties_model::view_object as component_view;
use crate::scope::Scope;
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

        /// Adds a component of this type that shows `dataset` (one the type
        /// can show, e.g. `net_worth_over_time` for `chart.line`, M6c.2).
        /// Gives its `id`, or the error.
        #[qinvokable]
        #[cxx_name = "addWith"]
        fn add_with(self: Pin<&mut DashboardLayout>, kind: &QString, dataset: &QString)
        -> QVariant;

        /// Adds a copy of a saved view's component (what `SavedViews.open`
        /// gives as `component` and `period`). Gives its `id`, or the error.
        #[qinvokable]
        #[cxx_name = "addView"]
        fn add_view(
            self: Pin<&mut DashboardLayout>,
            component: &QString,
            period: &QString,
        ) -> QVariant;

        /// Keeps the components as they are now in the book as a new
        /// dashboard with this name, at the end of the list (*Save as a new
        /// dashboard…*, D-330's behaviour); the session goes on. Gives the
        /// error, or an empty object.
        #[qinvokable]
        #[cxx_name = "saveLayoutAs"]
        fn save_layout_as(self: Pin<&mut DashboardLayout>, name: &QString) -> QVariant;

        /// The built-in Overview's name in the user's language: what the
        /// list shows for it and the name it is stored under when an action
        /// stores it (M6d.1).
        #[qinvokable]
        #[cxx_name = "setBuiltInName"]
        fn set_built_in_name(self: Pin<&mut DashboardLayout>, name: &QString);

        /// A book was opened: shows the dashboard it opens at (D-448).
        #[qinvokable]
        fn reopen(self: Pin<&mut DashboardLayout>);

        /// Shows the dashboard with this `id` from `view.dashboards` (empty
        /// for the built-in Overview). Ends any session.
        #[qinvokable]
        #[cxx_name = "showDashboard"]
        fn show_dashboard(self: Pin<&mut DashboardLayout>, id: &QString);

        /// Makes a new empty dashboard with this name at the end of the list
        /// and shows it, being arranged (*New dashboard…*). Gives the error,
        /// or an empty object.
        #[qinvokable]
        #[cxx_name = "newDashboard"]
        fn new_dashboard(self: Pin<&mut DashboardLayout>, name: &QString) -> QVariant;

        /// Gives the dashboard shown this name (*Rename…*). Gives the error,
        /// or an empty object.
        #[qinvokable]
        #[cxx_name = "renameDashboard"]
        fn rename_dashboard(self: Pin<&mut DashboardLayout>, name: &QString) -> QVariant;

        /// Copies the dashboard shown right after it and shows the copy
        /// (*Duplicate*): named `first`, or `numbered` with a number where it
        /// holds the character U+0001 if `first` is taken. Gives the error,
        /// or an empty object.
        #[qinvokable]
        #[cxx_name = "duplicateDashboard"]
        fn duplicate_dashboard(
            self: Pin<&mut DashboardLayout>,
            first: &QString,
            numbered: &QString,
        ) -> QVariant;

        /// Marks the dashboard shown as the one the book opens at (*Make
        /// this the default*, D-448). Gives the error, or an empty object.
        #[qinvokable]
        #[cxx_name = "makeDefault"]
        fn make_default(self: Pin<&mut DashboardLayout>) -> QVariant;

        /// Moves the dashboard shown `by` places in the list (-1 earlier, 1
        /// later). Gives the error, or an empty object.
        #[qinvokable]
        #[cxx_name = "moveDashboard"]
        fn move_dashboard(self: Pin<&mut DashboardLayout>, by: i32) -> QVariant;

        /// Puts the template's components and filters back on the dashboard
        /// shown (*Back to the template…*, D-439). Gives the error, or an
        /// empty object.
        #[qinvokable]
        #[cxx_name = "backToTemplate"]
        fn back_to_template(self: Pin<&mut DashboardLayout>) -> QVariant;

        /// Deletes the dashboard shown and shows the one the book opens at
        /// (*Delete…*). Gives the error, or an empty object.
        #[qinvokable]
        #[cxx_name = "deleteDashboard"]
        fn delete_dashboard(self: Pin<&mut DashboardLayout>) -> QVariant;

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

        /// Puts the components of the template the dashboard shown was made
        /// from back while it is arranged, as an edit Undo takes back
        /// (D-439); its filters stay (D-483).
        #[qinvokable]
        fn reset(self: Pin<&mut DashboardLayout>) -> QVariant;

        /// Sets one thing the filter bar narrows by (M6c.7, D-351):
        /// `dimension` is `period` (`value` is a fixed period, a typed range
        /// or empty for the page's own), `accounts` or `categories`
        /// (`value` is the IDs separated by commas, empty for all). While
        /// the layout is only looked at, the choice is for now: it changes
        /// what the page shows and nothing is written. While it is
        /// arranged, it becomes part of the layout and is saved with Done.
        /// Gives an empty map or the error.
        #[qinvokable]
        #[cxx_name = "setFilter"]
        fn set_filter(
            self: Pin<&mut DashboardLayout>,
            dimension: &QString,
            value: &QString,
        ) -> QVariant;

        /// Goes back to the filters the layout itself carries, taking back
        /// what was chosen for now. While the layout is arranged, takes the
        /// layout's filters out. Gives an empty map or the error.
        #[qinvokable]
        #[cxx_name = "resetFilters"]
        fn reset_filters(self: Pin<&mut DashboardLayout>) -> QVariant;

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
pub struct DashboardLayoutRust {
    view: QVariant,
    layout: Option<Layout>,
    editing: bool,
    /// The dashboards, as last read (M6d.1).
    dashboards: Vec<Dashboard>,
    /// The stored dashboard shown; `None` for the one the book opens at.
    /// Kept while the user goes to other pages (D-448).
    shown: Option<ViewId>,
    /// The book's dashboards as last read, to follow what Undo changed;
    /// `None` until the book is read the first time.
    seen: Option<Vec<Seen>>,
    /// The built-in Overview's name in the user's language.
    built_in_name: String,
    /// What the filter bar holds for now (D-351): a choice made while the
    /// layout is only looked at, never written to the book. `None` means
    /// the layout's own filters.
    bar: Option<Filters>,
}

impl Default for DashboardLayoutRust {
    fn default() -> Self {
        Self {
            view: QVariant::default(),
            layout: None,
            editing: false,
            dashboards: Vec::new(),
            shown: None,
            seen: None,
            built_in_name: OVERVIEW_NAME.to_owned(),
            bar: None,
        }
    }
}

/// A stored dashboard as it was read: its ID and its document.
type Seen = (ViewId, String);

/// Which dashboard to show after the book changed (D-436, *Undo across
/// dashboards*): the one shown if it is still there and either it changed
/// or nothing else did; otherwise the first that changed or came back, so
/// the user sees what Undo did; otherwise the one the book opens at
/// (`None`). Without an earlier reading, the one shown.
pub fn follow(shown: Option<ViewId>, before: Option<&[Seen]>, now: &[Seen]) -> Option<ViewId> {
    let Some(before) = before else {
        return shown;
    };
    let changed: Vec<ViewId> = now
        .iter()
        .filter(|(id, document)| {
            before
                .iter()
                .find(|(old, _)| old == id)
                .is_none_or(|(_, old)| old != document)
        })
        .map(|(id, _)| *id)
        .collect();
    match shown {
        Some(id) if now.iter().any(|(other, _)| *other == id) => {
            if changed.is_empty() || changed.contains(&id) {
                Some(id)
            } else {
                changed.first().copied()
            }
        }
        _ => changed.first().copied(),
    }
}

fn number(value: u32) -> QVariant {
    QVariant::from(&i32::try_from(value).unwrap_or(i32::MAX))
}

fn count(value: usize) -> QVariant {
    QVariant::from(&i32::try_from(value).unwrap_or(i32::MAX))
}

/// What a component asks for, for QML: `period` is the word of the period
/// (`page` for the page's own) and `scope` the text `setScope` takes.
fn asked_object(row: &grid_editor::Row) -> QVariant {
    row.asked.as_ref().map_or_else(
        || {
            map([
                ("period", text("page")),
                ("scope", text("")),
                ("shown", QVariant::from(&false)),
                ("fromBar", QVariant::from(&false)),
            ])
        },
        |asked| {
            // Whether any dimension comes from the filter bar.
            let from_bar = asked.period_origin == Origin::FilterBar
                || [&asked.accounts, &asked.categories]
                    .into_iter()
                    .flatten()
                    .any(|chosen| chosen.origin == Origin::FilterBar);
            map([
                ("fromBar", QVariant::from(&from_bar)),
                ("period", text(&asked.period)),
                ("scope", text(&Scope::from_asked(asked).to_text())),
                ("shown", QVariant::from(&true)),
            ])
        },
    )
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
        ("asked", asked_object(row)),
        (
            "subject",
            row.component.as_ref().map_or_else(
                || map([]),
                |component| component_view(Some(component), None, form),
            ),
        ),
    ])
}

/// The dashboards as QML gets them: objects with `id` (empty for the
/// built-in Overview), `name`, `isDefault` (the book opens at it, D-448) and
/// `shown`.
fn dashboards_object(dashboards: &[Dashboard], shown: Option<ViewId>) -> QVariant {
    list(dashboards.iter().map(|dashboard| {
        map([
            (
                "id",
                text(&dashboard.id.map(|id| id.to_string()).unwrap_or_default()),
            ),
            ("name", text(&dashboard.name)),
            ("isDefault", QVariant::from(&dashboard.is_default)),
            ("shown", QVariant::from(&(dashboard.id == shown))),
        ])
    }))
}

/// What QML gets for the layout; empty when no book is open. Besides the
/// grid: `dashboards` (see [`dashboards_object`]), `name` of the one shown,
/// `stored` (whether it is in the book, so it can be moved and deleted),
/// `isDefault`, `instead`, the name of the one the book would open at if
/// it were deleted, and for *Back to the template* (D-439): `template`, its
/// word (empty when it has none), `atTemplate` (it shows the template's
/// components and, unless it is arranged, filters), `widgets` and
/// `templateWidgets`, how many components it and the template have, and
/// `templateFilters`, whether the template's filters differ from its own.
fn view_object(
    layout: Option<&Layout>,
    editing: bool,
    form: NumberForm,
    bar: Option<&Filters>,
    dashboards: &[Dashboard],
    instead: &str,
) -> QVariant {
    let Some(layout) = layout else {
        return map([]);
    };
    let shown_default = dashboards
        .iter()
        .find(|dashboard| dashboard.id == layout.stored)
        .is_some_and(|dashboard| dashboard.is_default);
    let (columns, max_rows) = grid_editor::size();
    let editor = &layout.editor;
    let own = editor.filters();
    // While arranged, the bar is the layout's own filters.
    let bar = if editing { None } else { bar }.unwrap_or(&own);
    let word = dashboards::template_word(dashboards, layout.stored);
    let template = word.and_then(|word| templates::document(word).ok());
    let at_template = !layout.damaged
        && word.is_some_and(|word| {
            if editing {
                dashboards::has_template_components(editor.document(), word)
            } else {
                dashboards::is_template(editor.document(), word)
            }
        });
    map([
        ("dashboards", dashboards_object(dashboards, layout.stored)),
        ("name", text(&layout.name)),
        ("stored", QVariant::from(&layout.stored.is_some())),
        ("isDefault", QVariant::from(&shown_default)),
        ("instead", text(instead)),
        ("template", text(word.unwrap_or(""))),
        ("atTemplate", QVariant::from(&at_template)),
        ("widgets", count(editor.document().entries.len())),
        (
            "templateWidgets",
            count(
                template
                    .as_ref()
                    .map_or(0, |template| template.entries.len()),
            ),
        ),
        (
            "templateFilters",
            QVariant::from(
                &template
                    .as_ref()
                    .is_some_and(|template| template.filters() != own),
            ),
        ),
        ("filters", filters_object(bar, bar != &own)),
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
                grid_editor::rows(editor, bar)
                    .iter()
                    .map(|row| item_object(row, form)),
            ),
        ),
    ])
}

/// The filter bar as QML gets it: `period` (empty for the page's own),
/// `accounts` and `categories` as lists of IDs, and whether it holds a
/// choice made for now, which *Reset filters* takes back.
fn filters_object(bar: &Filters, for_now: bool) -> QVariant {
    map([
        ("period", text(bar.period.as_deref().unwrap_or(""))),
        ("accounts", list(bar.accounts.iter().map(|id| text(id)))),
        ("categories", list(bar.categories.iter().map(|id| text(id)))),
        ("forNow", QVariant::from(&for_now)),
    ])
}

/// The error as QML gets it.
fn error_text(book: &Book, error: &LayoutError, locale: &UserLocale<QtLocale>) -> ErrorText {
    match error {
        LayoutError::Store(error) => report_text(book, &error.report(), locale),
        LayoutError::Report(error) => report_text(book, &error.report(), locale),
        LayoutError::NoSuchLayout => ErrorText::plain("input.no-such-entry"),
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
        let rust = self.rust();
        let instead = rust
            .layout
            .as_ref()
            .and_then(|layout| layout.stored)
            .and_then(|id| {
                read_book(|book| dashboards::default_without(book, id, &rust.built_in_name)).ok()
            })
            .unwrap_or_default();
        let view = view_object(
            rust.layout.as_ref(),
            rust.editing,
            form,
            rust.bar.as_ref(),
            &rust.dashboards,
            &instead,
        );
        self.as_mut().rust_mut().view = view;
        self.as_mut().view_changed();
    }

    /// Reads the dashboards and the one to show from the book and shows it,
    /// ending any session. After a change it follows what changed
    /// ([`follow`]).
    fn load(mut self: Pin<&mut Self>, editing: bool) {
        let (shown, built_in_name) = (self.rust().shown, self.rust().built_in_name.clone());
        let before = self.rust().seen.clone();
        let read = read_book(|book| {
            let now: Vec<Seen> = book
                .views(Some(ViewKind::Dashboard))?
                .into_iter()
                .map(|view| (view.id, view.document))
                .collect();
            let wanted = follow(shown, before.as_deref(), &now);
            let layout = Layout::open(book, wanted, &built_in_name)?;
            let list = dashboards::list(book, &built_in_name)?;
            Ok((layout, list, now))
        })
        .ok();
        let mut this = self.as_mut().rust_mut();
        match read {
            Some((layout, list, now)) => {
                if this.shown != layout.stored {
                    // Another dashboard: what was chosen for now was for
                    // the one before.
                    this.bar = None;
                }
                this.shown = layout.stored;
                this.layout = Some(layout);
                this.dashboards = list;
                this.seen = Some(now);
            }
            None => {
                this.layout = None;
                this.dashboards = Vec::new();
            }
        }
        this.editing = editing && this.layout.is_some();
        self.show();
    }

    /// Does a dashboard action on the book; on success shows `then` (or
    /// the dashboard shown, for `None`), tells the screens the book changed
    /// and gives an empty object, otherwise the error.
    fn dashboard_action(
        mut self: Pin<&mut Self>,
        action: impl FnOnce(&mut Book, Option<ViewId>, &str) -> Result<Option<ViewId>, LayoutError>,
        editing: bool,
    ) -> QVariant {
        if self.rust().editing {
            // The menu is not offered while a dashboard is arranged.
            return map([]);
        }
        let shown = self.rust().layout.as_ref().and_then(|layout| layout.stored);
        let built_in_name = self.rust().built_in_name.clone();
        match change_book(|book| action(book, shown, &built_in_name)) {
            Ok(then) => {
                {
                    let mut this = self.as_mut().rust_mut();
                    if let Some(id) = then {
                        this.shown = Some(id);
                        this.bar = None;
                    }
                    // The action's own change is not something to follow.
                    this.seen = None;
                }
                self.as_mut().load(editing);
                self.as_mut().changed();
                map([])
            }
            Err(error) => error_object(&error),
        }
    }

    fn set_built_in_name(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        let name = name.trim();
        if name.is_empty() || name == self.rust().built_in_name {
            return;
        }
        name.clone_into(&mut self.as_mut().rust_mut().built_in_name);
        if !self.rust().editing {
            self.load(false);
        }
    }

    fn reopen(mut self: Pin<&mut Self>) {
        {
            let mut this = self.as_mut().rust_mut();
            this.shown = None;
            this.seen = None;
            this.bar = None;
        }
        self.load(false);
    }

    fn show_dashboard(mut self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        let wanted = if id.is_empty() {
            None
        } else {
            read_book(|book| dashboards::find(book, &id)).ok()
        };
        {
            let mut this = self.as_mut().rust_mut();
            if this.shown != wanted {
                this.bar = None;
            }
            this.shown = wanted;
            this.seen = None;
        }
        self.load(false);
    }

    fn new_dashboard(self: Pin<&mut Self>, name: &QString) -> QVariant {
        let name = name.to_string();
        // An empty dashboard opens being arranged, so the user can add to
        // it at once (section 2, `empty`).
        self.dashboard_action(
            |book, _, built_in| {
                dashboards::add(book, kassensturz_report::templates::EMPTY, &name, built_in)
                    .map(Some)
            },
            true,
        )
    }

    fn rename_dashboard(self: Pin<&mut Self>, name: &QString) -> QVariant {
        let name = name.to_string();
        self.dashboard_action(
            |book, shown, built_in| dashboards::rename(book, shown, &name, built_in).map(Some),
            false,
        )
    }

    fn duplicate_dashboard(self: Pin<&mut Self>, first: &QString, numbered: &QString) -> QVariant {
        let (first, numbered) = (first.to_string(), numbered.to_string());
        self.dashboard_action(
            |book, shown, built_in| {
                dashboards::duplicate(book, shown, &first, &numbered, built_in).map(Some)
            },
            false,
        )
    }

    fn make_default(self: Pin<&mut Self>) -> QVariant {
        self.dashboard_action(
            |book, shown, built_in| dashboards::make_default(book, shown, built_in).map(Some),
            false,
        )
    }

    fn move_dashboard(self: Pin<&mut Self>, by: i32) -> QVariant {
        self.dashboard_action(
            |book, shown, _| {
                let id = shown.ok_or(LayoutError::NoSuchLayout)?;
                dashboards::move_by(book, id, by)?;
                Ok(Some(id))
            },
            false,
        )
    }

    fn back_to_template(self: Pin<&mut Self>) -> QVariant {
        // The built-in Overview is shown if the last one was removed.
        self.dashboard_action(dashboards::back_to_template, false)
    }

    fn delete_dashboard(self: Pin<&mut Self>) -> QVariant {
        // Then the one the book opens at is shown.
        self.dashboard_action(
            |book, shown, _| {
                dashboards::delete(book, shown.ok_or(LayoutError::NoSuchLayout)?)?;
                Ok(None)
            },
            false,
        )
    }

    fn refresh(self: Pin<&mut Self>) {
        if !self.rust().editing {
            self.load(false);
        }
    }

    fn edit(mut self: Pin<&mut Self>) {
        // Arranging starts from the layout's own filters; what was chosen
        // for now is let go.
        self.as_mut().rust_mut().bar = None;
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
                self.as_mut().rust_mut().bar = None;
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

    /// An "add to page" edit that says in the debug log how it went, so a
    /// click that does nothing is visible there (M6-B9): the code if the grid
    /// refused, `input.no-such-entry` if no layout was loaded to add to.
    fn edited_add(
        self: Pin<&mut Self>,
        detail: Option<&'static str>,
        edit: impl FnOnce(&mut Layout) -> Result<String, LayoutError>,
    ) -> QVariant {
        let mut ran = false;
        let answer = self.edited(
            |layout| {
                ran = true;
                let result = edit(layout);
                let code = result.as_ref().err().map(|error| plain_error(error).code);
                crate::note!(LayoutWidgetAdded, detail, code);
                result
            },
            |id| map([("id", text(&id))]),
        );
        if !ran {
            crate::note!(LayoutWidgetAdded, detail, Some("input.no-such-entry"));
        }
        answer
    }

    fn add(self: Pin<&mut Self>, kind: &QString) -> QVariant {
        let kind = kind.to_string();
        let detail = grid_editor::widget_detail(&kind);
        self.edited_add(detail, |layout| grid_editor::add(&mut layout.editor, &kind))
    }

    fn add_with(self: Pin<&mut Self>, kind: &QString, dataset: &QString) -> QVariant {
        let (kind, dataset) = (kind.to_string(), dataset.to_string());
        let detail = grid_editor::widget_detail(&kind);
        self.edited_add(detail, |layout| {
            grid_editor::add_with(&mut layout.editor, &kind, &dataset)
        })
    }

    fn add_view(self: Pin<&mut Self>, component: &QString, period: &QString) -> QVariant {
        let (component, period) = (component.to_string(), period.to_string());
        self.edited_add(Some("saved_view"), |layout| {
            grid_editor::add_view(&mut layout.editor, &component, &period)
        })
    }

    fn save_layout_as(mut self: Pin<&mut Self>, name: &QString) -> QVariant {
        let name = name.to_string();
        let built_in_name = self.rust().built_in_name.clone();
        let Some(layout) = self.rust().layout.as_ref() else {
            return map([]);
        };
        match change_book(|book| {
            dashboards::save_as(book, &layout.editor, &name, &built_in_name)?;
            let list = dashboards::list(book, &built_in_name)?;
            Ok((
                book.views(Some(ViewKind::Dashboard))?
                    .first()
                    .map(|view| view.id),
                list,
            ))
        }) {
            Ok((first, list)) => {
                let mut this = self.as_mut().rust_mut();
                if let Some(layout) = this.layout.as_mut()
                    && layout.stored.is_none()
                {
                    // The built-in Overview was stored first: the session
                    // goes on with the stored copy.
                    layout.stored = first;
                    this.shown = first;
                }
                this.dashboards = list;
                this.seen = None;
                self.as_mut().show();
                self.as_mut().changed();
                map([])
            }
            Err(error) => error_object(&error),
        }
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
        let word = dashboards::template_word(
            &self.rust().dashboards,
            self.rust().layout.as_ref().and_then(|layout| layout.stored),
        )
        .map(str::to_owned);
        self.edited(
            |layout| {
                let word = word.ok_or(LayoutError::Report(ReportError::DocumentInvalid {
                    reason: "bad-choice",
                    path: "template".to_owned(),
                }))?;
                Ok(layout.editor.reset(&templates::document(&word)?.entries)?)
            },
            |_| map([]),
        )
    }

    fn set_filter(mut self: Pin<&mut Self>, dimension: &QString, value: &QString) -> QVariant {
        let (dimension, value) = (dimension.to_string(), value.to_string());
        let editing = self.rust().editing;
        let Some(layout) = self.rust().layout.as_ref() else {
            return map([]);
        };
        let own = layout.editor.filters();
        let mut next = if editing {
            own.clone()
        } else {
            self.rust().bar.clone().unwrap_or_else(|| own.clone())
        };
        let ids = |text: &str| -> Vec<String> {
            text.split(',')
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .collect()
        };
        match dimension.as_str() {
            "period" => next.period = (!value.is_empty()).then_some(value),
            "accounts" => next.accounts = ids(&value),
            "categories" => next.categories = ids(&value),
            _ => return map([]),
        }
        if editing {
            return self.edited(
                |layout| grid_editor::set_filters(&mut layout.editor, &next),
                |_| map([]),
            );
        }
        // For now only: checked as a layout's filters are, but kept apart.
        if let Err(error) = grid_editor::set_filters(&mut layout.editor.clone(), &next) {
            return error_object(&plain_error(&error));
        }
        self.as_mut().rust_mut().bar = (next != own).then_some(next);
        self.show();
        map([])
    }

    fn reset_filters(mut self: Pin<&mut Self>) -> QVariant {
        if self.rust().editing {
            return self.edited(
                |layout| grid_editor::set_filters(&mut layout.editor, &Filters::default()),
                |_| map([]),
            );
        }
        self.as_mut().rust_mut().bar = None;
        self.show();
        map([])
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
    use crate::variant::read_back::{flag_of, keys, list_of, number_of, object_of, text_of};
    use kassensturz_report::Document;

    fn layout() -> Layout {
        Layout::open(&Book::create_in_memory().unwrap(), None, OVERVIEW_NAME).unwrap()
    }

    #[test]
    fn the_layout_reads_back_the_way_qml_reads_it() {
        let layout = layout();
        let view = view_object(Some(&layout), true, NumberForm::DecimalComma, None, &[], "");
        assert_eq!(
            keys(&view),
            [
                "atTemplate",
                "builtIn",
                "canRedo",
                "canUndo",
                "columns",
                "damaged",
                "dashboards",
                "editing",
                "filters",
                "hasChanges",
                "instead",
                "isDefault",
                "items",
                "library",
                "maxRows",
                "name",
                "redoEdit",
                "rows",
                "stored",
                "template",
                "templateFilters",
                "templateWidgets",
                "undoEdit",
                "widgets"
            ]
        );
        assert!(flag_of(&view, "editing"));
        assert!(flag_of(&view, "builtIn"));
        assert!(!flag_of(&view, "hasChanges"));
        assert_eq!(text_of(&view, "undoEdit"), "");
        let items = list_of(&view, "items");
        assert_eq!(items.len(), 12);
        let first = &items[2];
        assert_eq!(
            keys(first),
            [
                "asked", "col", "cols", "id", "minCols", "minRows", "row", "rows", "subject",
                "type", "unknown"
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
        assert_eq!(library.len(), 18);
        assert_eq!(keys(&library[0]), ["dataset", "type"]);
    }

    #[test]
    fn the_filter_bar_reads_back_and_a_choice_for_now_reaches_every_component() {
        let layout = layout();
        let own = layout.editor.filters();
        // Looked at, with nothing chosen: the layout's own (none).
        let view = view_object(
            Some(&layout),
            false,
            NumberForm::DecimalComma,
            None,
            &[],
            "",
        );
        let filters = object_of(&view, "filters");
        assert_eq!(
            keys(&filters),
            ["accounts", "categories", "forNow", "period"]
        );
        assert_eq!(text_of(&filters, "period"), "");
        assert!(!flag_of(&filters, "forNow"));
        let income = list_of(&view, "items")
            .into_iter()
            .find(|item| text_of(item, "id") == "income")
            .expect("the income card");
        let asked = object_of(&income, "asked");
        assert_eq!(keys(&asked), ["fromBar", "period", "scope", "shown"]);
        assert_eq!(text_of(&asked, "period"), "page");
        assert!(!flag_of(&asked, "fromBar"));
        // A choice for now: the bar's period and accounts reach it.
        let bar = Filters {
            period: Some("last-year".into()),
            accounts: vec!["a-1".into()],
            categories: Vec::new(),
        };
        let view = view_object(
            Some(&layout),
            false,
            NumberForm::DecimalComma,
            Some(&bar),
            &[],
            "",
        );
        let filters = object_of(&view, "filters");
        assert_eq!(text_of(&filters, "period"), "last-year");
        assert_eq!(list_of(&filters, "accounts").len(), 1);
        assert!(flag_of(&filters, "forNow"));
        let income = list_of(&view, "items")
            .into_iter()
            .find(|item| text_of(item, "id") == "income")
            .expect("the income card");
        let asked = object_of(&income, "asked");
        assert_eq!(text_of(&asked, "period"), "last-year");
        assert!(flag_of(&asked, "fromBar"));
        let scope = Scope::from_text(&text_of(&asked, "scope"));
        assert!(scope.to_text().contains("\"accounts\":[\"a-1\"]"));
        // Nothing was written into the layout.
        assert_eq!(layout.editor.filters(), own);
        // While arranged, the bar is the layout's own, and a choice is kept
        // with the layout and counts as a change.
        let mut layout = layout;
        layout.editor.set_filters(&bar).unwrap();
        let view = view_object(Some(&layout), true, NumberForm::DecimalComma, None, &[], "");
        assert_eq!(text_of(&object_of(&view, "filters"), "period"), "last-year");
        assert!(!flag_of(&object_of(&view, "filters"), "forNow"));
        assert!(flag_of(&view, "hasChanges"));
        assert!(!flag_of(&view, "builtIn"));
    }

    #[test]
    fn the_dashboards_read_back_with_the_one_shown_and_the_default_from_a_real_book() {
        let mut book = Book::create_in_memory().unwrap();
        let holidays = dashboards::add(&mut book, "empty", "Holidays", "Übersicht").unwrap();
        let list = dashboards::list(&book, "Übersicht").unwrap();
        let layout = Layout::open(&book, Some(holidays), "Übersicht").unwrap();
        let instead = dashboards::default_without(&book, holidays, "Übersicht").unwrap();
        let view = view_object(
            Some(&layout),
            false,
            NumberForm::DecimalComma,
            None,
            &list,
            &instead,
        );
        assert_eq!(text_of(&view, "name"), "Holidays");
        assert!(flag_of(&view, "stored"));
        assert!(!flag_of(&view, "isDefault"));
        assert_eq!(text_of(&view, "instead"), "Übersicht");
        assert!(list_of(&view, "items").is_empty());
        let rows = list_of(&view, "dashboards");
        assert_eq!(rows.len(), 2);
        assert_eq!(keys(&rows[0]), ["id", "isDefault", "name", "shown"]);
        assert_eq!(text_of(&rows[0], "name"), "Übersicht");
        assert!(flag_of(&rows[0], "isDefault"));
        assert!(!flag_of(&rows[0], "shown"));
        assert_eq!(text_of(&rows[1], "id"), holidays.to_string());
        assert!(flag_of(&rows[1], "shown"));
        // The built-in Overview alone has no ID and is not stored.
        let book = Book::create_in_memory().unwrap();
        let list = dashboards::list(&book, "Übersicht").unwrap();
        let layout = Layout::open(&book, None, "Übersicht").unwrap();
        let view = view_object(
            Some(&layout),
            false,
            NumberForm::DecimalComma,
            None,
            &list,
            "",
        );
        let rows = list_of(&view, "dashboards");
        assert_eq!(rows.len(), 1);
        assert_eq!(text_of(&rows[0], "id"), "");
        assert!(flag_of(&rows[0], "shown"));
        assert!(flag_of(&view, "isDefault"));
        assert!(!flag_of(&view, "stored"));
        assert_eq!(text_of(&view, "name"), "Übersicht");
    }

    #[test]
    fn back_to_the_template_reads_back_with_the_counts_the_question_names() {
        let mut book = Book::create_in_memory().unwrap();
        let empty = dashboards::add(&mut book, "empty", "Plans", "Übersicht").unwrap();
        let mut arranged = templates::document("overview").unwrap();
        arranged.name = "Home".to_owned();
        arranged.entries.pop();
        arranged.accounts = vec!["account-1".to_owned()];
        let home = book.add_view(&arranged.to_json()).unwrap();
        let read = |book: &Book, id, editing| {
            let list = dashboards::list(book, "Übersicht").unwrap();
            let layout = Layout::open(book, Some(id), "Übersicht").unwrap();
            view_object(
                Some(&layout),
                editing,
                NumberForm::DecimalComma,
                None,
                &list,
                "",
            )
        };
        let overview = Document::default_overview().unwrap().entries.len();
        let view = read(&book, home, false);
        assert_eq!(text_of(&view, "template"), "overview");
        assert!(!flag_of(&view, "atTemplate"));
        assert_eq!(
            number_of(&view, "widgets"),
            i32::try_from(overview - 1).unwrap()
        );
        assert_eq!(
            number_of(&view, "templateWidgets"),
            i32::try_from(overview).unwrap()
        );
        assert!(flag_of(&view, "templateFilters"));
        dashboards::back_to_template(&mut book, Some(home), "Übersicht").unwrap();
        let view = read(&book, home, false);
        assert!(flag_of(&view, "atTemplate"));
        assert!(!flag_of(&view, "templateFilters"));
        assert_eq!(
            number_of(&view, "widgets"),
            i32::try_from(overview).unwrap()
        );
        // An empty one is at its template, which has no widgets.
        let view = read(&book, empty, true);
        assert_eq!(text_of(&view, "template"), "empty");
        assert!(flag_of(&view, "atTemplate"));
        assert_eq!(number_of(&view, "templateWidgets"), 0);
        // One stored before templates, not the M6c Overview, has none.
        let mut old = Document::default_overview().unwrap();
        old.name = "Month review".to_owned();
        old.template = None;
        let old = book.add_view(&old.to_json()).unwrap();
        let view = read(&book, old, false);
        assert_eq!(text_of(&view, "template"), "");
        assert!(!flag_of(&view, "atTemplate"));
    }

    #[test]
    fn after_a_change_the_page_follows_what_changed() {
        let [a, b, c] = [ViewId::generate(), ViewId::generate(), ViewId::generate()];
        let seen = |items: &[(ViewId, &str)]| -> Vec<Seen> {
            items
                .iter()
                .map(|(id, text)| (*id, (*text).to_owned()))
                .collect()
        };
        let before = seen(&[(a, "1"), (b, "1")]);
        // No earlier reading: the one asked for.
        assert_eq!(follow(Some(b), None, &before), Some(b));
        assert_eq!(follow(None, None, &before), None);
        // Nothing changed, or the one shown changed: it stays.
        assert_eq!(follow(Some(b), Some(&before), &before), Some(b));
        assert_eq!(
            follow(Some(b), Some(&before), &seen(&[(a, "2"), (b, "2")])),
            Some(b)
        );
        // Undo changed another one, or brought one back: the page goes there.
        assert_eq!(
            follow(Some(b), Some(&before), &seen(&[(a, "2"), (b, "1")])),
            Some(a)
        );
        assert_eq!(
            follow(
                Some(b),
                Some(&before),
                &seen(&[(a, "1"), (b, "1"), (c, "1")])
            ),
            Some(c)
        );
        // The one shown is gone and nothing else changed: the default.
        assert_eq!(follow(Some(b), Some(&before), &seen(&[(a, "1")])), None);
        // The built-in Overview was shown and Undo brought a stored one back.
        assert_eq!(follow(None, Some(&[]), &seen(&[(a, "1")])), Some(a));
    }

    #[test]
    fn an_edit_shows_in_the_view_and_the_undo_words() {
        let mut layout = layout();
        layout.editor.nudge("trend", 0, 1).unwrap();
        let view = view_object(Some(&layout), true, NumberForm::DecimalComma, None, &[], "");
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
        let view = view_object(Some(&layout), true, NumberForm::DecimalComma, None, &[], "");
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
        let mut layout = Layout::open(&book, None, OVERVIEW_NAME).unwrap();
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
