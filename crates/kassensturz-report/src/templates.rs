// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The built-in starter dashboards, by the word a dashboard made from one
//! keeps in its `template` key (D-439, D-440). M6d.1 started the set with
//! `overview` (the built-in Overview, D-314) and `empty`; M6d.2 added the
//! others, each with its word here and its document in [`document`].
//!
//! A template holds no English text a user would see: titles stay empty so
//! each component shows its own translated default, a heading holds
//! [`HEADING_KEY`] until the screen that makes the dashboard puts its
//! translated text in with [`fill_headings`], and the dashboard's name is
//! given by that screen too (`docs/REPORT_DESIGNER_M6D.md`, section 2).
//!
//! Every template except *Empty* starts with the period header and the
//! filter bar (D-452, D-474), widgets like the others, and is written in
//! the current [`crate::VERSION`]; a test makes sure a new version
//! revisits them, as it does the built-in Overview.

use kassensturz_core::ViewKind;

use crate::{Document, Entry, Item, OVERVIEW_NAME, ReportError, Setting};

/// The template of the built-in Overview.
pub const OVERVIEW: &str = "overview";
/// The template with no components.
pub const EMPTY: &str = "empty";

/// What you spent and on what.
pub const SPENDING: &str = "spending";
/// Money in and out, month by month.
pub const MONEY_IN_OUT: &str = "money-in-out";
/// What you have, owe and are worth.
pub const WORTH: &str = "worth";
/// The budgets and what was spent.
pub const BUDGETS: &str = "budgets";
/// The accounts, their banks' warnings and the Update all accounts button;
/// offered only while bank connections can be used (D-054).
pub const ACCOUNTS: &str = "accounts";

/// Every word the `template` key may hold, in the order a list shows them:
/// *Empty* first, *Overview* second (section 2).
pub const WORDS: [&str; 7] = [
    EMPTY,
    OVERVIEW,
    SPENDING,
    MONEY_IN_OUT,
    WORTH,
    BUDGETS,
    ACCOUNTS,
];

/// The text a template's heading holds until [`fill_headings`] gives it the
/// user's words. It is never shown: a dashboard is made only through
/// [`fill_headings`].
pub const HEADING_KEY: &str = "template-heading";

/// *Where the money goes*: a heading; the spending ring and the table of
/// what it shows, for the page's period; what went out, with the period
/// before; the spending by category over the last 12 months as bars (Q-336).
const SPENDING_TEXT: &str = r#"{
  "format": "kassensturz-dashboard", "version": 3, "kind": "dashboard",
  "name": "spending",
  "components": [
    {"id": "period-header", "type": "page.period",
     "frame": {"col": 0, "row": 0, "cols": 12, "rows": 1}},
    {"id": "filter-bar", "type": "page.filters",
     "frame": {"col": 0, "row": 1, "cols": 12, "rows": 1}},
    {"id": "heading", "type": "text.heading", "props": {"text": "template-heading"},
     "frame": {"col": 0, "row": 2, "cols": 12, "rows": 1}},
    {"id": "spending", "type": "chart.donut", "dataset": "spending_by_category",
     "params": {"grouping": "top_level_category"},
     "frame": {"col": 0, "row": 3, "cols": 6, "rows": 5}},
    {"id": "biggest", "type": "table.rows", "dataset": "spending_by_category",
     "params": {"grouping": "category"},
     "frame": {"col": 6, "row": 3, "cols": 6, "rows": 5}},
    {"id": "expenses", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {"measure": "expenses", "compare": "previous_period"},
     "frame": {"col": 0, "row": 8, "cols": 3, "rows": 2}},
    {"id": "year", "type": "chart.bar", "dataset": "spending_by_category",
     "params": {"grouping": "top_level_category"},
     "props": {"period": "last-12-months"},
     "frame": {"col": 3, "row": 8, "cols": 9, "rows": 4}}
  ]
}"#;

/// *Money in and out*: the four key figures of the Overview; the money in
/// and out of the accounts by month as bars, last 12 months; where the
/// money came from; the same months as a table.
const MONEY_IN_OUT_TEXT: &str = r#"{
  "format": "kassensturz-dashboard", "version": 3, "kind": "dashboard",
  "name": "money-in-out",
  "components": [
    {"id": "period-header", "type": "page.period",
     "frame": {"col": 0, "row": 0, "cols": 12, "rows": 1}},
    {"id": "filter-bar", "type": "page.filters",
     "frame": {"col": 0, "row": 1, "cols": 12, "rows": 1}},
    {"id": "income", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {"measure": "income", "compare": "previous_period"},
     "frame": {"col": 0, "row": 2, "cols": 3, "rows": 2}},
    {"id": "expenses", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {"measure": "expenses", "compare": "previous_period"},
     "frame": {"col": 3, "row": 2, "cols": 3, "rows": 2}},
    {"id": "left", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {"measure": "net", "compare": "previous_period"},
     "frame": {"col": 6, "row": 2, "cols": 3, "rows": 2}},
    {"id": "kept", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {"measure": "savings-rate", "compare": "previous_period"},
     "frame": {"col": 9, "row": 2, "cols": 3, "rows": 2}},
    {"id": "months", "type": "chart.bar", "dataset": "cash_flow_by_month",
     "props": {"period": "last-12-months"},
     "frame": {"col": 0, "row": 4, "cols": 12, "rows": 4}},
    {"id": "sources", "type": "table.income_sources",
     "frame": {"col": 0, "row": 8, "cols": 6, "rows": 4}},
    {"id": "month-table", "type": "table.rows", "dataset": "cash_flow_by_month",
     "props": {"period": "last-12-months"},
     "frame": {"col": 6, "row": 8, "cols": 6, "rows": 4}}
  ]
}"#;

/// *What you own*: what you have, owe and are worth; what you were worth
/// over the last 12 months; what each account holds.
const WORTH_TEXT: &str = r#"{
  "format": "kassensturz-dashboard", "version": 3, "kind": "dashboard",
  "name": "worth",
  "components": [
    {"id": "period-header", "type": "page.period",
     "frame": {"col": 0, "row": 0, "cols": 12, "rows": 1}},
    {"id": "filter-bar", "type": "page.filters",
     "frame": {"col": 0, "row": 1, "cols": 12, "rows": 1}},
    {"id": "have", "type": "kpi.card", "dataset": "account_balances",
     "params": {"measure": "assets"},
     "frame": {"col": 0, "row": 2, "cols": 4, "rows": 2}},
    {"id": "owe", "type": "kpi.card", "dataset": "account_balances",
     "params": {"measure": "liabilities"},
     "frame": {"col": 4, "row": 2, "cols": 4, "rows": 2}},
    {"id": "worth", "type": "kpi.card", "dataset": "account_balances",
     "params": {"measure": "net_worth"},
     "frame": {"col": 8, "row": 2, "cols": 4, "rows": 2}},
    {"id": "over-time", "type": "chart.line", "dataset": "net_worth_over_time",
     "props": {"period": "last-12-months"},
     "frame": {"col": 0, "row": 4, "cols": 12, "rows": 4}},
    {"id": "accounts", "type": "table.rows", "dataset": "account_balances",
     "params": {"grouping": "account"},
     "frame": {"col": 0, "row": 8, "cols": 12, "rows": 5}}
  ]
}"#;

/// *Budgets*: a heading; the bars of every budget, which take no room until
/// the book has one (D-357); the spending ring for the page's period.
const BUDGETS_TEXT: &str = r#"{
  "format": "kassensturz-dashboard", "version": 3, "kind": "dashboard",
  "name": "budgets",
  "components": [
    {"id": "period-header", "type": "page.period",
     "frame": {"col": 0, "row": 0, "cols": 12, "rows": 1}},
    {"id": "filter-bar", "type": "page.filters",
     "frame": {"col": 0, "row": 1, "cols": 12, "rows": 1}},
    {"id": "heading", "type": "text.heading", "props": {"text": "template-heading"},
     "frame": {"col": 0, "row": 2, "cols": 12, "rows": 1}},
    {"id": "budgets", "type": "budget.progress", "dataset": "budget_utilization",
     "frame": {"col": 0, "row": 3, "cols": 12, "rows": 4}},
    {"id": "spending", "type": "chart.donut", "dataset": "spending_by_category",
     "params": {"grouping": "top_level_category"},
     "frame": {"col": 0, "row": 7, "cols": 6, "rows": 5}}
  ]
}"#;

/// *Your accounts*: the warnings about accounts not updated from the bank;
/// the Update all accounts button (M6d.7, D-446, D-471); what each account
/// holds.
const ACCOUNTS_TEXT: &str = r#"{
  "format": "kassensturz-dashboard", "version": 3, "kind": "dashboard",
  "name": "accounts",
  "components": [
    {"id": "period-header", "type": "page.period",
     "frame": {"col": 0, "row": 0, "cols": 12, "rows": 1}},
    {"id": "filter-bar", "type": "page.filters",
     "frame": {"col": 0, "row": 1, "cols": 12, "rows": 1}},
    {"id": "fetch-gaps", "type": "notice.fetch_gap",
     "frame": {"col": 0, "row": 2, "cols": 12, "rows": 1}},
    {"id": "update-all", "type": "bank.update_all",
     "frame": {"col": 0, "row": 3, "cols": 6, "rows": 3}},
    {"id": "accounts", "type": "table.rows", "dataset": "account_balances",
     "params": {"grouping": "account"},
     "frame": {"col": 6, "row": 3, "cols": 6, "rows": 6}}
  ]
}"#;

/// The document of the template `word`, carrying the word in its `template`
/// key, named after the word until the screen gives it the user's name. A
/// word that is not one of [`WORDS`] is refused (`report.document-invalid`,
/// `bad-choice`, path `template`).
pub fn document(word: &str) -> Result<Document, ReportError> {
    let mut document = match word {
        OVERVIEW => Document::default_overview()?,
        EMPTY => Document {
            kind: ViewKind::Dashboard,
            name: OVERVIEW_NAME.to_owned(),
            period: None,
            accounts: Vec::new(),
            categories: Vec::new(),
            entries: Vec::new(),
            template: None,
            default: false,
        },
        SPENDING => Document::read(SPENDING_TEXT)?,
        MONEY_IN_OUT => Document::read(MONEY_IN_OUT_TEXT)?,
        WORTH => Document::read(WORTH_TEXT)?,
        BUDGETS => Document::read(BUDGETS_TEXT)?,
        ACCOUNTS => Document::read(ACCOUNTS_TEXT)?,
        _ => {
            return Err(ReportError::DocumentInvalid {
                reason: "bad-choice",
                path: "template".to_owned(),
            });
        }
    };
    document.template = Some(word.to_owned());
    Ok(document)
}

/// Puts `text`, the heading in the user's language, into every heading of
/// `document` that still holds [`HEADING_KEY`], once, when a dashboard is
/// made from a template. Empty text leaves such a heading out, so the key
/// is never shown.
pub fn fill_headings(document: &mut Document, text: &str) {
    let text = text.trim();
    document.entries.retain_mut(|entry| {
        let Item::Component(component) = &mut entry.item else {
            return true;
        };
        if !matches!(component.get("text"), Some(Setting::Text(held)) if held == HEADING_KEY) {
            return true;
        }
        !text.is_empty()
            && component
                .set("text", Setting::Text(text.to_owned()))
                .is_ok()
    });
}

/// Whether `entries` are the components of the template `word`, in the
/// same places, as [`fill_headings`] made them: a heading that holds
/// [`HEADING_KEY`] in the template matches a heading with the same ID and
/// frame whatever its words, since those are the user's (D-479). A word that
/// is not a template matches nothing.
pub fn holds_components_of(word: &str, entries: &[Entry]) -> bool {
    let Ok(template) = document(word) else {
        return false;
    };
    template.entries.len() == entries.len()
        && template.entries.iter().zip(entries).all(|(wanted, held)| {
            wanted == held
                || (is_heading_key(wanted)
                    && wanted.id == held.id
                    && wanted.frame == held.frame
                    && matches!(&held.item, Item::Component(component) if component.kind() == wanted_kind(wanted)))
        })
}

/// Whether `entry` is a template heading still holding [`HEADING_KEY`].
fn is_heading_key(entry: &Entry) -> bool {
    matches!(&entry.item, Item::Component(component)
        if matches!(component.get("text"), Some(Setting::Text(held)) if held == HEADING_KEY))
}

/// The type of a known component (only called for one).
fn wanted_kind(entry: &Entry) -> crate::ComponentType {
    match &entry.item {
        Item::Component(component) => component.kind(),
        Item::Unknown(_) => crate::ComponentType::Heading,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComponentType;

    #[test]
    fn every_template_reads_back_as_a_dashboard_with_its_word() {
        for word in WORDS {
            let document = document(word).unwrap();
            assert_eq!(document.kind, ViewKind::Dashboard);
            assert_eq!(document.template.as_deref(), Some(word));
            assert!(!document.default);
            let again = Document::read(&document.to_json()).unwrap();
            assert_eq!(again, document);
        }
    }

    #[test]
    fn the_overview_template_is_the_built_in_overview() {
        let template = document(OVERVIEW).unwrap();
        let built_in = Document::default_overview().unwrap();
        assert_eq!(template.entries, built_in.entries);
        assert_eq!(template.filters(), built_in.filters());
        assert!(
            template
                .entries
                .iter()
                .all(|entry| matches!(entry.item, Item::Component(_)))
        );
    }

    #[test]
    fn the_empty_template_has_no_components() {
        assert!(document(EMPTY).unwrap().entries.is_empty());
    }

    #[test]
    fn a_word_that_is_not_a_template_is_refused() {
        for word in ["", "Overview", "Spending", "money_in_out", "nonsense"] {
            assert!(matches!(
                document(word),
                Err(ReportError::DocumentInvalid {
                    reason: "bad-choice",
                    ..
                })
            ));
        }
    }

    /// The components of a template, by type id.
    fn types(word: &str) -> Vec<&'static str> {
        document(word)
            .unwrap()
            .entries
            .iter()
            .map(|entry| match &entry.item {
                Item::Component(component) => component.kind().id(),
                Item::Unknown(_) => "unknown",
            })
            .collect()
    }

    #[test]
    fn every_template_is_made_of_known_components_on_the_grid_without_titles() {
        for word in WORDS {
            let document = document(word).unwrap();
            for entry in &document.entries {
                let Item::Component(component) = &entry.item else {
                    panic!("{word}: {} is not a known component", entry.id);
                };
                let frame = entry.frame.expect("a dashboard component has a frame");
                assert!(frame.right() <= 12, "{word}: {} is too wide", entry.id);
                // No English: every component shows its translated default
                // title.
                assert!(
                    !matches!(component.get("title"), Some(Setting::Text(title)) if !title.is_empty()),
                    "{word}: {} has a title",
                    entry.id
                );
            }
            crate::grid::check(&document.entries).unwrap();
        }
    }

    #[test]
    fn the_templates_hold_what_section_2_lists() {
        // Every template but Empty starts with the page's two widgets.
        for word in WORDS {
            let kinds = types(word);
            if word == EMPTY {
                assert!(kinds.is_empty());
            } else {
                assert_eq!(kinds[..2], ["page.period", "page.filters"], "{word}");
            }
        }
        let rest = |word| types(word).split_off(2);
        assert_eq!(
            rest(SPENDING),
            [
                "text.heading",
                "chart.donut",
                "table.rows",
                "kpi.card",
                "chart.bar"
            ]
        );
        assert_eq!(
            rest(MONEY_IN_OUT),
            [
                "kpi.card",
                "kpi.card",
                "kpi.card",
                "kpi.card",
                "chart.bar",
                "table.income_sources",
                "table.rows"
            ]
        );
        assert_eq!(
            rest(WORTH),
            [
                "kpi.card",
                "kpi.card",
                "kpi.card",
                "chart.line",
                "table.rows"
            ]
        );
        assert_eq!(
            rest(BUDGETS),
            ["text.heading", "budget.progress", "chart.donut"]
        );
        // The Update all accounts widget of M6d.7 (D-440, D-471).
        assert_eq!(
            rest(ACCOUNTS),
            ["notice.fetch_gap", "bank.update_all", "table.rows"]
        );
        assert!(
            document(ACCOUNTS)
                .unwrap()
                .entries
                .iter()
                .any(|entry| matches!(&entry.item, Item::Component(c) if c.kind() == ComponentType::BankUpdateAll))
        );
    }

    #[test]
    fn every_template_text_is_written_in_the_current_version() {
        // A new version revisits the templates, as it does the built-in
        // Overview, instead of leaving them to the reading of older ones.
        for text in [
            SPENDING_TEXT,
            MONEY_IN_OUT_TEXT,
            WORTH_TEXT,
            BUDGETS_TEXT,
            ACCOUNTS_TEXT,
        ] {
            let value: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(value["version"], crate::VERSION);
        }
    }

    #[test]
    fn a_heading_gets_the_users_words_once_and_never_shows_the_key() {
        for word in WORDS {
            let mut made = document(word).unwrap();
            let headings = types(word)
                .iter()
                .filter(|kind| **kind == "text.heading")
                .count();
            fill_headings(&mut made, "  Wohin das Geld geht ");
            let texts: Vec<_> = made
                .entries
                .iter()
                .filter_map(|entry| match &entry.item {
                    Item::Component(component) => component.get("text").cloned(),
                    Item::Unknown(_) => None,
                })
                .collect();
            assert_eq!(texts.len(), headings, "{word}");
            assert!(
                texts
                    .iter()
                    .all(|text| *text == Setting::Text("Wohin das Geld geht".to_owned())),
                "{word}"
            );
            // What is stored reads back the same.
            assert_eq!(Document::read(&made.to_json()).unwrap(), made);
            // Without words, the heading is left out instead of showing the key.
            let mut bare = document(word).unwrap();
            fill_headings(&mut bare, " ");
            assert_eq!(bare.entries.len(), types(word).len() - headings, "{word}");
            assert!(!bare.to_json().contains(HEADING_KEY), "{word}");
        }
    }

    #[test]
    fn a_made_template_still_holds_its_components_whatever_the_heading_says() {
        for word in WORDS {
            let mut made = document(word).unwrap();
            fill_headings(&mut made, "Meins");
            assert!(holds_components_of(word, &made.entries), "{word}");
            let mut moved = made.clone();
            if let Some(frame) = moved
                .entries
                .last_mut()
                .and_then(|entry| entry.frame.as_mut())
            {
                frame.row += 1;
                assert!(!holds_components_of(word, &moved.entries), "{word}");
            }
        }
        // Without its heading, Where the money goes is not its template.
        let mut bare = document(SPENDING).unwrap();
        fill_headings(&mut bare, "");
        assert!(!holds_components_of(SPENDING, &bare.entries));
        assert!(!holds_components_of("radar", &[]));
        assert!(holds_components_of(EMPTY, &[]));
    }

    #[test]
    fn a_heading_the_user_wrote_is_left_alone() {
        let mut made = document(SPENDING).unwrap();
        fill_headings(&mut made, "Mine");
        let text = made.to_json();
        fill_headings(&mut made, "Other");
        assert_eq!(made.to_json(), text);
    }
}
