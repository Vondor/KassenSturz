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
    "propose_changes",
    "check_proposed_changes",
    "read_debug_log",
    "draft_issue",
    "open_page",
    // A call of a function that isn't offered.
    "unknown",
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
    "CategoriesPage",
    "CategoryPage",
    "CsvSettingsPage",
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

/// The entry of [`DETAILS`] that is `detail`, e.g. for a function a model
/// named.
pub fn known_detail(detail: &str) -> Option<&'static str> {
    DETAILS.iter().copied().find(|known| *known == detail)
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
}
