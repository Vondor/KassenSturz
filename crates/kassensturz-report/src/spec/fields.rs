// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fields a chart description can name (D-503, Q-353 = B). A description
//! sees a dataset as a flat table, one row per entry of its result. The ids
//! are those of the catalog; this file does not change the catalog.

use super::model::FieldType;

/// What a field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldValue {
    /// Text.
    Text,
    /// A calendar day.
    Day,
    /// An exact amount of money.
    Amount,
    /// A percentage, exact.
    Percent,
    /// Yes or no.
    YesNo,
}

impl FieldValue {
    /// Whether a description may give this field the type `kind` (section
    /// 2). `with_count` is true when the field is only counted, which suits
    /// any field as a quantity.
    #[must_use]
    pub fn suits(self, kind: FieldType, with_count: bool) -> bool {
        match kind {
            FieldType::Quantitative => {
                with_count || matches!(self, FieldValue::Amount | FieldValue::Percent)
            }
            FieldType::Temporal => self == FieldValue::Day,
            FieldType::Nominal | FieldType::Ordinal => {
                matches!(self, FieldValue::Text | FieldValue::YesNo | FieldValue::Day)
            }
        }
    }
}

/// One field of a dataset's flat table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecField {
    name: &'static str,
    value: FieldValue,
}

impl SpecField {
    const fn new(name: &'static str, value: FieldValue) -> Self {
        Self { name, value }
    }

    /// The name a description writes.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// What the field holds.
    #[must_use]
    pub const fn value(&self) -> FieldValue {
        self.value
    }
}

/// A catalog dataset as a flat table a description can name.
#[derive(Debug, PartialEq, Eq)]
pub struct SpecDataset {
    id: &'static str,
    fields: &'static [SpecField],
}

impl SpecDataset {
    /// The catalog id, the value of `data.name`.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// The fields, in the order of the design table.
    #[must_use]
    pub const fn fields(&self) -> &'static [SpecField] {
        self.fields
    }

    /// The field with this name.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&'static SpecField> {
        self.fields.iter().find(|field| field.name == name)
    }

    /// Whether `field` is one of this dataset's fields.
    #[must_use]
    pub fn has(&self, field: &SpecField) -> bool {
        self.fields.iter().any(|own| own == field)
    }
}

use FieldValue::{Amount, Day, Percent, Text, YesNo};

const fn f(name: &'static str, value: FieldValue) -> SpecField {
    SpecField::new(name, value)
}

/// The seven datasets of the catalog, with their fields.
pub static SPEC_DATASETS: &[SpecDataset] = &[
    SpecDataset {
        id: "spending_by_category",
        fields: &[
            f("category", Text),
            f("top_level", Text),
            f("total", Amount),
            f("currency", Text),
        ],
    },
    SpecDataset {
        id: "income_expense_by_month",
        fields: &[
            f("month", Day),
            f("income", Amount),
            f("expenses", Amount),
            f("net", Amount),
            f("currency", Text),
        ],
    },
    SpecDataset {
        id: "account_balances",
        fields: &[
            f("account", Text),
            f("owed", YesNo),
            f("closed", YesNo),
            f("balance", Amount),
            f("currency", Text),
        ],
    },
    SpecDataset {
        id: "net_worth_over_time",
        fields: &[
            f("month", Day),
            f("assets", Amount),
            f("liabilities", Amount),
            f("net_worth", Amount),
            f("currency", Text),
        ],
    },
    SpecDataset {
        id: "cash_flow_by_month",
        fields: &[
            f("month", Day),
            f("start", Amount),
            f("money_in", Amount),
            f("money_out", Amount),
            f("net_change", Amount),
            f("end", Amount),
            f("currency", Text),
        ],
    },
    SpecDataset {
        id: "category_over_time",
        fields: &[
            f("month", Day),
            f("series", Text),
            f("total", Amount),
            f("currency", Text),
        ],
    },
    SpecDataset {
        id: "budget_utilization",
        fields: &[
            f("category", Text),
            f("every", Text),
            f("first_day", Day),
            f("last_day", Day),
            f("planned", Amount),
            f("carried_in", Amount),
            f("available", Amount),
            f("spent", Amount),
            f("left", Amount),
            f("used", Percent),
            f("carries_over", YesNo),
            f("currency", Text),
        ],
    },
];

/// The dataset a description names in `data.name`.
#[must_use]
pub fn spec_dataset(id: &str) -> Option<&'static SpecDataset> {
    SPEC_DATASETS.iter().find(|dataset| dataset.id == id)
}
