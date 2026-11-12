// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which components the Statistic Builder makes (US5.5, #753). The rule is
//! that every chart the layout editor can place can also be made in the
//! Builder. Every entry of [`ComponentType::LIBRARY`] is therefore in exactly
//! one of these lists, and a test fails for a new component that is in
//! none: it needs a Builder entry, or a mark that it is not a chart.
//!
//! The lists live here, not in `schema.rs`, so steps that change the schema
//! side by side don't touch the same lines.

use crate::ComponentType;

impl ComponentType {
    /// Every component the Statistic Builder offers, in the order its
    /// "Show as" list names them. The screens' own lists
    /// (`StatisticsPage.qml`, `StatisticWizard.qml`, `OverviewPage.qml`) are
    /// checked against it by the UI bridge's `builder_audit`.
    pub const BUILDER: [Self; 11] = [
        Self::ChartBar,
        Self::ChartPie,
        Self::ChartDonut,
        Self::ChartLine,
        Self::ChartArea,
        Self::ChartStackedBar,
        Self::KpiCard,
        Self::ProgressBar,
        Self::TableRows,
        Self::ChartCandlestick,
        Self::BudgetProgress,
    ];

    /// The components that are not a chart, so the Builder doesn't offer
    /// them: text and lines that divide the page, the page's own notices,
    /// period header, filter bar and tabs, the bank button, and the
    /// ready-made tiles that have no data choices of their own (net worth,
    /// income sources and the warnings tile, Q-507). A Builder step for them
    /// would only ask questions with one answer.
    pub const NOT_A_CHART: [Self; 13] = [
        Self::Heading,
        Self::Separator,
        Self::FetchNotice,
        Self::TransfersNote,
        Self::UnsortedNote,
        Self::PeriodBox,
        Self::NetWorth,
        Self::IncomeSources,
        Self::BankUpdateAll,
        Self::PeriodHeader,
        Self::FilterBar,
        Self::DashboardTabs,
        Self::Alarms,
    ];

    /// Charts the Builder does not offer yet: the second step of #753 adds
    /// them and empties this list.
    pub const BUILDER_NEXT: [Self; 2] = [Self::CustomChart, Self::Transactions];

    /// Whether the Statistic Builder makes this component.
    pub fn in_builder(self) -> bool {
        Self::BUILDER.contains(&self)
    }
}

#[cfg(test)]
mod tests {
    use crate::ComponentType;

    /// The rule of #753: a component the library offers is made in the
    /// Builder or marked as not a chart, never neither and never both. A
    /// new component type fails here until it gets one of the two.
    #[test]
    fn every_library_component_has_a_builder_entry_or_is_not_a_chart() {
        for kind in ComponentType::LIBRARY {
            let lists = [
                ComponentType::BUILDER.contains(kind),
                ComponentType::NOT_A_CHART.contains(kind),
                ComponentType::BUILDER_NEXT.contains(kind),
            ];
            assert_eq!(
                lists.iter().filter(|&&listed| listed).count(),
                1,
                "{} must be in exactly one of BUILDER, NOT_A_CHART and BUILDER_NEXT",
                kind.id()
            );
        }
        let listed = ComponentType::BUILDER.len()
            + ComponentType::NOT_A_CHART.len()
            + ComponentType::BUILDER_NEXT.len();
        assert_eq!(
            listed,
            ComponentType::LIBRARY.len(),
            "a list holds a component twice"
        );
    }

    /// Everything the AI may prepare is a chart the Builder makes as well.
    #[test]
    fn the_builder_makes_every_component_the_ai_may_pick() {
        for kind in ComponentType::ALL {
            assert!(kind.in_builder(), "{}", kind.id());
        }
    }

    /// A component marked as not a chart names no dataset, so there is
    /// nothing for a Builder step to choose; none of them is in the Builder.
    #[test]
    fn nothing_marked_not_a_chart_is_in_the_builder() {
        for kind in ComponentType::NOT_A_CHART {
            assert!(!kind.in_builder(), "{}", kind.id());
            assert!(kind.shows_no_data(), "{}", kind.id());
        }
    }
}
