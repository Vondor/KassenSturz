// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the field table against the real catalog.

use super::*;
use crate::CATALOG;

#[test]
fn every_catalog_dataset_has_a_table_and_the_reverse() {
    // The money flow is drawn by its own chart and no custom chart can read
    // it, so it has no table of fields (it suits only `flow`).
    let mut catalog: Vec<_> = CATALOG
        .iter()
        .filter(|dataset| !dataset.suits.contains(&"flow"))
        .map(|dataset| dataset.id)
        .collect();
    let mut tables: Vec<_> = SPEC_DATASETS.iter().map(SpecDataset::id).collect();
    catalog.sort_unstable();
    tables.sort_unstable();
    assert_eq!(catalog, tables);
}

#[test]
fn field_names_are_unique_per_dataset() {
    for dataset in SPEC_DATASETS {
        for (index, field) in dataset.fields().iter().enumerate() {
            assert!(
                dataset.fields()[index + 1..]
                    .iter()
                    .all(|other| other.name() != field.name()),
                "{} repeats {}",
                dataset.id(),
                field.name()
            );
        }
    }
}

#[test]
fn lookup_finds_fields_and_refuses_strangers() {
    let dataset = spec_dataset("budget_utilization").expect("known");
    assert_eq!(
        dataset.field("used").map(SpecField::value),
        Some(FieldValue::Percent)
    );
    assert!(dataset.field("month").is_none());
    assert!(spec_dataset("nothing").is_none());
    let month = spec_dataset("cash_flow_by_month")
        .and_then(|d| d.field("month"))
        .expect("known");
    assert!(!dataset.has(month));
}

#[test]
fn types_suit_fields_as_the_design_says() {
    use FieldType::*;
    assert!(FieldValue::Amount.suits(Quantitative, false));
    assert!(FieldValue::Percent.suits(Quantitative, false));
    assert!(!FieldValue::Text.suits(Quantitative, false));
    assert!(FieldValue::Text.suits(Quantitative, true));
    assert!(FieldValue::Day.suits(Temporal, false));
    assert!(!FieldValue::Text.suits(Temporal, false));
    assert!(FieldValue::YesNo.suits(Nominal, false));
    assert!(FieldValue::Day.suits(Ordinal, false));
    assert!(!FieldValue::Amount.suits(Nominal, false));
}
