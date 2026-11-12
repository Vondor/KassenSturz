// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests that the Statistic Builder's screens offer exactly the components of
//! [`ComponentType::BUILDER`] (US5.5, #753). The screens keep their own lists
//! in QML, so a type added to the Rust list but forgotten on a screen, or the
//! other way round, fails here. The tests read the QML as text, as
//! `plain_text_audit` does.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use kassensturz_report::ComponentType;

    fn qml(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("qml").join(name);
        fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
    }

    /// The text between double quotes in `text`, in order.
    fn quoted(text: &str) -> Vec<String> {
        text.split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect()
    }

    /// The ids of the one-line list that follows `start` on its line.
    fn list_after(text: &str, start: &str) -> Vec<String> {
        let line = text
            .lines()
            .find(|line| line.trim_start().starts_with(start))
            .unwrap_or_else(|| panic!("no line starts with {start}"));
        quoted(&line[line.find('[').expect("a list")..])
    }

    fn builder_ids() -> Vec<String> {
        ComponentType::BUILDER
            .iter()
            .map(|kind| kind.id().to_owned())
            .collect()
    }

    /// The "Show as" list of the page with every setting, in the Builder's
    /// order, and a name for each of them.
    #[test]
    fn the_settings_page_offers_every_builder_type_with_a_name() {
        let page = qml("StatisticsPage.qml");
        assert_eq!(
            list_after(&page, "readonly property var types:"),
            builder_ids()
        );
        let names = &page[page.find("function typeName").expect("typeName")
            ..page.find("function periodName").expect("periodName")];
        for id in builder_ids() {
            assert!(
                names.contains(&format!("case \"{id}\":")),
                "no name for {id}"
            );
        }
    }

    /// The wizard's kinds hold every Builder type once and nothing else.
    #[test]
    fn the_wizard_offers_every_builder_type_once() {
        let wizard = qml("StatisticWizard.qml");
        let start = wizard
            .find("readonly property var kinds: [")
            .expect("the kinds");
        let end = start
            + wizard[start..]
                .find("\n    ]")
                .expect("the end of the kinds");
        let mut offered: Vec<String> = wizard[start..end]
            .lines()
            .filter_map(|line| line.find("types: [").map(|at| quoted(&line[at..])))
            .flatten()
            .collect();
        offered.sort();
        let mut wanted = builder_ids();
        wanted.sort();
        assert_eq!(offered, wanted);
    }

    /// "Change in the Statistic Builder" on the Overview is offered for the
    /// same types the Builder makes.
    #[test]
    fn the_overview_sends_every_builder_type_to_the_builder() {
        let overview = qml("OverviewPage.qml");
        assert_eq!(
            list_after(&overview, "readonly property var builderTypes:"),
            builder_ids()
        );
    }
}
