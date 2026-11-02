// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the chart model's constructors, with the real field table.

use std::cmp::Ordering;

use proptest::prelude::*;

use super::*;
use crate::tests::date;

fn field(dataset: &str, name: &str) -> &'static SpecField {
    spec_dataset(dataset)
        .and_then(|d| d.field(name))
        .expect("field exists")
}

fn enc(channel: Channel, dataset: &str, name: &str, kind: FieldType) -> Encoding {
    Encoding::plain(channel, field(dataset, name), kind).expect("valid encoding")
}

fn month_x() -> Encoding {
    Encoding::new(
        Channel::X,
        field("income_expense_by_month", "month"),
        FieldType::Temporal,
        None,
        None,
        Some(TimeUnit::YearMonth),
        None,
    )
    .expect("valid")
}

fn net_y() -> Encoding {
    enc(
        Channel::Y,
        "income_expense_by_month",
        "net",
        FieldType::Quantitative,
    )
}

fn bar() -> Layer {
    Layer::new(Mark::plain(MarkKind::Bar), vec![month_x(), net_y()], vec![]).expect("valid")
}

fn spec_of(layers: Vec<Layer>) -> Result<ChartSpec, ModelError> {
    ChartSpec::new(
        spec_dataset("income_expense_by_month").expect("known"),
        None,
        None,
        None,
        vec![],
        vec![],
        layers,
    )
}

#[test]
fn text_is_limited_and_one_line() {
    assert_eq!(
        SpecText::new("Net by month").expect("fine").as_str(),
        "Net by month"
    );
    assert!(SpecText::new(&"x".repeat(200)).is_ok());
    assert_eq!(
        SpecText::new(&"x".repeat(201)),
        Err(ModelError::TooBig("label"))
    );
    assert_eq!(SpecText::new("a\nb"), Err(ModelError::Invalid("bad-text")));
    assert_eq!(
        SpecText::new("a\u{7}b"),
        Err(ModelError::Invalid("bad-text"))
    );
    // Characters are counted, not bytes.
    assert!(SpecText::new(&"ä".repeat(200)).is_ok());
}

#[test]
fn numbers_keep_digits_and_places() {
    let number = SpecNumber::parse("12.50").expect("fine");
    assert_eq!((number.digits(), number.places()), (1250, 2));
    assert_eq!(SpecNumber::parse("-0.5").expect("fine").digits(), -5);
    assert_eq!(SpecNumber::parse("7"), Ok(SpecNumber::whole(7)));
    for bad in [
        "", "-", ".5", "5.", "1e3", "+1", " 1", "1,5", "0x10", "1.2.3", "--1",
    ] {
        assert_eq!(
            SpecNumber::parse(bad),
            Err(ModelError::Invalid("bad-number")),
            "{bad}"
        );
    }
    assert!(SpecNumber::parse(&format!("0.{}", "1".repeat(18))).is_ok());
    assert!(SpecNumber::parse(&format!("0.{}", "1".repeat(19))).is_err());
    assert!(SpecNumber::parse("9223372036854775808").is_err());
    assert!(SpecNumber::parse("9223372036854775807").is_ok());
}

#[test]
fn numbers_scale_exactly_without_rounding() {
    let n = |text: &str| SpecNumber::parse(text).expect("fine");
    assert_eq!(n("12.50").to_minor_units(2), Some(1250));
    assert_eq!(n("12.5").to_minor_units(2), Some(1250));
    assert_eq!(n("12").to_minor_units(2), Some(1200));
    assert_eq!(n("12.50").to_minor_units(1), Some(125));
    assert_eq!(n("12.55").to_minor_units(1), None);
    assert_eq!(n("12.5").to_minor_units(0), None);
    assert_eq!(n("9223372036854775807").to_minor_units(2), None);
    assert_eq!(n("1.10").compare(n("1.1")), Ordering::Equal);
    assert_eq!(n("1.01").compare(n("1.1")), Ordering::Less);
    assert_eq!(n("-2").compare(n("-1.5")), Ordering::Less);
}

proptest! {
    #[test]
    fn parse_then_scale_matches_integer_math(whole in 0_i64..1_000_000, cents in 0_i64..100) {
        let text = format!("{whole}.{cents:02}");
        let number = SpecNumber::parse(&text).expect("fine");
        prop_assert_eq!(number.to_minor_units(2), Some(whole * 100 + cents));
    }

    #[test]
    fn compare_matches_scaled_integers(a in -100_000_i64..100_000, b in -100_000_i64..100_000) {
        let x = SpecNumber::parse(&format!("{}.{:03}", a / 1000, (a % 1000).abs()));
        let y = SpecNumber::parse(&format!("{}.{:03}", b / 1000, (b % 1000).abs()));
        // Skip the shapes like "-0.500" whose sign the text form can't carry.
        prop_assume!(a.abs() >= 1000 && b.abs() >= 1000);
        let (x, y) = (x.expect("fine"), y.expect("fine"));
        prop_assert_eq!(x.compare(y), x.to_minor_units(3).cmp(&y.to_minor_units(3)));
    }
}

#[test]
fn size_palette_and_marks_are_checked() {
    assert!(ChartSize::new(Some(100), Some(2000)).is_ok());
    assert!(ChartSize::new(None, None).is_ok());
    assert!(ChartSize::new(Some(99), None).is_err());
    assert!(ChartSize::new(None, Some(2001)).is_err());
    for name in PaletteName::ALL {
        assert_eq!(PaletteName::from_word(name.word()), Some(name));
    }
    assert_eq!(PaletteName::Grey.index(), 7);
    assert_eq!(PaletteName::from_word("purple"), None);
    assert!(Mark::new(MarkKind::Line, true, true, None).is_ok());
    assert!(Mark::new(MarkKind::Bar, false, true, None).is_err());
    assert!(Mark::new(MarkKind::Arc, false, false, Some(40)).is_ok());
    assert_eq!(
        Mark::new(MarkKind::Arc, false, false, Some(0)),
        Err(ModelError::Invalid("bad-number"))
    );
    assert!(Mark::new(MarkKind::Line, false, false, Some(40)).is_err());
}

#[test]
fn encodings_follow_the_type_rules() {
    let sp = |name| field("spending_by_category", name);
    let bd = |name| field("budget_utilization", name);
    let ok = |channel, f, kind, agg| Encoding::new(channel, f, kind, None, agg, None, None);
    // A type that does not suit the field.
    assert_eq!(
        ok(Channel::Y, sp("category"), FieldType::Quantitative, None),
        Err(ModelError::Invalid("type-mismatch"))
    );
    // Counting makes any field a quantity.
    assert!(
        ok(
            Channel::Y,
            sp("category"),
            FieldType::Quantitative,
            Some(Aggregate::Count)
        )
        .is_ok()
    );
    // A percent takes no sum and no mean, but min and max.
    for bad in [Aggregate::Sum, Aggregate::Mean] {
        assert_eq!(
            ok(Channel::Y, bd("used"), FieldType::Quantitative, Some(bad)),
            Err(ModelError::Invalid("bad-aggregate"))
        );
    }
    assert!(
        ok(
            Channel::Y,
            bd("used"),
            FieldType::Quantitative,
            Some(Aggregate::Max)
        )
        .is_ok()
    );
    assert!(
        ok(
            Channel::Y,
            bd("spent"),
            FieldType::Quantitative,
            Some(Aggregate::Mean)
        )
        .is_ok()
    );
    // A min of text is no aggregate.
    assert!(
        ok(
            Channel::Y,
            sp("category"),
            FieldType::Quantitative,
            Some(Aggregate::Min)
        )
        .is_err()
    );
    // Channel and type: no horizontal quantity on x, no quantitative colour.
    assert!(ok(Channel::X, sp("total"), FieldType::Quantitative, None).is_err());
    assert!(ok(Channel::Color, sp("total"), FieldType::Quantitative, None).is_err());
    assert!(ok(Channel::Theta, sp("category"), FieldType::Nominal, None).is_err());
    // A time unit needs a temporal day.
    let month = field("income_expense_by_month", "month");
    assert!(
        Encoding::new(
            Channel::X,
            month,
            FieldType::Ordinal,
            None,
            None,
            Some(TimeUnit::Year),
            None
        )
        .is_err()
    );
    assert!(
        Encoding::new(
            Channel::Color,
            sp("category"),
            FieldType::Nominal,
            None,
            None,
            Some(TimeUnit::Year),
            None
        )
        .is_err()
    );
}

#[test]
fn filters_check_their_values() {
    let text = |t: &str| SpecValue::Text(SpecText::new(t).expect("fine"));
    let num = |t: &str| SpecValue::Number(SpecNumber::parse(t).expect("fine"));
    let category = field("spending_by_category", "category");
    let total = field("spending_by_category", "total");
    let month = field("income_expense_by_month", "month");
    assert!(FieldFilter::new(category, FilterTest::Equal(text("Food"))).is_ok());
    assert!(FieldFilter::new(category, FilterTest::Equal(num("1"))).is_err());
    assert!(
        FieldFilter::new(
            total,
            FilterTest::Range {
                from: Some(num("0")),
                to: None
            }
        )
        .is_ok()
    );
    assert_eq!(
        FieldFilter::new(
            total,
            FilterTest::Range {
                from: Some(num("5")),
                to: Some(num("1.5"))
            }
        ),
        Err(ModelError::Invalid("bad-choice"))
    );
    assert_eq!(
        FieldFilter::new(
            total,
            FilterTest::Range {
                from: None,
                to: None
            }
        ),
        Err(ModelError::Invalid("missing"))
    );
    assert!(
        FieldFilter::new(
            month,
            FilterTest::Range {
                from: Some(SpecValue::Day(date(2026, 1, 1))),
                to: Some(SpecValue::Day(date(2026, 3, 1)))
            }
        )
        .is_ok()
    );
    assert!(
        FieldFilter::new(
            month,
            FilterTest::Range {
                from: Some(SpecValue::Day(date(2026, 3, 1))),
                to: Some(SpecValue::Day(date(2026, 1, 1)))
            }
        )
        .is_err()
    );
    assert!(FieldFilter::new(category, FilterTest::OneOf(vec![])).is_err());
    let many = |n: usize| FilterTest::OneOf((0..n).map(|i| text(&format!("c{i}"))).collect());
    assert!(FieldFilter::new(category, many(50)).is_ok());
    assert_eq!(
        FieldFilter::new(category, many(51)),
        Err(ModelError::TooBig("values"))
    );
    // Yes/no fields take no filter in this version.
    let owed = field("account_balances", "owed");
    assert!(FieldFilter::new(owed, FilterTest::Equal(text("yes"))).is_err());
}

#[test]
fn layers_follow_the_channel_table() {
    let line = |kind| Layer::new(Mark::plain(kind), vec![month_x(), net_y()], vec![]);
    for kind in [
        MarkKind::Line,
        MarkKind::Bar,
        MarkKind::Area,
        MarkKind::Point,
    ] {
        assert!(line(kind).is_ok());
    }
    // Missing y.
    assert_eq!(
        Layer::new(Mark::plain(MarkKind::Bar), vec![month_x()], vec![]),
        Err(ModelError::Invalid("missing"))
    );
    // size only on a point; text on no bar.
    let size = enc(
        Channel::Size,
        "income_expense_by_month",
        "net",
        FieldType::Quantitative,
    );
    let with = |mark, extra: Encoding| {
        Layer::new(Mark::plain(mark), vec![month_x(), net_y(), extra], vec![])
    };
    assert!(with(MarkKind::Point, size.clone()).is_ok());
    assert_eq!(
        with(MarkKind::Bar, size),
        Err(ModelError::Invalid("bad-channel-for-mark"))
    );
    // A channel twice, except tooltip.
    assert!(
        Layer::new(
            Mark::plain(MarkKind::Bar),
            vec![month_x(), net_y(), net_y()],
            vec![]
        )
        .is_err()
    );
    let tip = || {
        enc(
            Channel::Tooltip,
            "income_expense_by_month",
            "income",
            FieldType::Quantitative,
        )
    };
    assert!(
        Layer::new(
            Mark::plain(MarkKind::Bar),
            vec![month_x(), net_y(), tip(), tip()],
            vec![]
        )
        .is_ok()
    );
    // Arc: theta and colour.
    let theta = enc(
        Channel::Theta,
        "spending_by_category",
        "total",
        FieldType::Quantitative,
    );
    let color = enc(
        Channel::Color,
        "spending_by_category",
        "category",
        FieldType::Nominal,
    );
    assert!(
        Layer::new(
            Mark::plain(MarkKind::Arc),
            vec![theta.clone(), color],
            vec![]
        )
        .is_ok()
    );
    assert!(Layer::new(Mark::plain(MarkKind::Arc), vec![theta], vec![]).is_err());
    // Rule: exactly one of x and y.
    assert!(Layer::new(Mark::plain(MarkKind::Rule), vec![net_y()], vec![]).is_ok());
    assert!(Layer::new(Mark::plain(MarkKind::Rule), vec![month_x()], vec![]).is_ok());
    assert!(
        Layer::new(
            Mark::plain(MarkKind::Rule),
            vec![month_x(), net_y()],
            vec![]
        )
        .is_err()
    );
    assert!(Layer::new(Mark::plain(MarkKind::Rule), vec![], vec![]).is_err());
    // Text needs a text channel.
    let label = enc(
        Channel::Text,
        "income_expense_by_month",
        "net",
        FieldType::Quantitative,
    );
    assert!(
        Layer::new(
            Mark::plain(MarkKind::Text),
            vec![month_x(), net_y(), label],
            vec![]
        )
        .is_ok()
    );
    assert!(
        Layer::new(
            Mark::plain(MarkKind::Text),
            vec![month_x(), net_y()],
            vec![]
        )
        .is_err()
    );
}

#[test]
fn a_spec_checks_counts_fields_and_the_shared_axis() {
    let one = spec_of(vec![bar()]).expect("valid");
    assert_eq!(one.dataset().id(), "income_expense_by_month");
    assert_eq!(one.layers().len(), 1);
    assert_eq!(one.layers()[0].mark().kind(), MarkKind::Bar);
    assert!(one.title().is_none() && one.size().is_none() && one.palette().is_empty());

    assert_eq!(spec_of(vec![]), Err(ModelError::Invalid("missing")));
    assert_eq!(
        spec_of(vec![bar(), bar(), bar(), bar(), bar()]),
        Err(ModelError::TooBig("layers"))
    );
    // Five layers of two encodings are over 8 encodings but four are exactly 8.
    assert!(spec_of(vec![bar(), bar(), bar(), bar()]).is_ok());
    // One more encoding in four layers goes over.
    let tip = enc(
        Channel::Tooltip,
        "income_expense_by_month",
        "income",
        FieldType::Quantitative,
    );
    let fat = Layer::new(
        Mark::plain(MarkKind::Bar),
        vec![month_x(), net_y(), tip],
        vec![],
    )
    .expect("valid");
    assert_eq!(
        spec_of(vec![fat, bar(), bar(), bar()]),
        Err(ModelError::TooBig("encodings"))
    );
    // A field of another dataset.
    let stranger = enc(
        Channel::Y,
        "spending_by_category",
        "total",
        FieldType::Quantitative,
    );
    let layer = Layer::new(
        Mark::plain(MarkKind::Bar),
        vec![month_x(), stranger],
        vec![],
    )
    .expect("valid");
    assert_eq!(
        spec_of(vec![layer]),
        Err(ModelError::Invalid("unknown-field"))
    );
    // Layers must share the x axis.
    let other_x = Encoding::new(
        Channel::X,
        field("income_expense_by_month", "month"),
        FieldType::Temporal,
        None,
        None,
        Some(TimeUnit::Year),
        None,
    )
    .expect("valid");
    let other =
        Layer::new(Mark::plain(MarkKind::Line), vec![other_x, net_y()], vec![]).expect("valid");
    assert_eq!(
        spec_of(vec![bar(), other]),
        Err(ModelError::Invalid("layers-differ"))
    );
    assert!(
        spec_of(vec![
            bar(),
            Layer::new(
                Mark::plain(MarkKind::Line),
                vec![month_x(), net_y()],
                vec![]
            )
            .expect("valid")
        ])
        .is_ok()
    );
    // An arc stands alone.
    let theta = enc(
        Channel::Theta,
        "income_expense_by_month",
        "net",
        FieldType::Quantitative,
    );
    let color = enc(
        Channel::Color,
        "income_expense_by_month",
        "currency",
        FieldType::Nominal,
    );
    let arc = Layer::new(Mark::plain(MarkKind::Arc), vec![theta, color], vec![]).expect("valid");
    assert!(spec_of(vec![arc.clone()]).is_ok());
    assert_eq!(
        spec_of(vec![arc, bar()]),
        Err(ModelError::Invalid("layers-differ"))
    );
}

#[test]
fn filters_and_palette_are_counted_over_the_whole_spec() {
    let currency = field("income_expense_by_month", "currency");
    let filter = || {
        FieldFilter::new(
            currency,
            FilterTest::Equal(SpecValue::Text(SpecText::new("EUR").expect("fine"))),
        )
        .expect("valid")
    };
    let make = |top: usize, own: usize, palette: usize| {
        let layer = Layer::new(
            Mark::plain(MarkKind::Bar),
            vec![month_x(), net_y()],
            (0..own).map(|_| filter()).collect(),
        )
        .expect("valid");
        ChartSpec::new(
            spec_dataset("income_expense_by_month").expect("known"),
            None,
            None,
            None,
            PaletteName::ALL
                .iter()
                .copied()
                .cycle()
                .take(palette)
                .collect(),
            (0..top).map(|_| filter()).collect(),
            vec![layer],
        )
    };
    assert!(make(4, 4, 8).is_ok());
    assert_eq!(make(5, 4, 0), Err(ModelError::TooBig("filters")));
    assert_eq!(make(0, 0, 9), Err(ModelError::TooBig("values")));
    assert_eq!(
        Layer::new(
            Mark::plain(MarkKind::Bar),
            vec![month_x(), net_y()],
            (0..9).map(|_| filter()).collect()
        ),
        Err(ModelError::TooBig("filters"))
    );
}
