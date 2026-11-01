// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fixed names of the AI functions, the MCP tools and the screens, which
//! the debug log (D-173, D-227) and issue drafts (D-228) use instead of any
//! text: a name not in [`DETAILS`] is never written.

/// The names a log entry's detail may be: the AI functions and MCP tools a
/// call names, and the screens. Tests in `kassensturz-ai-tools`,
/// `kassensturz-mcp` and `kassensturz-ui-bridge` check that each of theirs
/// is here.
pub const DETAILS: &[&str] = &[
    // The functions of `kassensturz-ai-tools` and the MCP write tools.
    "list_accounts",
    "account_balance",
    "category_totals",
    "search_transactions",
    "list_categories",
    "list_rules",
    "list_budgets",
    "propose_changes",
    "propose_view",
    "check_proposed_changes",
    "read_debug_log",
    "draft_issue",
    "open_page",
    // A call of a function that isn't offered.
    "unknown",
    // The kinds of widget the Overview editor adds (M6-B12), with the dot
    // of the type id written as an underscore; a saved view is one more.
    "chart_bar",
    "chart_pie",
    "chart_donut",
    "chart_line",
    "chart_area",
    "chart_stacked_bar",
    "kpi_card",
    "progress_bar",
    "table_rows",
    "table_income_sources",
    "card_net_worth",
    "budget_progress",
    "text_heading",
    "text_separator",
    "group_period",
    "notice_fetch_gap",
    "notice_transfers",
    "page_period",
    "page_filters",
    "bank_update_all",
    "saved_view",
    // The screens, by the names of their QML files.
    "AccountDetailsPage",
    "AccountsPage",
    "AiChangeReviewPage",
    "AiSettingsPage",
    "BankCheckPage",
    "BankFetchPage",
    "BankSetupPage",
    "BookFromFileDonePage",
    "BookFromFilePage",
    "BookProblemPage",
    // KMyMoney's words for the relative periods a report import turns into
    // fixed days (M6d.6a, Q-319 = A); `kassensturz-pipeline` checks that
    // each of its words is here.
    "today",
    "last7days",
    "last30days",
    "last3months",
    "last6months",
    "last11months",
    "last3tonext3months",
    "monthtodate",
    "untiltoday",
    "alldates",
    "currentquarter",
    "lastquarter",
    "BudgetPage",
    "BudgetsPage",
    "CategoriesPage",
    "CategoryGroupPage",
    "CategoryGroupsPage",
    "CategoryPage",
    "CsvSettingsPage",
    "DashboardFilePage",
    "ExportPage",
    "HistoryPage",
    "ImportDonePage",
    "ImportPreviewPage",
    "ImportStatementsPage",
    "NewAccountPage",
    "OverviewPage",
    "PartsPage",
    "RegisterPage",
    "RulePage",
    "RulesPage",
    "SettingsPage",
    "StatisticsPage",
    "TransactionPage",
    "WelcomePage",
];

mod actions;

pub use actions::ACTIONS;

/// The entry of [`DETAILS`] or [`ACTIONS`] that is `detail`, e.g. for a
/// function a model named.
pub fn known_detail(detail: &str) -> Option<&'static str> {
    DETAILS
        .iter()
        .chain(ACTIONS)
        .copied()
        .find(|known| *known == detail)
}

/// The action of the name `name` from [`ACTIONS`], for a button, menu entry
/// or choice that names itself.
pub fn action(name: &str) -> Option<&'static str> {
    ACTIONS.iter().copied().find(|known| *known == name)
}

/// The screen of the name `name` from [`DETAILS`], for a screen that names
/// itself, e.g. QML's `OverviewPage_QMLTYPE_12(0x…)`: the part before the
/// first `_` or `(`.
pub fn screen(name: &str) -> Option<&'static str> {
    let name = name.split(['_', '(']).next().unwrap_or_default();
    known_detail(name).filter(|detail| detail.ends_with("Page"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screens_are_found_by_the_names_qml_gives_them() {
        assert_eq!(screen("OverviewPage_QMLTYPE_3(0x1)"), Some("OverviewPage"));
        assert_eq!(screen("SettingsPage(0x5)"), Some("SettingsPage"));
        assert_eq!(screen("SecretField_QMLTYPE_1"), None);
        assert_eq!(screen("list_accounts"), None, "only pages are screens");
        assert_eq!(known_detail("draft_issue"), Some("draft_issue"));
        assert_eq!(known_detail("Groceries"), None);
    }

    #[test]
    fn actions_are_known_details() {
        let first = ACTIONS[0];
        assert_eq!(action(first), Some(first));
        assert_eq!(known_detail(first), Some(first));
        assert_eq!(action("Groceries"), None);
        assert_eq!(action("list_accounts"), None);
        for action in ACTIONS {
            assert!(
                action
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "{action}"
            );
            assert!(!DETAILS.contains(action), "{action} is in DETAILS too");
        }
        let mut sorted = ACTIONS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, ACTIONS, "ACTIONS is sorted and without doubles");
    }
}
