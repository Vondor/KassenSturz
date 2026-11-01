// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! *Add reports from a KMyMoney file…* on the Statistics page (M6d.6b,
//! D-450, D-491; `docs/REPORT_DESIGNER_M6D.md` section 5.3). The file is
//! read as untrusted input with the importer's limits, only its reports
//! and the names of its accounts and categories are used, and the preview
//! says what would happen to each report before anything is written; the
//! pipeline does the work (`kassensturz_pipeline::plan_reports`,
//! `add_reports`). `kmy_reports_import_model.rs` hands it to QML.
//!
//! The file is never given to the AI and reading it starts no network
//! access.

use std::path::Path;

use kassensturz_pipeline::{AddedReports, ReportsPlan, add_reports, plan_reports};
use kassensturz_store::Book;

use crate::book_from_file::{
    FromFileError, ReportText, note_fixed_periods, read_kmymoney, report_texts,
};
use crate::format::Locale;

/// A KMyMoney file's reports planned for the open book: nothing is written
/// yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opened {
    /// The file's name, without its folder.
    pub file_name: String,
    /// What would happen to each report.
    pub plan: ReportsPlan,
    /// Whether each report is added or its view replaced, as the user
    /// chose; the defaults first (nothing replaced).
    pub chosen: Vec<bool>,
}

/// One line of the preview, as the page shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewLine {
    /// The report's line, worded as on the book import's done page.
    pub text: ReportText,
    /// What would happen: `new`, `deleted-earlier`, `already-here`,
    /// `kept-yours`, `not-kept` or `same-id`.
    pub state: &'static str,
    /// Whether the user can tick it: adding a report, or replacing a view
    /// with KMyMoney's version.
    pub can_choose: bool,
    /// Whether it is ticked.
    pub chosen: bool,
}

/// Reads the KMyMoney file at `path` and plans its reports for `book` on the
/// day the locale gives. Nothing in the book changes.
pub fn open(book: &Book, path: &Path, locale: &impl Locale) -> Result<Opened, FromFileError> {
    let file = read_kmymoney(path)?;
    let plan = plan_reports(book, &file.accounts, &file.reports, locale.today())?;
    let chosen = plan
        .reports
        .iter()
        .map(|report| report.state.chosen_by_default())
        .collect();
    Ok(Opened {
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        plan,
        chosen,
    })
}

impl Opened {
    /// Ticks or unticks the report at `index`, if it has a choice.
    pub fn choose(&mut self, index: usize, chosen: bool) {
        if let (Some(report), Some(choice)) =
            (self.plan.reports.get(index), self.chosen.get_mut(index))
            && report.state.can_choose()
        {
            *choice = chosen;
        }
    }

    /// The preview's lines, in the file's order, dates in the user's format.
    pub fn lines(&self, locale: &impl Locale) -> Vec<PreviewLine> {
        let lines: Vec<_> = self
            .plan
            .reports
            .iter()
            .map(|report| report.line.clone())
            .collect();
        report_texts(&lines, locale)
            .into_iter()
            .zip(self.plan.reports.iter().zip(&self.chosen))
            .map(|(text, (report, &chosen))| PreviewLine {
                text,
                state: report.state.keyword(),
                can_choose: report.state.can_choose(),
                chosen,
            })
            .collect()
    }

    /// Writes what the user chose into `book`, as one entry of the change
    /// history; one Undo takes it back.
    pub fn add(&self, book: &mut Book) -> Result<AddedReports, FromFileError> {
        let added = add_reports(book, &self.plan, &self.chosen)?;
        note_fixed_periods(
            self.plan
                .reports
                .iter()
                .zip(&self.chosen)
                .filter(|(report, chosen)| **chosen && report.state.can_choose())
                .map(|(report, _)| &report.line),
        );
        Ok(added)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use kassensturz_core::ViewKind;
    use kassensturz_import::{Imported, Kmy, Limits, import};
    use kassensturz_pipeline::{RuleScope, fill_book_from_on};

    use super::*;
    use crate::register::tests::TestLocale;

    fn golden(folder: &str, name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kassensturz-import/tests/golden")
            .join(folder)
            .join(name)
    }

    /// A book made from the sample file as the app makes one, on the day the
    /// test locale gives.
    fn made_from_sample() -> Book {
        let bytes = std::fs::read(golden("kmy", "reports-anon.kmy")).expect("the sample");
        let Imported::Book(imported) = import(&Kmy, &bytes, &Limits::default()).expect("read")
        else {
            panic!("a book");
        };
        let mut book = Book::create_in_memory().expect("book");
        fill_book_from_on(
            &mut book,
            &imported,
            &HashMap::new(),
            RuleScope::default(),
            TestLocale.today(),
        )
        .expect("filled");
        book
    }

    #[test]
    fn the_preview_of_the_same_file_again_has_nothing_to_do_until_a_view_is_deleted() {
        let mut book = made_from_sample();
        let path = golden("kmy", "reports-anon.kmy");
        let opened = open(&book, &path, &TestLocale).expect("opened");
        assert_eq!(opened.file_name, "reports-anon.kmy");
        let states: Vec<_> = opened
            .lines(&TestLocale)
            .iter()
            .map(|line| (line.state, line.can_choose, line.chosen))
            .collect();
        assert_eq!(
            states[..4],
            [
                ("already-here", false, false),
                ("already-here", false, false),
                ("already-here", false, false),
                ("not-kept", false, false),
            ]
        );

        let third = book.views(Some(ViewKind::View)).expect("views")[2].id;
        book.remove_view(third).expect("removed");
        let mut opened = open(&book, &path, &TestLocale).expect("opened");
        let lines = opened.lines(&TestLocale);
        assert_eq!(lines[2].state, "deleted-earlier");
        assert!(lines[2].can_choose && lines[2].chosen);
        assert_eq!(lines[2].text.name, "Report 3");
        // Leaving it out adds nothing; nor can a line without a choice be
        // ticked.
        opened.choose(2, false);
        opened.choose(0, true);
        assert_eq!(opened.chosen[..3], [false, false, false]);
        let added = opened.add(&mut book).expect("nothing added");
        assert_eq!((added.added, added.replaced), (0, 0));
        opened.choose(2, true);
        let added = opened.add(&mut book).expect("added");
        assert_eq!((added.added, added.replaced), (1, 0));
        assert_eq!(book.views(Some(ViewKind::View)).expect("views").len(), 3);
    }

    #[test]
    fn a_file_that_is_not_kmymoney_is_refused_with_its_own_code() {
        let book = Book::create_in_memory().expect("book");
        let statement = golden("camt053", "batch-entry.xml");
        let error = open(&book, &statement, &TestLocale).expect_err("refused");
        assert!(matches!(error, FromFileError::NotKmyMoney), "{error:?}");
        let text = crate::book_from_file::error_text_in(Some(&book), &error, &TestLocale);
        assert_eq!(text.code, "input.not-kmymoney");
    }
}
