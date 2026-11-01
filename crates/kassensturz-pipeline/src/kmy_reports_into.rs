// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KMyMoney's saved reports into a book that already exists, again and
//! again (M6d.6b, D-450, D-491; `docs/REPORT_DESIGNER_M6D.md` section 5.3).
//!
//! Only the reports of the file are used, with the mapping of
//! `kmy_reports.rs`; nothing else of the file comes into the book. As there
//! is no mapping of the file's IDs here, its accounts and categories are
//! found in the open book by name, as dashboard files find them (D-442): an
//! account by its whole name, a category by its whole path from the top,
//! ignoring capitals only, never by a similar name.
//!
//! [`plan_reports`] says what would happen to each report and changes
//! nothing; [`add_reports`] then writes what the user chose as one entry of
//! the change history, so one Undo takes the whole import back. Nothing is
//! overwritten or deleted by default.

use std::collections::{HashMap, HashSet};

use kassensturz_core::{AccountId, AccountKind, Date, ViewId, ViewKind};
use kassensturz_import::{BookAccount, BookReport};
use kassensturz_report::{Document, ViewOrigin};
use kassensturz_store::{Action, Book};

use crate::PipelineError;
use crate::kmy_reports::{
    Found, Outcome, ReportBook, ReportLine, cut, map_report, rename, unique_name, view_name,
};

/// The longest report ID a view's `origin` keeps (D-450).
const MAX_KEY_CHARS: usize = 64;

/// The open book as the mapping sees it: the file's accounts and categories,
/// each found in the book by name, or not.
struct NamedBook {
    /// What each of the file's IDs stands for in the book.
    found: HashMap<String, Found>,
    /// The file's categories in its order: key, whether it is money in,
    /// whether it is a top-level one.
    order: Vec<(String, bool, bool)>,
}

/// Names compare as the book compares them: ignoring capitals, without
/// spaces around them (the importer keeps names that way).
fn folded(name: &str) -> String {
    name.trim().to_lowercase()
}

/// The one ID in `matches`, or none for several or none: a name that fits
/// two accounts is never guessed.
fn the_one(mut matches: impl Iterator<Item = AccountId>) -> Option<AccountId> {
    match (matches.next(), matches.next()) {
        (Some(id), None) => Some(id),
        _ => None,
    }
}

impl NamedBook {
    fn new(book: &Book, file: &[BookAccount]) -> Result<Self, PipelineError> {
        let accounts: Vec<(AccountId, String, bool)> = book
            .accounts(true)?
            .into_iter()
            .map(|account| (account.id, folded(&account.name), account.closed))
            .collect();
        let categories: Vec<(AccountId, AccountKind, Vec<String>)> = book
            .categories()?
            .into_iter()
            .map(|entry| {
                let path = entry.path.iter().map(|name| folded(name)).collect();
                (entry.category.id, entry.category.kind, path)
            })
            .collect();
        let by_key: HashMap<&str, &BookAccount> = file
            .iter()
            .map(|account| (account.key.as_str(), account))
            .collect();
        // A category's names from the top down; none if its parents don't
        // lead to the top (a broken file).
        let path_of = |account: &BookAccount| -> Option<Vec<String>> {
            let mut path = vec![folded(&account.name)];
            let mut parent = account.parent.as_deref();
            while let Some(key) = parent {
                if path.len() > file.len() {
                    return None;
                }
                let above = by_key.get(key)?;
                path.push(folded(&above.name));
                parent = above.parent.as_deref();
            }
            path.reverse();
            Some(path)
        };
        let mut found = HashMap::with_capacity(file.len());
        let mut order = Vec::new();
        for account in file {
            let what = if account.kind.is_account() {
                let wanted = folded(&account.name);
                let wanted = &wanted;
                let named = |closed: bool| {
                    accounts
                        .iter()
                        .filter(move |(_, name, is_closed)| *is_closed == closed && name == wanted)
                        .map(|(id, ..)| *id)
                };
                let several = named(false).count() > 1;
                match the_one(named(false)).or_else(|| {
                    // Open accounts first, then closed ones, as D-442.
                    (!several).then(|| the_one(named(true))).flatten()
                }) {
                    Some(id) => Found::Account(id),
                    None => Found::AccountNotFound,
                }
            } else if account.kind.is_category() {
                order.push((
                    account.key.clone(),
                    account.kind == AccountKind::Income,
                    account.parent.is_none(),
                ));
                let path = path_of(account);
                let id = path.and_then(|path| {
                    the_one(
                        categories
                            .iter()
                            .filter(|(_, kind, names)| *kind == account.kind && *names == path)
                            .map(|(id, ..)| *id),
                    )
                });
                id.map_or(Found::CategoryNotFound, Found::Category)
            } else {
                Found::Nothing
            };
            found.insert(account.key.clone(), what);
        }
        Ok(Self { found, order })
    }

    fn of_kind(&self, income: bool, top_only: bool) -> impl Iterator<Item = Found> + '_ {
        self.order
            .iter()
            .filter(move |(_, is_income, top)| *is_income == income && (*top || !top_only))
            .map(|(key, ..)| self.find(key))
    }
}

impl ReportBook for NamedBook {
    fn find(&self, key: &str) -> Found {
        self.found.get(key).copied().unwrap_or(Found::Nothing)
    }

    fn categories(&self, income: bool, top_only: bool) -> Vec<AccountId> {
        self.of_kind(income, top_only)
            .filter_map(|found| match found {
                Found::Category(id) => Some(id),
                _ => None,
            })
            .collect()
    }

    fn lacks_categories(&self, income: bool, top_only: bool) -> bool {
        self.of_kind(income, top_only)
            .any(|found| found == Found::CategoryNotFound)
    }
}

/// What would happen to a report of the file (section 5.3's table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportState {
    /// Not imported before: added as a new view unless the user leaves it
    /// out.
    New,
    /// Imported before, and its view deleted since, as the change history
    /// still shows: added again unless the user leaves it out.
    DeletedEarlier,
    /// Imported before, and mapping it now gives the view that is there:
    /// nothing to do.
    AlreadyHere,
    /// Imported before, and the view is different now (changed in KMyMoney
    /// or in KassenSturz since): the view stays as the user has it unless
    /// they choose *Replace with KMyMoney's version*.
    Differs,
    /// The report can't be a saved view; its line says why. A view made
    /// from it earlier stays.
    NotKept,
    /// Another report of the file has the same ID, so neither is taken
    /// (a broken file).
    SameId,
}

impl ReportState {
    /// The keyword the preview words it from (D-034, D-450).
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::DeletedEarlier => "deleted-earlier",
            Self::AlreadyHere => "already-here",
            Self::Differs => "kept-yours",
            Self::NotKept => "not-kept",
            Self::SameId => "same-id",
        }
    }

    /// Whether the user has a choice for it: adding a new one, or replacing
    /// the view with KMyMoney's version.
    pub const fn can_choose(self) -> bool {
        matches!(self, Self::New | Self::DeletedEarlier | Self::Differs)
    }

    /// What is chosen until the user says otherwise: a report is added, a
    /// view is never replaced.
    pub const fn chosen_by_default(self) -> bool {
        matches!(self, Self::New | Self::DeletedEarlier)
    }
}

/// One report of the file and what would happen to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedReport {
    /// What the mapping makes of it now, with the name the view would have
    /// (or has, for one imported before).
    pub line: ReportLine,
    /// What would happen.
    pub state: ReportState,
    /// The view made from it earlier, if it is in the book.
    pub view: Option<ViewId>,
    /// The view that adding or replacing writes.
    document: Option<Document>,
}

/// What bringing the reports of a file into the book would do; nothing is
/// written yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportsPlan {
    /// Each report of the file, in its order.
    pub reports: Vec<PlannedReport>,
    /// The names of views made from a report that the file no longer has,
    /// in the order of the list: they stay.
    pub no_longer_in_file: Vec<String>,
}

/// What [`add_reports`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AddedReports {
    /// How many views it added.
    pub added: usize,
    /// How many views it replaced with KMyMoney's version.
    pub replaced: usize,
}

/// Whether two views are the same, read the way the book will read them.
fn same(made: &Document, stored: &Document) -> bool {
    Document::read(&made.to_json()).is_ok_and(|made| made == *stored)
}

/// Says what bringing `reports` of a KMyMoney file, whose accounts and
/// categories are `accounts`, into `book` would do on the import day
/// `today` (section 5.3). Nothing in the book changes.
///
/// A report imported before is recognised by its view's `origin`. Whether
/// its view is still what the report gives is judged on the day it was
/// imported, so a relative period that became days then doesn't count as a
/// change (D-491); replacing it maps it on `today`.
pub fn plan_reports(
    book: &Book,
    accounts: &[BookAccount],
    reports: &[BookReport],
    today: Date,
) -> Result<ReportsPlan, PipelineError> {
    let named = NamedBook::new(book, accounts)?;
    let views = book.views(Some(ViewKind::View))?;
    let mut taken: HashSet<String> = views.iter().map(|view| view.name.to_lowercase()).collect();
    // The views made from a report, by its ID; the first one if a broken
    // book had two.
    let mut imported: Vec<(String, ViewId, String, Document)> = Vec::new();
    for view in &views {
        if let Ok(document) = Document::read(&view.document)
            && let Some(origin) = &document.origin
            && origin.from == ViewOrigin::KMYMONEY
            && !imported.iter().any(|(key, ..)| *key == origin.report)
        {
            imported.push((origin.report.clone(), view.id, view.name.clone(), document));
        }
    }
    let deleted: HashSet<String> = book
        .removed_views(ViewKind::View)?
        .iter()
        .filter_map(|text| Document::read(text).ok()?.origin)
        .filter(|origin| origin.from == ViewOrigin::KMYMONEY)
        .map(|origin| origin.report)
        .collect();
    let keys: Vec<String> = reports
        .iter()
        .map(|report| cut(&report.key, MAX_KEY_CHARS))
        .collect();
    let mut times: HashMap<&str, usize> = HashMap::new();
    for key in &keys {
        *times.entry(key.as_str()).or_default() += 1;
    }

    let mut planned = Vec::with_capacity(reports.len());
    for (report, key) in reports.iter().zip(&keys) {
        if times[key.as_str()] > 1 {
            planned.push(PlannedReport {
                line: ReportLine {
                    key: report.key.clone(),
                    name: view_name(report),
                    outcome: Outcome::NotKept,
                    reasons: vec!["same-id"],
                    filters: Vec::new(),
                    period: None,
                },
                state: ReportState::SameId,
                view: None,
                document: None,
            });
            continue;
        }
        let now = map_report(report, &named, today);
        let earlier = imported.iter().find(|(of, ..)| of == key);
        let Some(mut document) = now.document else {
            planned.push(PlannedReport {
                line: now.line,
                state: ReportState::NotKept,
                view: earlier.map(|(_, id, ..)| *id),
                document: None,
            });
            continue;
        };
        let mut line = now.line;
        let plan = match earlier {
            Some((_, id, name, stored)) => {
                rename(&mut document, name);
                line.name.clone_from(name);
                let then = stored
                    .origin
                    .as_ref()
                    .map(|origin| map_report(report, &named, origin.imported));
                let unchanged = then.and_then(|then| then.document).is_some_and(|mut then| {
                    rename(&mut then, name);
                    then.origin.clone_from(&stored.origin);
                    same(&then, stored)
                });
                PlannedReport {
                    line,
                    state: if unchanged {
                        ReportState::AlreadyHere
                    } else {
                        ReportState::Differs
                    },
                    view: Some(*id),
                    document: (!unchanged).then_some(document),
                }
            }
            None => {
                let name = unique_name(&document.name, &mut taken);
                rename(&mut document, &name);
                line.name = name;
                PlannedReport {
                    line,
                    state: if deleted.contains(key) {
                        ReportState::DeletedEarlier
                    } else {
                        ReportState::New
                    },
                    view: None,
                    document: Some(document),
                }
            }
        };
        planned.push(plan);
    }
    let no_longer_in_file = imported
        .iter()
        .filter(|(key, ..)| !keys.contains(key))
        .map(|(_, _, name, _)| name.clone())
        .collect();
    Ok(ReportsPlan {
        reports: planned,
        no_longer_in_file,
    })
}

/// Writes what the user chose of `plan` into `book`, as one entry of the
/// change history (D-450): `chosen[i]` adds the `i`-th report or replaces
/// its view with KMyMoney's version, as [`ReportState::can_choose`] allows;
/// a report it doesn't name keeps [`ReportState::chosen_by_default`].
/// Replacing keeps the view's ID, name and place. A view whose name another
/// took since the plan gets *(2)*. Nothing is deleted.
pub fn add_reports(
    book: &mut Book,
    plan: &ReportsPlan,
    chosen: &[bool],
) -> Result<AddedReports, PipelineError> {
    book.as_one_action(Action::AddView, |book| {
        let mut taken: HashSet<String> = book
            .views(Some(ViewKind::View))?
            .iter()
            .map(|view| view.name.to_lowercase())
            .collect();
        let mut done = AddedReports::default();
        for (index, report) in plan.reports.iter().enumerate() {
            let wanted = chosen
                .get(index)
                .copied()
                .unwrap_or(report.state.chosen_by_default());
            let Some(document) = &report.document else {
                continue;
            };
            if !wanted || !report.state.can_choose() {
                continue;
            }
            match (report.state, report.view) {
                (ReportState::Differs, Some(id)) => {
                    book.edit_view(id, &document.to_json())?;
                    done.replaced += 1;
                }
                (ReportState::New | ReportState::DeletedEarlier, None) => {
                    let mut document = document.clone();
                    let name = unique_name(&document.name, &mut taken);
                    rename(&mut document, &name);
                    book.add_view(&document.to_json())?;
                    done.added += 1;
                }
                _ => {}
            }
        }
        Ok(done)
    })
}
