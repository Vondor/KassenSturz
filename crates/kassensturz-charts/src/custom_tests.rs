// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the renderer of custom charts (M6e.2). The descriptions are built
//! by hand from the model's constructors (the parser is another step) and the
//! tables are made from the real output of the datasets on a book, so every
//! chart can be compared with the built-in chart of the same dataset.

use kassensturz_core::{GroupMember, Money, Report, RoundingMode};
use kassensturz_report::spec::{
    Aggregate, Cell, Channel, ChartSpec, Encoding, FieldFilter, FieldType, FilterTest, FlatTable,
    Layer, Mark, MarkKind, PaletteName, Sort, SpecNumber, SpecText, SpecValue, TimeUnit,
    spec_dataset,
};
use kassensturz_report::{
    Filter, Overlap, Source, TextFormat, cash_flow_by_month as cash_flow_dataset,
    category_over_time as over_time_dataset,
};
use proptest::prelude::*;

use crate::tests::{EUR, Silent, Words, autumn, book, date, money, monthly, spending};
use crate::*;

fn field(dataset: &str, name: &str) -> &'static kassensturz_report::spec::SpecField {
    spec_dataset(dataset)
        .and_then(|d| d.field(name))
        .expect("a field")
}

fn plain(dataset: &str, channel: Channel, name: &str, kind: FieldType) -> Encoding {
    Encoding::plain(channel, field(dataset, name), kind).expect("an encoding")
}

fn with(
    dataset: &str,
    channel: Channel,
    name: &str,
    kind: FieldType,
    aggregate: Option<Aggregate>,
    unit: Option<TimeUnit>,
    sort: Option<Sort>,
) -> Encoding {
    Encoding::new(
        channel,
        field(dataset, name),
        kind,
        None,
        aggregate,
        unit,
        sort,
    )
    .expect("an encoding")
}

fn layer(kind: MarkKind, encodings: Vec<Encoding>) -> Layer {
    Layer::new(Mark::plain(kind), encodings, Vec::new()).expect("a layer")
}

fn spec(dataset: &str, layers: Vec<Layer>) -> ChartSpec {
    spec_with(dataset, Vec::new(), layers)
}

fn spec_with(dataset: &str, filters: Vec<FieldFilter>, layers: Vec<Layer>) -> ChartSpec {
    ChartSpec::new(
        spec_dataset(dataset).expect("dataset"),
        None,
        None,
        None,
        Vec::new(),
        filters,
        layers,
    )
    .expect("a spec")
}

fn draw(spec: &ChartSpec, table: &FlatTable) -> CustomChart {
    custom_chart(spec, table, &Words)
        .expect("drawn")
        .expect("something to show")
}

fn amounts(series: &Series) -> Vec<i64> {
    series.points.iter().map(|p| p.minor_units).collect()
}

/// A table of the spending dataset by hand: `(category, amount)`.
fn spending_table(rows: &[(&str, i64)]) -> FlatTable {
    FlatTable::new(
        spec_dataset("spending_by_category").expect("dataset"),
        EUR,
        rows.iter()
            .map(|(name, minor)| {
                vec![
                    Cell::Text((*name).into()),
                    Cell::Text((*name).into()),
                    Cell::Amount(money(*minor)),
                    Cell::Text("EUR".into()),
                ]
            })
            .collect(),
    )
    .expect("table")
}

#[test]
fn a_bar_of_the_months_net_is_the_built_in_chart_of_the_same_numbers() {
    let book = book();
    let data = monthly(&book);
    let table = FlatTable::from_monthly(&data);
    let ds = "income_expense_by_month";
    let bars = spec(
        ds,
        vec![layer(
            MarkKind::Bar,
            vec![
                with(
                    ds,
                    Channel::X,
                    "month",
                    FieldType::Temporal,
                    None,
                    Some(TimeUnit::YearMonth),
                    None,
                ),
                plain(ds, Channel::Y, "net", FieldType::Quantitative),
            ],
        )],
    );
    let custom = draw(&bars, &table);
    let built_in = income_expense_by_month(&data, ChartKind::Bar, &Words).expect("chart");
    let chart = &custom.chart;
    assert_eq!(chart.kind, ChartKind::Bar);
    assert_eq!(chart.categories, built_in.categories);
    assert_eq!(chart.series.len(), 1);
    // The same exact amounts the dataset has, month by month.
    let net: Vec<i64> = data.rows.iter().map(|row| row.net.minor_units()).collect();
    assert_eq!(amounts(&chart.series[0]), net);
    let labels: Vec<&str> = chart.series[0]
        .points
        .iter()
        .map(|p| p.label.as_str())
        .collect();
    let written: Vec<String> = data.rows.iter().map(|r| Words.amount(r.net)).collect();
    assert_eq!(labels, written);
    assert_eq!(chart.series[0].name.as_deref(), Some("net"));
    assert_eq!(chart.dataset, "income_expense_by_month");
    assert_eq!(chart.currency, "EUR");
    assert!(chart.total.is_empty() && chart.overlays.is_empty());
    assert!(chart.ticks.len() >= 2);
    assert_eq!(chart.table.columns, ["category", "series", "value"]);
    assert_eq!(chart.table.rows.len(), 2);
    assert_eq!(chart.table.rows[0], ["2026-09", "net", written[0].as_str()]);
}

#[test]
fn a_bar_of_spending_keeps_the_datasets_order_the_unsorted_place_and_its_labels() {
    let book = book();
    let data = spending(&book);
    let table = FlatTable::from_spending(&data);
    let ds = "spending_by_category";
    let bars = spec(
        ds,
        vec![layer(
            MarkKind::Bar,
            vec![
                plain(ds, Channel::X, "category", FieldType::Nominal),
                plain(ds, Channel::Y, "total", FieldType::Quantitative),
            ],
        )],
    );
    let chart = draw(&bars, &table).chart;
    let built_in = spending_by_category(&data, ChartKind::Bar, &Words).expect("chart");
    // The unsorted money is named by the format, as in the built-in chart (D-034).
    assert_eq!(chart.categories, ["Rent", "Food › Groceries", "(unsorted)"]);
    assert_eq!(chart.unsorted, built_in.unsorted);
    assert_eq!(amounts(&chart.series[0]), amounts(&built_in.series[0]));
    let labels = |c: &ChartModel| -> Vec<String> {
        c.series[0].points.iter().map(|p| p.label.clone()).collect()
    };
    assert_eq!(labels(&chart), labels(&built_in));
}

#[test]
fn a_pie_and_a_donut_have_the_slices_of_the_built_in_one() {
    let book = book();
    let data = spending(&book);
    let table = FlatTable::from_spending(&data);
    let ds = "spending_by_category";
    for radius in [None, Some(40)] {
        let arc = Layer::new(
            Mark::new(MarkKind::Arc, false, false, radius).expect("mark"),
            vec![
                plain(ds, Channel::Theta, "total", FieldType::Quantitative),
                plain(ds, Channel::Color, "category", FieldType::Nominal),
            ],
            Vec::new(),
        )
        .expect("layer");
        let chart = draw(&spec(ds, vec![arc]), &table).chart;
        let built_in = spending_by_category(
            &data,
            if radius.is_some() {
                ChartKind::Donut
            } else {
                ChartKind::Pie
            },
            &Words,
        )
        .expect("chart");
        assert_eq!(chart.kind, built_in.kind);
        assert_eq!(chart.categories, built_in.categories);
        assert_eq!(chart.slices, built_in.slices);
        assert_eq!(amounts(&chart.series[0]), amounts(&built_in.series[0]));
        assert_eq!(chart.table, built_in.table);
        assert!(chart.ticks.is_empty());
    }
}

#[test]
fn more_than_eight_slices_become_other_and_more_than_fifty_are_refused() {
    let ds = "spending_by_category";
    let arc = || {
        Layer::new(
            Mark::plain(MarkKind::Arc),
            vec![
                plain(ds, Channel::Theta, "total", FieldType::Quantitative),
                plain(ds, Channel::Color, "category", FieldType::Nominal),
            ],
            Vec::new(),
        )
        .expect("layer")
    };
    let names: Vec<String> = (0..12).map(|n| format!("c{n:02}")).collect();
    let rows: Vec<(&str, i64)> = names
        .iter()
        .enumerate()
        .map(|(n, name)| (name.as_str(), 1_000 - i64::try_from(n).expect("small") * 10))
        .collect();
    let chart = draw(&spec(ds, vec![arc()]), &spending_table(&rows)).chart;
    assert_eq!(chart.categories.len(), 8);
    assert_eq!(chart.slices.other, Some(7));
    assert_eq!(chart.slices.holds.len(), 5);
    // Other is the sum of what it holds, exactly.
    let rest: i64 = (7..12).map(|n| 1_000 - n * 10).sum();
    assert_eq!(chart.series[0].points[7].minor_units, rest);

    let many: Vec<String> = (0..51).map(|n| format!("c{n:02}")).collect();
    let rows: Vec<(&str, i64)> = many.iter().map(|name| (name.as_str(), 100)).collect();
    assert_eq!(
        custom_chart(&spec(ds, vec![arc()]), &spending_table(&rows), &Words),
        Err(CustomError::TooBig("series"))
    );
}

#[test]
fn colored_bars_stack_and_are_the_built_in_stacked_bars_of_the_same_series() {
    let book = book();
    let source = Source::read(&book, autumn()).expect("source");
    let ids = [
        GroupMember::Category(find(&book, "Rent")),
        GroupMember::Category(find(&book, "Groceries")),
    ];
    let data = over_time_dataset(
        &source,
        autumn(),
        &Filter::default(),
        &ids,
        Overlap::Allowed,
    )
    .expect("dataset")
    .remove(0);
    let table = FlatTable::from_over_time(&data);
    let ds = "category_over_time";
    let stacked = spec(
        ds,
        vec![layer(
            MarkKind::Bar,
            vec![
                with(
                    ds,
                    Channel::X,
                    "month",
                    FieldType::Ordinal,
                    None,
                    None,
                    None,
                ),
                plain(ds, Channel::Y, "total", FieldType::Quantitative),
                plain(ds, Channel::Color, "series", FieldType::Nominal),
            ],
        )],
    );
    let chart = draw(&stacked, &table).chart;
    let built_in = category_over_time(&data, ChartKind::StackedBar, &Words).expect("chart");
    assert_eq!(chart.kind, ChartKind::StackedBar);
    // A field called `month` is read as months when the description names no time unit.
    assert_eq!(chart.categories, ["2026-09", "2026-10"]);
    assert_eq!(chart.series.len(), 2);
    for (mine, theirs) in chart.series.iter().zip(&built_in.series) {
        assert_eq!(mine.name, theirs.name);
        assert_eq!(mine.color, theirs.color);
        assert_eq!(amounts(mine), amounts(theirs));
    }
    // The area version stacks too.
    let area = spec(
        ds,
        vec![layer(
            MarkKind::Area,
            vec![
                with(
                    ds,
                    Channel::X,
                    "month",
                    FieldType::Temporal,
                    None,
                    Some(TimeUnit::YearMonth),
                    None,
                ),
                plain(ds, Channel::Y, "total", FieldType::Quantitative),
                plain(ds, Channel::Color, "series", FieldType::Nominal),
            ],
        )],
    );
    assert_eq!(draw(&area, &table).chart.kind, ChartKind::StackedArea);
}

fn find(book: &kassensturz_store::Book, name: &str) -> kassensturz_core::AccountId {
    book.all_accounts()
        .expect("accounts")
        .into_iter()
        .find(|account| account.name == name)
        .expect("account")
        .id
}

#[test]
fn a_series_missing_at_a_place_is_a_dash_and_zero_not_a_made_up_figure() {
    let ds = "category_over_time";
    let dataset = spec_dataset(ds).expect("dataset");
    let row = |m: i8, series: &str, minor: i64| {
        vec![
            Cell::Day(date(2026, m, 1)),
            Cell::Text(series.into()),
            Cell::Amount(money(minor)),
            Cell::Text("EUR".into()),
        ]
    };
    let table = FlatTable::new(
        dataset,
        EUR,
        vec![row(1, "A", 100), row(2, "A", 200), row(2, "B", 50)],
    )
    .expect("table");
    let lines = spec(
        ds,
        vec![layer(
            MarkKind::Line,
            vec![
                with(
                    ds,
                    Channel::X,
                    "month",
                    FieldType::Temporal,
                    None,
                    None,
                    None,
                ),
                plain(ds, Channel::Y, "total", FieldType::Quantitative),
                plain(ds, Channel::Color, "series", FieldType::Nominal),
            ],
        )],
    );
    let chart = draw(&lines, &table).chart;
    assert_eq!(chart.kind, ChartKind::Line);
    assert_eq!(chart.series[1].points[0].label, "–");
    assert_eq!(chart.series[1].points[0].minor_units, 0);
    assert_eq!(amounts(&chart.series[1]), [0, 50]);
    // The table lists only what exists.
    assert_eq!(chart.table.rows.len(), 3);
}

#[test]
fn aggregates_are_exact_and_the_mean_follows_the_books_rounding() {
    let table = spending_table(&[("Food", 100), ("Food", 101), ("Food", 101), ("Rent", 7)]);
    let ds = "spending_by_category";
    let by = |aggregate| {
        spec(
            ds,
            vec![layer(
                MarkKind::Bar,
                vec![
                    plain(ds, Channel::X, "category", FieldType::Nominal),
                    with(
                        ds,
                        Channel::Y,
                        "total",
                        FieldType::Quantitative,
                        aggregate,
                        None,
                        None,
                    ),
                ],
            )],
        )
    };
    let values = |aggregate| amounts(&draw(&by(aggregate), &table).chart.series[0]);
    assert_eq!(values(None), [302, 7], "added up by default");
    assert_eq!(values(Some(Aggregate::Sum)), [302, 7]);
    assert_eq!(values(Some(Aggregate::Min)), [100, 7]);
    assert_eq!(values(Some(Aggregate::Max)), [101, 7]);
    // 302 / 3 = 100.67: half away from zero gives 101, toward zero 100.
    assert_eq!(values(Some(Aggregate::Mean)), [101, 7]);
    let floor = table
        .clone()
        .with_book_settings(2, RoundingMode::TowardZero);
    assert_eq!(
        amounts(&draw(&by(Some(Aggregate::Mean)), &floor).chart.series[0]),
        [100, 7]
    );
    // A count is a number, not money.
    let counted = draw(&by(Some(Aggregate::Count)), &table).chart;
    assert_eq!(amounts(&counted.series[0]), [300, 100]);
    assert_eq!(counted.series[0].points[0].label, "300 hundredths");
    assert_eq!(counted.ticks[1].label, "100 hundredths");
}

#[test]
fn filters_compare_exactly_and_include_both_ends() {
    let table = spending_table(&[("A", 5_320), ("B", 5_319), ("C", 5_321), ("D", 100)]);
    let ds = "spending_by_category";
    let chart_of = |filter: FieldFilter| {
        let spec = spec_with(
            ds,
            vec![filter],
            vec![layer(
                MarkKind::Bar,
                vec![
                    plain(ds, Channel::X, "category", FieldType::Nominal),
                    plain(ds, Channel::Y, "total", FieldType::Quantitative),
                ],
            )],
        );
        custom_chart(&spec, &table, &Words)
            .expect("drawn")
            .map(|c| c.chart.categories)
    };
    let number = |text: &str| SpecValue::Number(SpecNumber::parse(text).expect("number"));
    let total = field(ds, "total");
    let range = |from: Option<&str>, to: Option<&str>| {
        FieldFilter::new(
            total,
            FilterTest::Range {
                from: from.map(number),
                to: to.map(number),
            },
        )
        .expect("filter")
    };
    assert_eq!(
        chart_of(range(Some("53.20"), None)),
        Some(vec!["A".into(), "C".into()])
    );
    assert_eq!(
        chart_of(range(None, Some("53.20"))),
        Some(vec!["A".into(), "B".into(), "D".into()])
    );
    // 53.205 is finer than the currency: it is compared as it is, never rounded.
    assert_eq!(
        chart_of(range(Some("53.205"), None)),
        Some(vec!["C".into()])
    );
    assert_eq!(
        chart_of(range(Some("53.2"), Some("53.2"))),
        Some(vec!["A".into()])
    );
    // Text filters.
    let text = |t: &str| SpecValue::Text(SpecText::new(t).expect("text"));
    let name = field(ds, "category");
    let one_of =
        FieldFilter::new(name, FilterTest::OneOf(vec![text("B"), text("D")])).expect("filter");
    assert_eq!(chart_of(one_of), Some(vec!["B".into(), "D".into()]));
    // Nothing left: the screen says there's nothing to show.
    let none = FieldFilter::new(name, FilterTest::Equal(text("Z"))).expect("filter");
    assert_eq!(chart_of(none), None);
}

#[test]
fn a_yes_no_filter_takes_true_and_false() {
    let ds = "account_balances";
    let dataset = spec_dataset(ds).expect("dataset");
    let row = |name: &str, owed: bool, minor: i64| {
        vec![
            Cell::Text(name.into()),
            Cell::YesNo(owed),
            Cell::YesNo(false),
            Cell::Amount(money(minor)),
            Cell::Text("EUR".into()),
        ]
    };
    let table = FlatTable::new(
        dataset,
        EUR,
        vec![row("Card", true, 300), row("Bank", false, 900)],
    )
    .expect("table");
    let only_owed = FieldFilter::new(field(ds, "owed"), FilterTest::Equal(SpecValue::YesNo(true)))
        .expect("filter");
    let spec = spec_with(
        ds,
        vec![only_owed],
        vec![layer(
            MarkKind::Bar,
            vec![
                plain(ds, Channel::X, "account", FieldType::Nominal),
                plain(ds, Channel::Y, "balance", FieldType::Quantitative),
            ],
        )],
    );
    assert_eq!(draw(&spec, &table).chart.categories, ["Card"]);
}

#[test]
fn layers_put_a_level_a_dots_and_texts_over_the_bars_on_one_axis() {
    let book = book();
    let data = monthly(&book);
    let table = FlatTable::from_monthly(&data);
    let ds = "income_expense_by_month";
    let x = || {
        with(
            ds,
            Channel::X,
            "month",
            FieldType::Temporal,
            None,
            Some(TimeUnit::YearMonth),
            None,
        )
    };
    let spec = spec(
        ds,
        vec![
            layer(
                MarkKind::Bar,
                vec![x(), plain(ds, Channel::Y, "net", FieldType::Quantitative)],
            ),
            layer(
                MarkKind::Rule,
                vec![with(
                    ds,
                    Channel::Y,
                    "net",
                    FieldType::Quantitative,
                    Some(Aggregate::Mean),
                    None,
                    None,
                )],
            ),
            layer(
                MarkKind::Point,
                vec![
                    x(),
                    plain(ds, Channel::Y, "income", FieldType::Quantitative),
                ],
            ),
            layer(
                MarkKind::Text,
                vec![plain(ds, Channel::Text, "currency", FieldType::Nominal)],
            ),
        ],
    );
    let chart = draw(&spec, &table).chart;
    assert_eq!(chart.kind, ChartKind::Bar);
    assert_eq!(chart.series.len(), 1);
    let marks: Vec<OverlayMark> = chart.overlays.iter().map(|o| o.mark).collect();
    assert_eq!(
        marks,
        [
            OverlayMark::LevelRule,
            OverlayMark::Point,
            OverlayMark::Text
        ]
    );
    // The level is the exact mean of the net, rounded once.
    let net: i64 = data.rows.iter().map(|r| r.net.minor_units()).sum();
    let level = &chart.overlays[0].series[0].items[0];
    assert_eq!(level.category, None);
    assert_eq!(level.minor_units, Some((net + 1) / 2));
    // The dots sit at each month's income; the texts inherit x and y of the dots.
    let dots = &chart.overlays[1].series[0].items;
    let income: Vec<i64> = data.rows.iter().map(|r| r.income.minor_units()).collect();
    assert_eq!(
        dots.iter()
            .filter_map(|d| d.minor_units)
            .collect::<Vec<_>>(),
        income
    );
    assert_eq!(
        dots.iter().filter_map(|d| d.category).collect::<Vec<_>>(),
        [0, 1]
    );
    let texts = &chart.overlays[2].series[0].items;
    assert_eq!(
        texts.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(),
        ["EUR", "EUR"]
    );
    assert_eq!(
        texts
            .iter()
            .filter_map(|d| d.minor_units)
            .collect::<Vec<_>>(),
        income
    );
    // The axis reaches the highest of every layer.
    let top = chart.ticks.last().expect("ticks").minor_units;
    assert!(top >= income.iter().copied().max().expect("income"));
    // Series of different layers have different colors.
    let colors = [
        chart.series[0].color,
        chart.overlays[0].series[0].color,
        chart.overlays[1].series[0].color,
    ];
    assert_eq!(colors, [0, 1, 2]);
    // And the drawing values of the overlays are made by the provider.
    let prepared = Silent.prepare(&chart).expect("prepared");
    assert_eq!(prepared.overlays.len(), 3);
    assert_eq!(prepared.overlays[1][0].len(), 2);
    assert_eq!(prepared.overlays[1][0][0], Some(income[0] as f64));
}

#[test]
fn a_rule_at_a_place_runs_bottom_to_top_and_a_description_of_marks_alone_still_draws() {
    let book = book();
    let table = FlatTable::from_monthly(&monthly(&book));
    let ds = "income_expense_by_month";
    let april = FieldFilter::new(
        field(ds, "month"),
        FilterTest::Equal(SpecValue::Day(date(2026, 10, 1))),
    )
    .expect("filter");
    let x = || {
        with(
            ds,
            Channel::X,
            "month",
            FieldType::Temporal,
            None,
            Some(TimeUnit::YearMonth),
            None,
        )
    };
    let marks = Layer::new(
        Mark::plain(MarkKind::Point),
        vec![x(), plain(ds, Channel::Y, "net", FieldType::Quantitative)],
        Vec::new(),
    )
    .expect("layer");
    let rule = Layer::new(Mark::plain(MarkKind::Rule), vec![x()], vec![april]).expect("layer");
    let chart = draw(&spec(ds, vec![marks, rule]), &table).chart;
    // No line, bar or area: an empty bar chart carries the overlays.
    assert_eq!(chart.kind, ChartKind::Bar);
    assert!(chart.series.is_empty());
    let item = &chart.overlays[1].series[0].items[0];
    assert_eq!((item.category, item.minor_units), (Some(1), None));
    assert_eq!(chart.overlays[1].mark, OverlayMark::CategoryRule);
}

#[test]
fn layers_that_disagree_about_the_values_are_refused() {
    let book = book();
    let table = FlatTable::from_spending(&spending(&book));
    let ds = "spending_by_category";
    let spec = spec(
        ds,
        vec![
            layer(
                MarkKind::Bar,
                vec![
                    plain(ds, Channel::X, "category", FieldType::Nominal),
                    plain(ds, Channel::Y, "total", FieldType::Quantitative),
                ],
            ),
            layer(
                MarkKind::Rule,
                vec![with(
                    ds,
                    Channel::Y,
                    "total",
                    FieldType::Quantitative,
                    Some(Aggregate::Count),
                    None,
                    None,
                )],
            ),
        ],
    );
    assert_eq!(
        custom_chart(&spec, &table, &Words),
        Err(CustomError::Invalid("layers-differ"))
    );
}

#[test]
fn percentages_are_not_added_up_but_axis_and_labels_are_percentages() {
    let ds = "budget_utilization";
    let dataset = spec_dataset(ds).expect("dataset");
    let row = |name: &str, used: Option<i64>| {
        let zero = Cell::Amount(money(0));
        vec![
            Cell::Text(name.into()),
            Cell::Text("month".into()),
            Cell::Day(date(2026, 10, 1)),
            Cell::Day(date(2026, 10, 31)),
            zero.clone(),
            zero.clone(),
            zero.clone(),
            zero.clone(),
            zero,
            Cell::Percent(used.map(kassensturz_report::Percent::from_tenths)),
            Cell::YesNo(false),
            Cell::Text("EUR".into()),
        ]
    };
    let table = FlatTable::new(
        dataset,
        EUR,
        vec![
            row("Food", Some(250)),
            row("Rent", Some(1_000)),
            row("Fun", None),
        ],
    )
    .expect("table");
    let bars = spec(
        ds,
        vec![layer(
            MarkKind::Bar,
            vec![
                plain(ds, Channel::X, "category", FieldType::Nominal),
                plain(ds, Channel::Y, "used", FieldType::Quantitative),
            ],
        )],
    );
    let chart = draw(&bars, &table).chart;
    assert_eq!(chart.categories, ["Food", "Rent", "Fun"]);
    assert_eq!(amounts(&chart.series[0]), [250, 1_000, 0]);
    assert_eq!(
        chart.series[0].points[2].label, "–",
        "nothing was available to compare with"
    );
    assert_eq!(chart.series[0].points[1].label, "1000 tenths");
    assert_eq!(chart.ticks.last().expect("ticks").label, "1000 tenths");
    // Two rows on one place give no sum.
    let twice = FlatTable::new(
        dataset,
        EUR,
        vec![row("Food", Some(250)), row("Food", Some(100))],
    )
    .expect("table");
    assert_eq!(
        custom_chart(&bars, &twice, &Words),
        Err(CustomError::Invalid("bad-aggregate"))
    );
}

#[test]
fn sorting_by_the_value_or_another_field_orders_the_places() {
    let table = spending_table(&[("b", 30), ("c", 10), ("a", 20)]);
    let ds = "spending_by_category";
    let order = |sort: Option<Sort>| {
        let spec = spec(
            ds,
            vec![layer(
                MarkKind::Bar,
                vec![
                    with(
                        ds,
                        Channel::X,
                        "category",
                        FieldType::Nominal,
                        None,
                        None,
                        sort,
                    ),
                    plain(ds, Channel::Y, "total", FieldType::Quantitative),
                ],
            )],
        );
        draw(&spec, &table).chart.categories
    };
    assert_eq!(order(None), ["b", "c", "a"], "the dataset's own order");
    assert_eq!(order(Some(Sort::Ascending)), ["a", "b", "c"]);
    assert_eq!(order(Some(Sort::Descending)), ["c", "b", "a"]);
    assert_eq!(
        order(Some(Sort::ByField(field(ds, "total")))),
        ["c", "a", "b"]
    );
}

#[test]
fn the_palette_of_the_description_colors_the_series() {
    let table = spending_table(&[("a", 1), ("b", 2)]);
    let ds = "spending_by_category";
    let by_color = layer(
        MarkKind::Bar,
        vec![
            plain(ds, Channel::X, "top_level", FieldType::Nominal),
            plain(ds, Channel::Y, "total", FieldType::Quantitative),
            plain(ds, Channel::Color, "category", FieldType::Nominal),
        ],
    );
    let spec = ChartSpec::new(
        spec_dataset(ds).expect("dataset"),
        Some(SpecText::new("Title").expect("text")),
        None,
        None,
        vec![PaletteName::ALL[5], PaletteName::ALL[2]],
        Vec::new(),
        vec![by_color],
    )
    .expect("spec");
    let custom = draw(&spec, &table);
    assert_eq!(custom.title.as_deref(), Some("Title"));
    let colors: Vec<u8> = custom.chart.series.iter().map(|s| s.color).collect();
    assert_eq!(colors, [5, 2]);
}

#[test]
fn too_many_rows_or_amounts_that_overflow_are_errors_not_partial_charts() {
    let ds = "spending_by_category";
    let spec = spec(
        ds,
        vec![layer(
            MarkKind::Bar,
            vec![
                plain(ds, Channel::X, "category", FieldType::Nominal),
                plain(ds, Channel::Y, "total", FieldType::Quantitative),
            ],
        )],
    );
    let many: Vec<(&str, i64)> = vec![("a", 1); MAX_ROWS + 1];
    assert_eq!(
        custom_chart(&spec, &spending_table(&many), &Words),
        Err(CustomError::TooBig("rows"))
    );
    let exactly: Vec<(&str, i64)> = vec![("a", 1); MAX_ROWS];
    assert!(custom_chart(&spec, &spending_table(&exactly), &Words).is_ok());
    let huge = spending_table(&[("a", i64::MAX), ("a", 1)]);
    assert_eq!(
        custom_chart(&spec, &huge, &Words),
        Err(CustomError::Chart(ChartError::Overflow))
    );
}

#[test]
fn a_chart_never_draws_more_points_than_it_may_use_rows() {
    // 5,001 places with 3 series: 15,003 points from 5,001 rows.
    let ds = "category_over_time";
    let dataset = spec_dataset(ds).expect("dataset");
    let mut days = vec![date(2000, 1, 1)];
    while days.len() < 5_001 {
        days.push(days.last().expect("a day").tomorrow().expect("next day"));
    }
    let row = |place: usize, series: &str| {
        vec![
            Cell::Day(days[place]),
            Cell::Text(series.into()),
            Cell::Amount(money(1)),
            Cell::Text("EUR".into()),
        ]
    };
    let mut rows: Vec<Vec<Cell>> = (0..5_001).map(|n| row(n, "A")).collect();
    rows.push(row(0, "B"));
    rows.push(row(0, "C"));
    let table = FlatTable::new(dataset, EUR, rows).expect("table");
    let colored = |color: bool| {
        let mut encodings = vec![
            with(
                ds,
                Channel::X,
                "month",
                FieldType::Temporal,
                None,
                Some(TimeUnit::YearMonthDate),
                None,
            ),
            plain(ds, Channel::Y, "total", FieldType::Quantitative),
        ];
        if color {
            encodings.push(plain(ds, Channel::Color, "series", FieldType::Nominal));
        }
        spec(ds, vec![layer(MarkKind::Bar, encodings)])
    };
    assert_eq!(
        custom_chart(&colored(true), &table, &Words),
        Err(CustomError::TooBig("rows"))
    );
    // One series of 5,001 points is fine.
    assert!(custom_chart(&colored(false), &table, &Words).is_ok());
}

#[test]
fn a_description_of_another_dataset_is_refused() {
    let ds = "income_expense_by_month";
    let spec = spec(
        ds,
        vec![layer(
            MarkKind::Bar,
            vec![
                plain(ds, Channel::X, "month", FieldType::Ordinal),
                plain(ds, Channel::Y, "net", FieldType::Quantitative),
            ],
        )],
    );
    assert_eq!(
        custom_chart(&spec, &spending_table(&[("a", 1)]), &Words),
        Err(CustomError::Invalid("unknown-dataset"))
    );
}

#[test]
fn the_cash_flow_months_are_labeled_as_months() {
    let book = book();
    let source = Source::read_with_balances(&book, autumn()).expect("source");
    let flows = cash_flow_dataset(&source, autumn(), &Filter::default()).expect("flow");
    let table = FlatTable::from_cash_flow(&flows[0]);
    let ds = "cash_flow_by_month";
    let lines = spec(
        ds,
        vec![layer(
            MarkKind::Line,
            vec![
                plain(ds, Channel::X, "month", FieldType::Temporal),
                plain(ds, Channel::Y, "end", FieldType::Quantitative),
            ],
        )],
    );
    let chart = draw(&lines, &table).chart;
    assert_eq!(chart.categories, ["2026-09", "2026-10"]);
    let end: Vec<i64> = flows[0].rows.iter().map(|r| r.end.minor_units()).collect();
    assert_eq!(amounts(&chart.series[0]), end);
    let _ = Money::zero(EUR);
}

// ---- Where the parser and the renderer meet (M6e.2) -----------------------
//
// The descriptions below are JSON written by hand for these tests (D-280),
// read by the real parser (`spec::read`), and drawn from the tables of a real
// book, so a field the parser leaves out or a shape the renderer doesn't
// expect shows up here and not on the screen.

/// What the parser makes of `text`, with nothing left out.
fn parsed(text: &str) -> ChartSpec {
    let read = kassensturz_report::spec::read(text).expect("read");
    assert!(read.unsupported().is_empty(), "{:?}", read.unsupported());
    read.chart().cloned().expect("a chart")
}

#[test]
fn a_description_read_by_the_parser_draws_the_same_chart_as_the_one_built_by_hand() {
    let book = book();
    let data = monthly(&book);
    let table = FlatTable::from_monthly(&data);
    let ds = "income_expense_by_month";
    let by_hand = spec(
        ds,
        vec![layer(
            MarkKind::Bar,
            vec![
                with(
                    ds,
                    Channel::X,
                    "month",
                    FieldType::Temporal,
                    None,
                    Some(TimeUnit::YearMonth),
                    None,
                ),
                plain(ds, Channel::Y, "net", FieldType::Quantitative),
            ],
        )],
    );
    let read = parsed(
        r#"{
          "$schema": "https://vega.github.io/schema/vega-lite/v6.json",
          "data": {"name": "income_expense_by_month"},
          "title": "Net by month",
          "mark": "bar",
          "encoding": {
            "x": {"field": "month", "type": "temporal", "timeUnit": "yearmonth"},
            "y": {"field": "net", "type": "quantitative"}
          }
        }"#,
    );
    let drawn = draw(&read, &table);
    assert_eq!(drawn.chart, draw(&by_hand, &table).chart);
    assert_eq!(drawn.title.as_deref(), Some("Net by month"));
}

#[test]
fn layers_a_filter_and_a_sort_from_the_parser_reach_the_chart() {
    let book = book();
    let data = monthly(&book);
    let table = FlatTable::from_monthly(&data);
    let read = parsed(
        r#"{
          "data": {"name": "income_expense_by_month"},
          "transform": [{"filter": {"field": "currency", "equal": "EUR"}}],
          "layer": [
            {"mark": "bar", "encoding": {
              "x": {"field": "month", "type": "temporal", "timeUnit": "yearmonth"},
              "y": {"field": "net", "type": "quantitative"}}},
            {"mark": "rule", "encoding": {
              "y": {"field": "net", "type": "quantitative", "aggregate": "mean"}}},
            {"mark": "point", "encoding": {
              "x": {"field": "month", "type": "temporal", "timeUnit": "yearmonth"},
              "y": {"field": "income", "type": "quantitative"}}}
          ]
        }"#,
    );
    let chart = draw(&read, &table).chart;
    assert_eq!(chart.kind, ChartKind::Bar);
    assert_eq!(chart.series.len(), 1);
    assert_eq!(chart.overlays.len(), 2);
    let net: Vec<i64> = data.rows.iter().map(|row| row.net.minor_units()).collect();
    assert_eq!(amounts(&chart.series[0]), net);
    // The mean is the rounded exact mean of the net amounts, at the level the
    // rule sits.
    let sum: i64 = net.iter().sum();
    let count = i64::try_from(net.len()).expect("small");
    let level = chart
        .overlays
        .iter()
        .find(|overlay| overlay.mark == OverlayMark::LevelRule)
        .expect("a level");
    let drawn_level = level.series[0].items[0].minor_units.expect("an amount");
    assert!((drawn_level * count - sum).abs() <= count, "{drawn_level}");
}

#[test]
fn a_filter_range_read_by_the_parser_includes_both_ends_exactly() {
    let table = spending_table(&[("a", 1_000), ("b", 1_250), ("c", 2_000), ("d", 2_001)]);
    let read = parsed(
        r#"{
          "data": {"name": "spending_by_category"},
          "transform": [{"filter": {"field": "total", "range": ["12.5", "20.00"]}}],
          "mark": "bar",
          "encoding": {"x": {"field": "category", "type": "nominal"},
                       "y": {"field": "total", "type": "quantitative"}}
        }"#,
    );
    let chart = draw(&read, &table).chart;
    assert_eq!(chart.categories, ["b", "c"]);
    assert_eq!(amounts(&chart.series[0]), [1_250, 2_000]);
}

#[test]
fn a_pie_of_the_parser_has_the_slices_of_the_built_in_one() {
    let book = book();
    let data = spending(&book);
    let table = FlatTable::from_spending(&data);
    let read = parsed(
        r#"{
          "data": {"name": "spending_by_category"},
          "mark": {"type": "arc", "innerRadius": 50},
          "encoding": {"theta": {"field": "total", "type": "quantitative"},
                       "color": {"field": "category", "type": "nominal"}}
        }"#,
    );
    let chart = draw(&read, &table).chart;
    assert_eq!(chart.kind, ChartKind::Donut);
    let built_in = spending_by_category(&data, ChartKind::Donut, &Words).expect("chart");
    assert_eq!(chart.categories, built_in.categories);
    assert_eq!(chart.slices, built_in.slices);
}

#[test]
fn what_the_parser_leaves_out_does_not_stop_a_chart_from_being_drawn() {
    let table = spending_table(&[("a", 1_000), ("b", 500)]);
    let text = r#"{
      "data": {"name": "spending_by_category"},
      "mark": {"type": "bar", "tooltip": true, "cornerRadius": 3},
      "encoding": {"x": {"field": "category", "type": "nominal"},
                   "y": {"field": "total", "type": "quantitative"}}
    }"#;
    let read = kassensturz_report::spec::read(text).expect("read");
    assert!(!read.unsupported().is_empty());
    let spec = read.chart().cloned().expect("still a chart");
    let chart = draw(&spec, &table).chart;
    assert_eq!(amounts(&chart.series[0]), [1_000, 500]);
}

#[test]
fn the_errors_of_the_renderer_are_the_reports_the_parser_uses() {
    let names: Vec<String> = (0..51).map(|n| format!("c{n:02}")).collect();
    let rows: Vec<(&str, i64)> = names.iter().map(|name| (name.as_str(), 100)).collect();
    let read = parsed(
        r#"{
          "data": {"name": "spending_by_category"},
          "mark": "arc",
          "encoding": {"theta": {"field": "total", "type": "quantitative"},
                       "color": {"field": "category", "type": "nominal"}}
        }"#,
    );
    let error = custom_chart(&read, &spending_table(&rows), &Words).expect_err("too many");
    let report = error.report();
    assert_eq!(report.code, "report.spec-too-big");
    assert_eq!(
        report.code,
        kassensturz_report::ReportError::SpecTooBig { limit: "series" }
            .report()
            .code
    );
    assert!(
        report
            .params
            .iter()
            .any(|(_, value)| format!("{value:?}").contains("series"))
    );

    let wrong = parsed(
        r#"{"data": {"name": "income_expense_by_month"}, "mark": "bar",
            "encoding": {"x": {"field": "month", "type": "temporal"},
                         "y": {"field": "net", "type": "quantitative"}}}"#,
    );
    let error = custom_chart(&wrong, &spending_table(&[("a", 100)]), &Words).expect_err("wrong");
    assert_eq!(error.report().code, "report.spec-invalid");
}

proptest! {
    /// The bars of a sum add up to exactly the amounts of the rows, in any
    /// grouping.
    #[test]
    fn the_sum_of_the_bars_is_the_sum_of_the_rows(
        rows in proptest::collection::vec((0usize..4, -1_000_000i64..1_000_000), 1..60)
    ) {
        let names = ["a", "b", "c", "d"];
        let pairs: Vec<(&str, i64)> = rows.iter().map(|(n, v)| (names[*n], *v)).collect();
        let table = spending_table(&pairs);
        let ds = "spending_by_category";
        let spec = spec(ds, vec![layer(MarkKind::Bar, vec![
            plain(ds, Channel::X, "category", FieldType::Nominal),
            plain(ds, Channel::Y, "total", FieldType::Quantitative),
        ])]);
        let chart = draw(&spec, &table).chart;
        let drawn: i64 = chart.series[0].points.iter().map(|p| p.minor_units).sum();
        let given: i64 = pairs.iter().map(|(_, v)| *v).sum();
        prop_assert_eq!(drawn, given);
        // No place is listed twice.
        let mut seen = chart.categories.clone();
        seen.sort();
        seen.dedup();
        prop_assert_eq!(seen.len(), chart.categories.len());
    }
}

/// A yes/no field on an axis or a color is written as marks, never as an
/// English word, and the screen's words go in their place everywhere the
/// chart holds a name (D-525); numbers are untouched.
#[test]
fn a_yes_no_field_is_marked_and_the_screens_words_go_in_its_place() {
    let ds = "account_balances";
    let dataset = spec_dataset(ds).expect("dataset");
    let row = |name: &str, owed: bool, minor: i64| {
        vec![
            Cell::Text(name.into()),
            Cell::YesNo(owed),
            Cell::YesNo(false),
            Cell::Amount(money(minor)),
            Cell::Text("EUR".into()),
        ]
    };
    let table = FlatTable::new(
        dataset,
        EUR,
        vec![
            row("Card", true, 300),
            row("Bank", false, 900),
            row("Cash", false, 100),
        ],
    )
    .expect("table");
    let by_answer = spec_with(
        ds,
        Vec::new(),
        vec![layer(
            MarkKind::Bar,
            vec![
                plain(ds, Channel::X, "owed", FieldType::Nominal),
                plain(ds, Channel::Y, "balance", FieldType::Quantitative),
                plain(ds, Channel::Color, "owed", FieldType::Nominal),
            ],
        )],
    );
    let mut chart = draw(&by_answer, &table).chart;
    assert_eq!(
        chart.categories,
        [ANSWER_YES.to_string(), ANSWER_NO.to_string()]
    );
    assert!(chart.series.iter().all(|s| {
        s.name
            .as_deref()
            .is_some_and(|n| n.contains([ANSWER_YES, ANSWER_NO]))
    }));
    let first_column: Vec<String> = chart.table.rows.iter().map(|r| r[0].clone()).collect();
    assert!(
        first_column
            .iter()
            .all(|c| c.contains([ANSWER_YES, ANSWER_NO]))
    );
    let amounts_before: Vec<Vec<i64>> = chart.series.iter().map(amounts).collect();
    chart.write_answers("ja", "nein");
    assert_eq!(chart.categories, ["ja", "nein"]);
    assert!(
        chart
            .series
            .iter()
            .all(|s| matches!(s.name.as_deref(), Some("ja" | "nein")))
    );
    assert!(
        chart
            .table
            .rows
            .iter()
            .all(|r| matches!(r[0].as_str(), "ja" | "nein"))
    );
    let amounts_after: Vec<Vec<i64>> = chart.series.iter().map(amounts).collect();
    assert_eq!(amounts_before, amounts_after);
}
