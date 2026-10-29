// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The catalog of datasets (D-277, D-282): what each one counts, in plain
//! words, and which choices a dashboard or the AI may make for it.
//!
//! The description is English text for the AI, which sees only this about a
//! dataset, and the key the UI looks its translation up by, the dataset's
//! `id` (D-034). It says what is counted and what is left out, and states
//! the rounding rule of every ratio (D-285).

use crate::{INCOME_EXPENSE_BY_MONTH, SPENDING_BY_CATEGORY};

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
}

#[cfg(test)]
const LEAVES_OUT: &str = "Money moved between your own accounts and starting balances are not counted, \
     and amounts in different currencies are never added together: there is one result per currency.";

/// The datasets, in the order they are offered.
pub const CATALOG: &[DatasetInfo] = &[
    DatasetInfo {
        id: SPENDING_BY_CATEGORY,
        description: "What you spent in a period, by category. A refund reduces the category it \
             came back to. Money you have not sorted into a category yet is one line of its own. \
             Money moved between your own accounts and starting balances are not counted, and \
             amounts in different currencies are never added together: there is one result per \
             currency.",
        groupings: &["category", "top_level_category"],
        suits: &["pie", "bar", "table"],
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
    },
];

/// The dataset with this ID.
pub fn dataset(id: &str) -> Option<&'static DatasetInfo> {
    CATALOG.iter().find(|dataset| dataset.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dataset_is_described_once_and_says_what_it_leaves_out() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|dataset| dataset.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
        for dataset in CATALOG {
            assert!(!dataset.groupings.is_empty(), "{}", dataset.id);
            assert!(!dataset.suits.is_empty(), "{}", dataset.id);
            // The shared sentence is the contract; each description says it.
            let words: Vec<&str> = LEAVES_OUT.split_whitespace().collect();
            let described: String = dataset
                .description
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                described.contains(&words.join(" ")),
                "{} must say what it leaves out",
                dataset.id
            );
        }
        assert_eq!(
            dataset("spending_by_category").map(|dataset| dataset.id),
            Some(SPENDING_BY_CATEGORY)
        );
        assert!(dataset("no_such_dataset").is_none());
    }
}
