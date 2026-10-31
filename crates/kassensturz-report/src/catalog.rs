// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The catalog of datasets (D-277, D-282): what each one counts, in plain
//! words, and which choices a dashboard or the AI may make for it.
//!
//! The description is English text for the AI, which sees only this about a
//! dataset, and the key the UI looks its translation up by, the dataset's
//! `id` (D-034). It says what is counted and what is left out, and states
//! the rounding rule of every ratio (D-285).

use crate::{
    ACCOUNT_BALANCES, BUDGET_UTILIZATION, CASH_FLOW_BY_MONTH, CATEGORY_OVER_TIME, FORMULA_MEASURE,
    INCOME_EXPENSE_BY_MONTH, NET_WORTH_OVER_TIME, SPENDING_BY_CATEGORY,
};

/// One dataset a chart or card can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DatasetInfo {
    /// The ID dashboards and the AI use.
    pub id: &'static str,
    /// What it counts and leaves out, in plain words.
    pub description: &'static str,
    /// The ways to group it, by fixed name; the first is the default.
    pub groupings: &'static [&'static str],
    /// The kinds of chart or card it suits, by fixed name.
    pub suits: &'static [&'static str],
    /// The figures a key figure card can show of it, by fixed name; the
    /// first is the default. They are the words of the card's `measure`
    /// (D-368). Empty for a dataset no card shows.
    pub measures: &'static [&'static str],
    /// Whether the AI may choose it for a chart it prepares. Not in M6b for
    /// anything that uses My groups or the hide list: the AI sees neither
    /// (D-291).
    pub for_ai: bool,
}

#[cfg(test)]
const LEAVES_OUT: &str = "Money moved between your own accounts and starting balances are not counted, \
     and amounts in different currencies are never added together: there is one result per currency.";

/// What a component without data (the heading, D-312) holds in place of a
/// dataset. It is in no listing and no document names it.
pub(crate) const NO_DATASET: DatasetInfo = DatasetInfo {
    id: "",
    description: "",
    groupings: &[],
    suits: &[],
    measures: &[],
    for_ai: false,
};

/// The datasets, in the order they are offered.
pub const CATALOG: &[DatasetInfo] = &[
    DatasetInfo {
        id: SPENDING_BY_CATEGORY,
        description: "What you spent in a period, by category. A refund reduces the category it \
             came back to. Money you have not sorted into a category yet is one line of its own. \
             Money moved between your own accounts and starting balances are not counted, and \
             amounts in different currencies are never added together: there is one result per \
             currency.",
        groupings: &["category", "top_level_category", GROUPS_GROUPING],
        suits: &["pie", "bar", "table"],
        measures: &[],
        for_ai: true,
    },
    DatasetInfo {
        id: INCOME_EXPENSE_BY_MONTH,
        description: "Money in and money out in each calendar month of a period, and what is \
             left (money in less money out). The savings rate is what is left as a percentage of \
             money in, rounded half away from zero to one decimal place; it has no value for a \
             period without money in. Money moved between your own accounts and starting \
             balances are not counted, and amounts in different currencies are never added \
             together: there is one result per currency.",
        groupings: &["month"],
        suits: &["line", "stacked_bar", "kpi", "table"],
        measures: &["income", "expenses", "net", "savings-rate", FORMULA_MEASURE],
        for_ai: true,
    },
    DatasetInfo {
        id: CATEGORY_OVER_TIME,
        description: "What you spent (or received, for income categories) in each calendar month \
             of a period, for the categories and groups the user picked, one line each. A refund \
             reduces the category it came back to. A category counts with everything inside it \
             and a group counts all its members, each category once, hidden ones too. Money moved \
             between your own accounts and starting balances are not counted, and amounts in \
             different currencies are never added together: there is one result per currency.",
        groupings: &["series"],
        suits: &["line", "stacked_bar"],
        measures: &[],
        for_ai: false,
    },
    // How much of each budget is spent and left (D-346, D-347), built in
    // M6c.4 and drawn since M6c.5 (D-378, D-381). The AI sees budgets only
    // with the Budgets row of the permission grid (D-358, M6c.11), so it
    // does not know this dataset.
    DatasetInfo {
        id: BUDGET_UTILIZATION,
        description: "For each budget: how much was planned for one of its periods, what was carried \
         in from the period before, what was available (planned plus carried in), what was \
         spent, what is left (available less spent; below zero, the amount it is over by) and \
         the share used. Spent is what was booked to the budget's category and every category \
         inside it, each category once, by the transaction's date, from the period's first day \
         to the day asked for (the shown period's last day, or today if that is earlier), in \
         the category's own currency as it was booked; a refund reduces it. Hidden categories \
         count as well, and money not sorted into a category yet is not in any budget. Without \
         a grouping each budget shows the period that contains the day asked for; a budget that \
         has not started or has ended by then is not shown. Grouped by period, each budget \
         shows every one of its periods the shown period touches, up to the day asked for. \
         With carry over on, what is left at the end of a period, or what was spent beyond it, \
         goes into the next one; the chain starts at zero in the budget's first period. A \
         change of the planned amount applies from one period on and never changes an earlier \
         one. The share used is spent divided by available, rounded half away from zero to one \
         decimal place of a percent; it has no value when nothing is available. Money moved \
         between your own accounts and starting balances are not counted, and amounts in \
         different currencies are never added together: there is one result per currency.",
        groupings: &["budget", "period"],
        suits: &["budget"],
        measures: &[],
        for_ai: false,
    },
];

/// The datasets of what the user has, owes and is worth and of the money in
/// and out of their accounts (D-348, D-349), built in M6c.1. They join
/// [`CATALOG`] in the step that teaches the app to draw them (M6c.2), so no
/// chart offers a dataset it cannot show yet (D-368). Until then no
/// component, document or AI names them.
pub const FINANCIAL: &[DatasetInfo] = &[
    DatasetInfo {
        id: ACCOUNT_BALANCES,
        description: "What each of your accounts held at the end of the period's last day, and \
             what you have, what you owe and what you are worth on that day. What you have is \
             what your checking, savings and cash accounts hold together; what you owe is the \
             debt on your credit cards, as a positive amount; what you are worth is the first \
             less the second. An account counts by its kind, not by the sign of its balance: an \
             overdrawn checking account lowers what you have. A day's balance counts what the \
             bank booked by that day where the bank said so, otherwise what is dated up to that \
             day, and starting balances count. A closed account is listed only if its balance \
             is not zero. Categories are not counted. Amounts in different currencies are never \
             added together: there is one result per currency. A total in one currency converts \
             each other currency with the newest exchange rate the book has on or before the \
             day, in either direction, rounded once; a currency without one is left out of that \
             total and named, never guessed.",
        groupings: &["account", "account_kind"],
        suits: &["kpi", "table"],
        measures: &["net_worth", "assets", "liabilities", FORMULA_MEASURE],
        for_ai: false,
    },
    DatasetInfo {
        id: NET_WORTH_OVER_TIME,
        description: "What you have, what you owe and what you are worth at the end of each \
             calendar month of a period (for the last month, at the end of the period's last \
             day), counted as for account_balances: what you have is what your checking, savings \
             and cash accounts hold together, what you owe is the debt on your credit cards as a \
             positive amount, and what you are worth is the first less the second. Starting \
             balances count; categories are not counted. Amounts in different currencies are \
             never added together: there is one result per currency. A total in one currency \
             converts each other currency with the newest exchange rate the book has on or \
             before each month's day, in either direction, rounded once; a currency without one \
             is left out of that month's total and named, never guessed.",
        groupings: &["month"],
        suits: &["line", "table"],
        measures: &["net_worth", "assets", "liabilities"],
        for_ai: false,
    },
    DatasetInfo {
        id: CASH_FLOW_BY_MONTH,
        description: "For each calendar month of a period, of the chosen accounts together \
             (without a choice, all checking, savings and cash accounts): what they held at the \
             start, the money that came in, the money that went out, the starting balances set \
             in the month, and what they held at the end. The start plus money in, less money \
             out, plus starting balances is exactly the end. A transaction counts with what it \
             adds to or takes from the chosen accounts together, so money moved between two \
             chosen accounts counts as neither in nor out, and paying a credit card that is not \
             chosen is money out. The net change is money in less money out; starting balances \
             are not in it. Amounts in different currencies are never added together: there is \
             one result per currency.",
        groupings: &["month"],
        suits: &["bar", "table"],
        measures: &["net_change", "money_in", "money_out"],
        for_ai: false,
    },
];

/// The grouping that shows My groups; the AI does not know it (D-291).
pub const GROUPS_GROUPING: &str = "groups";

/// The dataset with this ID.
pub fn dataset(id: &str) -> Option<&'static DatasetInfo> {
    CATALOG.iter().find(|dataset| dataset.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn every_dataset_is_described_once_and_says_what_it_leaves_out() {
        let every = || CATALOG.iter().chain(FINANCIAL);
        let mut ids: Vec<&str> = every().map(|dataset| dataset.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len() + FINANCIAL.len());
        for dataset in every() {
            assert!(!dataset.groupings.is_empty(), "{}", dataset.id);
            assert!(!dataset.suits.is_empty(), "{}", dataset.id);
            // A dataset a key figure card shows has figures to choose from.
            assert!(
                !dataset.suits.contains(&"kpi") || !dataset.measures.is_empty(),
                "{}",
                dataset.id
            );
            assert!(
                flat(dataset.description).contains(
                    "Amounts in different currencies are never added together: there is one \
                     result per currency."
                ) || flat(dataset.description).contains(
                    "amounts in different currencies are never added together: there is one \
                     result per currency."
                ),
                "{} must say it keeps currencies apart",
                dataset.id
            );
        }
        // The datasets of balances count everything booked to an account,
        // starting balances too, and say so; the others leave them out.
        let balances = [ACCOUNT_BALANCES, NET_WORTH_OVER_TIME, CASH_FLOW_BY_MONTH];
        for dataset in every() {
            let described = flat(dataset.description);
            if balances.contains(&dataset.id) {
                let lower = described.to_lowercase();
                assert!(
                    lower.contains("starting balances")
                        && !lower.contains("starting balances are not counted"),
                    "{}",
                    dataset.id
                );
            } else {
                // The shared sentence is the contract; each description says it.
                assert!(
                    described.contains(&flat(LEAVES_OUT)),
                    "{} must say what it leaves out",
                    dataset.id
                );
            }
        }
        assert_eq!(
            dataset("spending_by_category").map(|dataset| dataset.id),
            Some(SPENDING_BY_CATEGORY)
        );
        assert!(dataset("no_such_dataset").is_none());
        // Not offered before the app can draw them (D-368).
        for financial in FINANCIAL {
            assert!(dataset(financial.id).is_none(), "{}", financial.id);
            assert!(!financial.for_ai, "{}", financial.id);
        }
    }
}
