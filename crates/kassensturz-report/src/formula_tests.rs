// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of formulas (M6b.6, D-292, D-295, T-41, T-42): the syntax and its
//! mistakes, the limits against hostile formulas, the kinds, exact results
//! against an independent calculation, the user's number forms, and
//! formulas worked out on a real book with the store's own groups, through
//! the reporting layer's real datasets.

use kassensturz_core::{AccountKind, GroupKind, GroupMember, Money, RoundingMode};
use proptest::prelude::*;

use super::tests::{EUR, USD, account, autumn, build, chart, date, entries, period, sample};
use super::*;

/// A book without anything, for formulas of numbers only.
fn empty() -> Source {
    Source::from_parts(autumn(), Vec::new(), Vec::new(), Vec::new())
}

fn worked_out(source: &Source, text: &str, figure: Period) -> Result<FormulaValue, String> {
    Formula::parse(text)
        .and_then(|formula| formula.evaluate(source, figure, &Filter::default(), EUR))
        .map(|result| result.value)
        .map_err(described)
}

fn number(text: &str) -> Result<FormulaValue, String> {
    worked_out(&empty(), text, autumn())
}

fn hundredths(hundredths: i64) -> FormulaValue {
    FormulaValue::Number { hundredths }
}

fn tenths(tenths: i64) -> FormulaValue {
    FormulaValue::Percent(Percent::from_tenths(tenths))
}

fn eur(minor_units: i64) -> FormulaValue {
    FormulaValue::Amount(Money::from_minor_units(minor_units, EUR))
}

/// The formula read in the stored form, a mistake described.
fn read(text: &str) -> Result<Formula, String> {
    Formula::parse(text).map_err(described)
}

/// The formula read in the user's form, a mistake described.
fn read_users(text: &str, form: NumberForm) -> Result<Formula, String> {
    Formula::parse_users(text, form).map_err(described)
}

/// The mistake a result is.
fn mistake<T: core::fmt::Debug>(result: Result<T, String>) -> String {
    result.expect_err("a mistake")
}

/// The code and parameters of a mistake, as the UI gets them.
fn described(error: ReportError) -> String {
    let report = kassensturz_core::Report::report(&error);
    let params: Vec<String> = report
        .params
        .iter()
        .map(|(name, value)| match value {
            kassensturz_core::ErrorParam::Number(number) => format!("{name}={number}"),
            kassensturz_core::ErrorParam::Keyword(word) => format!("{name}={word}"),
            kassensturz_core::ErrorParam::Text(text) => format!("{name}={text}"),
            kassensturz_core::ErrorParam::Currency(code) => format!("{name}={code}"),
            other => format!("{name}={other:?}"),
        })
        .collect();
    format!("{} {}", report.code, params.join(" "))
}

fn parse_mistake(text: &str) -> String {
    mistake(read(text))
}

#[test]
fn numbers_and_operators_follow_the_usual_order() {
    assert_eq!(number("=1 + 2 * 3"), Ok(hundredths(700)));
    assert_eq!(number("(1 + 2) * 3"), Ok(hundredths(900)));
    assert_eq!(number("=10 - 4 - 3"), Ok(hundredths(300)));
    assert_eq!(number("=-2 * -3"), Ok(hundredths(600)));
    assert_eq!(number("=--+-5"), Ok(hundredths(-500)));
    assert_eq!(number("=1.5 * 1.5"), Ok(hundredths(225)));
    // The result is rounded once, at the end: 0.125 + 0.125 is 0.25.
    assert_eq!(number("=0.125 + 0.125"), Ok(hundredths(25)));
    assert_eq!(number("=0.125"), Ok(hundredths(13)));
    assert_eq!(number("=-0.125"), Ok(hundredths(-13)));
    // `%` divides by a hundred and shows a percent.
    assert_eq!(number("=20%"), Ok(tenths(200)));
    assert_eq!(number("=-12.34%"), Ok(tenths(-123)));
    assert_eq!(number("=200 * 15%"), Ok(hundredths(3000)));
}

#[test]
fn division_rounds_to_ten_places_and_by_zero_has_no_value() {
    assert_eq!(number("=1/3*3"), Ok(hundredths(100)));
    // 2/3 is 0.6666666667 after the division; times 3 it is 2.0000000001.
    assert_eq!(number("=2/3*300000000"), Ok(hundredths(20_000_000_001)));
    assert_eq!(
        mistake(number("=1 + 5/(2-2)")),
        "report.formula-no-value position=7"
    );
    assert_eq!(number("=IFERROR(5/0; 7)"), Ok(hundredths(700)));
    // IFERROR doesn't hide a mistake in the formula.
    assert_eq!(
        mistake(number("=IFERROR(Nothing; 7)")),
        "report.formula-unknown-name position=10 name=Nothing"
    );
}

#[test]
fn the_wave_one_functions_give_what_the_spec_says() {
    let cases = [
        ("=SUM(1; 2; 3.5)", hundredths(650)),
        ("=sum(1)", hundredths(100)),
        ("=AVERAGE(1; 2)", hundredths(150)),
        ("=AVERAGE(1; 1; 2)", hundredths(133)),
        ("=MIN(3; -1; 2)", hundredths(-100)),
        ("=MAX(3; -1; 2)", hundredths(300)),
        ("=ABS(-4.5)", hundredths(450)),
        ("=SIGN(-4.5)", hundredths(-100)),
        ("=SIGN(0)", hundredths(0)),
        ("=ROUND(2.345; 2)", hundredths(235)),
        ("=ROUND(-2.345; 2)", hundredths(-235)),
        ("=ROUND(2.5)", hundredths(300)),
        ("=ROUND(1250; -2)", hundredths(130_000)),
        ("=ROUNDUP(2.301; 1)", hundredths(240)),
        ("=ROUNDUP(-2.301; 1)", hundredths(-240)),
        ("=ROUNDDOWN(2.399; 1)", hundredths(230)),
        ("=ROUNDDOWN(-2.399; 1)", hundredths(-230)),
        ("=INT(2.7)", hundredths(200)),
        ("=INT(-2.1)", hundredths(-300)),
        ("=IF(1 < 2; 10; 20)", hundredths(1000)),
        ("=IF(1 > 2; 10; 20)", hundredths(2000)),
        // Only the chosen branch is worked out.
        ("=IF(1 = 1; 1; 1/0)", hundredths(100)),
        ("=IF(AND(1 < 2; 2 < 3); 1; 0)", hundredths(100)),
        ("=IF(AND(1 < 2; 3 < 2); 1; 0)", hundredths(0)),
        ("=IF(OR(1 > 2; 2 < 3); 1; 0)", hundredths(100)),
        ("=IF(NOT(1 <> 1); 1; 0)", hundredths(100)),
        ("=IF(2 >= 2; 1; 0)", hundredths(100)),
        ("=IF(2 <= 1; 1; 0)", hundredths(0)),
        ("=PERCENT_OF(1; 3)", tenths(333)),
        ("=PERCENT_OF(2; 3)", tenths(667)),
        ("=CHANGE(110; 100)", tenths(100)),
        // From a negative figure, a rise is positive (D-285).
        ("=CHANGE(-50; -100)", tenths(500)),
        // A percent stays one through MIN, MAX, AVERAGE, ABS and rounding.
        ("=MAX(10%; 20%)", tenths(200)),
        ("=AVERAGE(10%; 20%)", tenths(150)),
        ("=ABS(-CHANGE(90; 100))", tenths(100)),
        ("=ROUND(PERCENT_OF(1; 3); 2)", tenths(330)),
        ("=IF(1 < 2; 10%; 20%)", tenths(100)),
        // A sum of percents, or a percent joined with a number, is a number.
        ("=SUM(10%; 20%)", hundredths(30)),
        ("=MAX(10%; 1)", hundredths(100)),
    ];
    for (text, expected) in cases {
        assert_eq!(number(text), Ok(expected), "{text}");
    }
    assert_eq!(
        mistake(number("=PERCENT_OF(1; 0)")),
        "report.formula-no-value position=2"
    );
    assert_eq!(
        mistake(number("=CHANGE(1; 0)")),
        "report.formula-no-value position=2"
    );
    assert_eq!(
        mistake(number("=ROUND(1; 1.5)")),
        "report.formula-arguments position=11 function=ROUND reason=places"
    );
    assert_eq!(
        mistake(number("=ROUND(1; 11)")),
        "report.formula-arguments position=11 function=ROUND reason=places"
    );
}

#[test]
fn syntax_mistakes_say_where_and_what() {
    let cases = [
        ("", "report.formula-syntax position=1 reason=empty"),
        ("=", "report.formula-syntax position=2 reason=empty"),
        (
            "=SUM(1; 2",
            "report.formula-syntax position=10 reason=missing-close",
        ),
        (
            "=(1 + 2",
            "report.formula-syntax position=8 reason=missing-close",
        ),
        (
            "=1 + 2)",
            "report.formula-syntax position=7 reason=missing-open",
        ),
        (
            "=1 +",
            "report.formula-syntax position=5 reason=expected-value",
        ),
        (
            "=1 + * 2",
            "report.formula-syntax position=6 reason=expected-value",
        ),
        ("=1 2", "report.formula-syntax position=4 reason=unexpected"),
        (
            "=Pets Travel",
            "report.formula-syntax position=7 reason=unexpected",
        ),
        (
            "=1 < 2 < 3",
            "report.formula-syntax position=8 reason=two-comparisons",
        ),
        // In the stored form, `,` is never a separator (D-295).
        (
            "=SUM(1, 2)",
            "report.formula-syntax position=7 reason=wrong-separator",
        ),
        (
            "=1; 2",
            "report.formula-syntax position=3 reason=wrong-separator",
        ),
        (
            "=SUM(;1)",
            "report.formula-syntax position=6 reason=wrong-separator",
        ),
        ("=12.", "report.formula-syntax position=4 reason=bad-number"),
        ("=.5", "report.formula-syntax position=2 reason=bad-number"),
        (
            "=1 & 2",
            "report.formula-syntax position=4 reason=bad-character",
        ),
        (
            "='Pets",
            "report.formula-syntax position=2 reason=missing-quote",
        ),
        (
            "=Pets@\"next month\"",
            "report.formula-syntax position=7 reason=bad-period",
        ),
        (
            "=Pets@",
            "report.formula-syntax position=7 reason=bad-period",
        ),
        (
            "=Pets@26",
            "report.formula-syntax position=7 reason=bad-period",
        ),
        (
            "=Pets@2026-3",
            "report.formula-syntax position=7 reason=bad-period",
        ),
        (
            "=Pets@2026-13",
            "report.formula-syntax position=7 reason=bad-period",
        ),
        (
            "=Pets@2026-02-30",
            "report.formula-syntax position=7 reason=bad-period",
        ),
        (
            "=Pets@2026x",
            "report.formula-syntax position=11 reason=bad-period",
        ),
        // A control character or a direction mark outside quotes (T-42).
        (
            "=1 +\u{7}2",
            "report.formula-syntax position=5 reason=bad-character",
        ),
        (
            "=Pets\u{202E}",
            "report.formula-syntax position=6 reason=bad-character",
        ),
    ];
    for (text, expected) in cases {
        assert_eq!(parse_mistake(text), expected, "{text:?}");
    }
    assert_eq!(
        parse_mistake("=SUMME(1)"),
        "report.formula-unknown-function position=2 name=SUMME"
    );
    assert_eq!(
        parse_mistake("=Pets@2026-03..2026-01"),
        "report.bad-period "
    );
    assert_eq!(parse_mistake("=Pets@1900..2026"), "report.period-too-long ");
}

#[test]
fn functions_check_their_arguments() {
    let cases = [
        (
            "=ABS()",
            "report.formula-arguments position=2 function=ABS reason=too-few",
        ),
        (
            "=ABS(1; 2)",
            "report.formula-arguments position=2 function=ABS reason=too-many",
        ),
        (
            "=IF(1 < 2; 3)",
            "report.formula-arguments position=2 function=IF reason=too-few",
        ),
        (
            "=COUNT(1)",
            "report.formula-arguments position=8 function=COUNT reason=names-only",
        ),
        (
            "=COUNT(Pets + Vet)",
            "report.formula-arguments position=13 function=COUNT reason=names-only",
        ),
        (
            "=ROUND(Pets; Vet)",
            "report.formula-arguments position=14 function=ROUND reason=places",
        ),
    ];
    for (text, expected) in cases {
        assert_eq!(parse_mistake(text), expected, "{text}");
    }
}

#[test]
fn kinds_are_checked_before_anything_is_read() {
    let cases = [
        (
            "=Pets * Vet",
            "report.formula-kinds position=7 reason=amount-times-amount",
        ),
        (
            "=1 / Pets",
            "report.formula-kinds position=4 reason=number-over-amount",
        ),
        (
            "=(1 < 2) + 1",
            "report.formula-kinds position=5 reason=yes-no-in-arithmetic",
        ),
        (
            "=-(1 < 2)",
            "report.formula-kinds position=6 reason=yes-no-in-arithmetic",
        ),
        (
            "=SUM(1 < 2)",
            "report.formula-kinds position=8 reason=yes-no-in-arithmetic",
        ),
        (
            "=1 < 2",
            "report.formula-kinds position=4 reason=yes-no-result",
        ),
        (
            "=IF(1; 2; 3)",
            "report.formula-kinds position=5 reason=needs-yes-no",
        ),
        (
            "=AND(1 < 2; 3)",
            "report.formula-kinds position=13 reason=needs-yes-no",
        ),
        (
            "=IF(1 < 2; 1 < 2; 3)",
            "report.formula-kinds position=19 reason=if-kinds-differ",
        ),
        (
            "=IFERROR(1; 1 < 2)",
            "report.formula-kinds position=15 reason=if-kinds-differ",
        ),
        (
            "=PERCENT_OF(Pets; 3)",
            "report.formula-kinds position=19 reason=ratio-kinds",
        ),
    ];
    for (text, expected) in cases {
        assert_eq!(parse_mistake(text), expected, "{text}");
    }
    // These go together: a number joins an amount as an amount.
    for text in [
        "=Pets - 50",
        "=50 - Pets",
        "=Pets * 1.5",
        "=Pets / 2",
        "=Pets / Vet",
        "=IF(Pets > 100; Pets; 0)",
        "=SUM(Pets; 5)",
        "=Pets * 20%",
        "=COUNT(Pets; Vet@\"last month\")",
    ] {
        assert!(Formula::parse(text).is_ok(), "{text}");
    }
}

/// `=1+1+…` with this many ones.
fn ones(count: usize) -> String {
    format!("={}", vec!["1"; count].join("+"))
}

#[test]
fn hostile_formulas_stop_at_the_limits() {
    // Characters.
    let long = format!("={}", " ".repeat(MAX_FORMULA_CHARS - 2) + "1");
    assert!(Formula::parse(&long).is_ok());
    assert_eq!(
        parse_mistake(&format!("{long} ")),
        "report.formula-too-big limit=characters"
    );
    // Tokens: `=` and 250 ones with 249 pluses are 500.
    assert!(Formula::parse(&ones(250)).is_ok());
    assert_eq!(
        parse_mistake(&ones(251)),
        "report.formula-too-big limit=tokens"
    );
    // Depth: brackets and calls.
    let nested = |depth: usize| format!("={}1{}", "(".repeat(depth), ")".repeat(depth));
    assert!(Formula::parse(&nested(MAX_FORMULA_DEPTH)).is_ok());
    assert_eq!(
        parse_mistake(&nested(MAX_FORMULA_DEPTH + 1)),
        "report.formula-too-big limit=depth"
    );
    let calls = |depth: usize| format!("={}1{}", "ABS(".repeat(depth), ")".repeat(depth));
    assert!(Formula::parse(&calls(MAX_FORMULA_DEPTH)).is_ok());
    assert_eq!(
        parse_mistake(&calls(MAX_FORMULA_DEPTH + 1)),
        "report.formula-too-big limit=depth"
    );
    // Arguments.
    let sum = |count: usize| format!("=SUM({})", vec!["1"; count].join(";"));
    assert!(Formula::parse(&sum(MAX_FORMULA_ARGUMENTS)).is_ok());
    assert_eq!(
        parse_mistake(&sum(MAX_FORMULA_ARGUMENTS + 1)),
        "report.formula-too-big limit=arguments"
    );
    // Names.
    let names = |count: usize| format!("={}", vec!["Pets"; count].join("+"));
    assert!(Formula::parse(&names(MAX_FORMULA_NAMES)).is_ok());
    assert_eq!(
        parse_mistake(&names(MAX_FORMULA_NAMES + 1)),
        "report.formula-too-big limit=names"
    );
    // Different periods; the same one twice counts once.
    let periods = |count: usize| {
        let names: Vec<String> = (0..count)
            .map(|year| format!("Pets@{}", 2001 + year))
            .collect();
        format!("={}+Pets@2001", names.join("+"))
    };
    assert!(Formula::parse(&periods(MAX_FORMULA_PERIODS)).is_ok());
    assert_eq!(
        parse_mistake(&periods(MAX_FORMULA_PERIODS + 1)),
        "report.formula-too-big limit=periods"
    );
    // Numbers with too many digits, figures too large.
    assert_eq!(
        parse_mistake(&format!("={}", "9".repeat(40))),
        "report.formula-syntax position=2 reason=bad-number"
    );
    assert_eq!(
        parse_mistake(&format!("=0.{}1", "0".repeat(28))),
        "report.formula-syntax position=2 reason=bad-number"
    );
    let huge = "99999999999999999999";
    assert_eq!(
        mistake(number(&format!("={huge}*{huge}"))),
        "report.formula-too-large position=22"
    );
    assert_eq!(
        mistake(number(&format!("={huge}"))),
        "report.formula-too-large position=1"
    );
    assert_eq!(
        number(&format!("=IFERROR({huge}*{huge}; 1)")),
        Ok(hundredths(100))
    );
    // A deep run of signs is read in a loop, not by recursion.
    let signs = format!("={}1", "-".repeat(MAX_FORMULA_TOKENS - 2));
    assert_eq!(number(&signs), Ok(hundredths(100)));
}

#[test]
fn the_users_form_changes_only_separators_and_decimal_marks() {
    let stored =
        "=ROUND(Pets * 1.5; 2) + 'Eating; out, 1.5' + Pets@2026-01..2026-03 + Vet@\"last month\"";
    let comma = Formula::to_users_form(stored, NumberForm::DecimalComma).expect("converted");
    assert_eq!(
        comma,
        "=ROUND(Pets * 1,5; 2) + 'Eating; out, 1.5' + Pets@2026-01..2026-03 + Vet@\"last month\""
    );
    let point = Formula::to_users_form(stored, NumberForm::DecimalPoint).expect("converted");
    assert_eq!(
        point,
        "=ROUND(Pets * 1.5, 2) + 'Eating; out, 1.5' + Pets@2026-01..2026-03 + Vet@\"last month\""
    );
    for (text, form) in [
        (&comma, NumberForm::DecimalComma),
        (&point, NumberForm::DecimalPoint),
    ] {
        assert_eq!(
            Formula::to_canonical(text, form).expect("converted"),
            stored
        );
        assert_eq!(read_users(text, form), read(stored));
    }
    // `;` is a separator in every form, as LibreOffice takes it.
    assert_eq!(
        Formula::to_canonical("=SUM(1.5; 2)", NumberForm::DecimalPoint).expect("converted"),
        "=SUM(1.5; 2)"
    );
    assert_eq!(
        read_users("=SUM(1, 5)", NumberForm::DecimalPoint),
        read("=SUM(1; 5)")
    );
    assert_eq!(
        read_users("=SUM(1,5)", NumberForm::DecimalComma),
        read("=SUM(1.5)")
    );
    // With a decimal comma, a comma between the parts is a mistake.
    assert_eq!(
        mistake(read_users("=SUM(Pets, Vet)", NumberForm::DecimalComma)),
        "report.formula-syntax position=10 reason=wrong-separator"
    );
    assert_eq!(
        mistake(read_users("=SUM(1.5)", NumberForm::DecimalComma)),
        "report.formula-syntax position=7 reason=bad-character"
    );
}

#[test]
fn names_count_what_the_datasets_count_on_a_real_book() {
    let sample = sample();
    // Read for more than autumn, for the periods the names give themselves.
    let second_half = period(date(2026, 7, 1), date(2026, 12, 31));
    let source = Source::read(&sample.book, second_half).expect("read");
    let figure = |text: &str| worked_out(&source, text, autumn());
    // Food holds Groceries; a refund made Groceries smaller.
    assert_eq!(figure("=Food"), Ok(eur(8_820)));
    assert_eq!(figure("=Groceries"), Ok(eur(7_820)));
    assert_eq!(figure("='Food > Groceries'"), Ok(eur(7_820)));
    assert_eq!(figure("=food - GROCERIES"), Ok(eur(1_000)));
    assert_eq!(figure("=Salary - Rent - Food"), Ok(eur(501_180)));
    assert_eq!(figure("=SUM(Rent; Food)"), Ok(eur(248_820)));
    assert_eq!(figure("=Rent / 3"), Ok(eur(80_000)));
    assert_eq!(figure("=AVERAGE(Rent; Food)"), Ok(eur(124_410)));
    assert_eq!(figure("=PERCENT_OF(Rent; Salary)"), Ok(tenths(320)));
    assert_eq!(figure("=Rent / Salary"), Ok(hundredths(32)));
    assert_eq!(figure("=Food - 50"), Ok(eur(3_820)));
    assert_eq!(figure("=IF(Food > 100; 1; 2)"), Ok(hundredths(200)));
    assert_eq!(figure("=ROUND(Groceries; 0)"), Ok(eur(7_800)));
    // A name gives itself a period; words count from the figure's period.
    let october = period(date(2026, 10, 1), date(2026, 10, 31));
    assert_eq!(
        worked_out(&source, "=Food@\"last month\"", october),
        Ok(eur(3_500))
    );
    assert_eq!(
        worked_out(&source, "=Rent@\"this quarter\"", october),
        Ok(eur(80_000))
    );
    assert_eq!(figure("=Groceries@2026-08"), Ok(eur(5_320)));
    assert_eq!(figure("=Groceries@2026-09-01..2026-09-12"), Ok(eur(2_500)));
    assert_eq!(
        worked_out(&source, "=CHANGE(Food; Food@\"last month\")", october),
        Ok(tenths(-1_000))
    );
    // COUNT counts transactions, each once.
    assert_eq!(figure("=COUNT(Food)"), Ok(hundredths(400)));
    assert_eq!(figure("=COUNT(Food; Groceries)"), Ok(hundredths(400)));
    assert_eq!(figure("=COUNT(Rent; Food)"), Ok(hundredths(700)));
    assert_eq!(figure("=COUNT(Rent@2026-10)"), Ok(hundredths(100)));
    // Only counted, Travel's dollars don't decide the currency.
    assert_eq!(figure("=Food + COUNT(Travel)"), Ok(eur(8_920)));
    // A name with nothing in the period is zero in the given currency.
    assert_eq!(figure("=Food@2026-12"), Ok(eur(0)));
}

#[test]
fn the_result_and_what_it_counted_come_back_together() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("read");
    let formula = Formula::parse("=Food + Food + Rent@2026-10").expect("parsed");
    let result = formula
        .evaluate(&source, autumn(), &Filter::default(), USD)
        .expect("worked out");
    assert_eq!(result.value, eur(17_640 + 80_000));
    let counted: Vec<(GroupMember, Vec<String>, Period)> = result
        .counted
        .into_iter()
        .map(|counted| (counted.of, counted.path, counted.period))
        .collect();
    assert_eq!(
        counted,
        [
            (
                GroupMember::Category(sample.food),
                vec!["Food".into()],
                autumn()
            ),
            (
                GroupMember::Category(sample.rent),
                vec!["Rent".into()],
                period(date(2026, 10, 1), date(2026, 10, 31))
            ),
        ]
    );
}

#[test]
fn the_span_covers_every_period_a_name_gives_itself() {
    let october = period(date(2026, 10, 1), date(2026, 10, 31));
    let formula = Formula::parse("=Pets@2025-03 + Vet@\"last quarter\" + Food").expect("parsed");
    assert_eq!(
        formula.span(october).ok(),
        Some(period(date(2025, 3, 1), date(2026, 10, 31)))
    );
    // A Source read for less is refused, never silently short.
    let sample = sample();
    let source = Source::read(&sample.book, october).expect("read");
    let formula = Formula::parse("=Food@\"last month\"").expect("parsed");
    assert_eq!(
        mistake(
            formula
                .evaluate(&source, october, &Filter::default(), EUR)
                .map_err(described)
        ),
        "report.period-not-read "
    );
}

#[test]
fn groups_and_hidden_categories_count_as_the_store_keeps_them() {
    let mut sample = sample();
    let household = sample
        .book
        .add_category_group(
            "Household",
            GroupKind::Expense,
            &[
                GroupMember::Category(sample.rent),
                GroupMember::Category(sample.food),
                GroupMember::Category(sample.groceries),
            ],
        )
        .expect("group added");
    sample
        .book
        .add_category_group(
            "Everything",
            GroupKind::Expense,
            &[GroupMember::Group(household)],
        )
        .expect("group added");
    sample
        .book
        .set_hidden_categories(&[sample.rent])
        .expect("hidden");
    let source = Source::read(&sample.book, autumn()).expect("read");
    let figure = |text: &str| worked_out(&source, text, autumn());
    // Groceries is reached twice but counts once; hidden Rent counts.
    assert_eq!(figure("=Household"), Ok(eur(248_820)));
    assert_eq!(figure("=Everything - Rent"), Ok(eur(8_820)));
    assert_eq!(figure("=COUNT(Household)"), Ok(hundredths(700)));
}

#[test]
fn unclear_names_are_refused_with_the_paths_to_write_never_guessed() {
    let mut sample = sample();
    // A second Groceries, and a group made before a category of its name.
    account(
        &mut sample.book,
        "Groceries",
        AccountKind::Expense,
        EUR,
        Some(sample.rent),
    );
    sample
        .book
        .add_category_group(
            "Pets",
            GroupKind::Expense,
            &[GroupMember::Category(sample.food)],
        )
        .expect("group added");
    account(&mut sample.book, "Pets", AccountKind::Expense, EUR, None);
    // Names that look like a formula, or have a direction mark.
    account(
        &mut sample.book,
        "Rent - Food",
        AccountKind::Expense,
        EUR,
        None,
    );
    account(
        &mut sample.book,
        "Car\u{202E}",
        AccountKind::Expense,
        EUR,
        None,
    );
    let source = Source::read(&sample.book, autumn()).expect("read");
    let figure = |text: &str| worked_out(&source, text, autumn());
    assert_eq!(
        mistake(figure("=1 + Groceries")),
        "report.formula-ambiguous-name position=6 name=Groceries paths='Food > Groceries', 'Rent > Groceries'"
    );
    assert_eq!(figure("='Food > Groceries'"), Ok(eur(7_820)));
    assert_eq!(figure("='rent>groceries'"), Ok(eur(0)));
    assert_eq!(
        mistake(figure("=Pets")),
        "report.formula-ambiguous-name position=2 name=Pets paths='Pets', 'Pets'"
    );
    // Bare, it is a subtraction; in quotes, the category.
    assert_eq!(figure("=Rent - Food"), Ok(eur(231_180)));
    assert_eq!(figure("='Rent - Food'"), Ok(eur(0)));
    // The mark is part of the name only inside quotes, and shown as data.
    assert_eq!(figure("='Car\u{202E}'"), Ok(eur(0)));
    assert_eq!(
        mistake(figure("='Cat\u{202E}'")),
        "report.formula-unknown-name position=2 name=Cat\u{202E}"
    );
    assert_eq!(
        mistake(figure("=Travel + Food")),
        "report.formula-mixed-currencies position=11 first=Travel first_currency=USD \
         second=Food second_currency=EUR"
    );
}

#[test]
fn the_books_rounding_rule_rounds_an_amount_once_at_the_end() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn())
        .expect("read")
        .with_settings(&[], RoundingMode::TowardZero);
    // 88.20 / 8 = 11.025: cut off, not rounded up.
    assert_eq!(worked_out(&source, "=Food / 8", autumn()), Ok(eur(1_102)));
    let source = source.with_settings(&[], RoundingMode::HalfAwayFromZero);
    assert_eq!(worked_out(&source, "=Food / 8", autumn()), Ok(eur(1_103)));
    // A percent always rounds half away from zero, whatever the book says.
    let source = source.with_settings(&[], RoundingMode::TowardZero);
    assert_eq!(
        worked_out(&source, "=PERCENT_OF(Food; Salary)", autumn()),
        Ok(tenths(12))
    );
}

/// Rounds `numerator ÷ denominator` half away from zero, written apart from
/// the code under test.
fn half_away(numerator: i128, denominator: i128) -> i128 {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    if 2 * remainder.abs() >= denominator.abs() {
        quotient + numerator.signum() * denominator.signum()
    } else {
        quotient
    }
}

/// A number with two decimal places from its hundredths, as text.
fn written(hundredths: i64) -> String {
    let sign = if hundredths < 0 { "-" } else { "" };
    let size = hundredths.unsigned_abs();
    format!("({sign}{}.{:02})", size / 100, size % 100)
}

/// A small formula in the stored form, from a few pieces, for the round
/// trip through the user's forms.
fn stored_formula() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        (0u32..100_000, 0u32..1000).prop_map(|(whole, part)| format!("{whole}.{part}")),
        (0u32..1000).prop_map(|whole| whole.to_string()),
        Just("Pets".to_owned()),
        Just("'Eating, out; 1.5'".to_owned()),
        Just("Vet@2026-01..2026-03".to_owned()),
        Just("Vet@\"last month\"".to_owned()),
    ];
    (
        prop::sample::select(vec!["SUM", "MAX", "AVERAGE"]),
        prop::collection::vec(piece, 1..6),
        prop::sample::select(vec![";", "; ", " ;  "]),
    )
        .prop_map(|(function, pieces, separator)| {
            format!("={function}({}) * 1.25", pieces.join(separator))
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// `+`, `-` and `*` are exact; only the result is rounded, once.
    #[test]
    fn sums_and_products_are_exact(
        a in -100_000_000i64..100_000_000,
        b in -100_000_000i64..100_000_000,
        c in -100_000_000i64..100_000_000,
    ) {
        let text = format!("=({} + {}) * {} - {}", written(a), written(b), written(c), written(a));
        // In hundredths: (a + b) × c has four decimal places.
        let exact = (i128::from(a) + i128::from(b)) * i128::from(c) - i128::from(a) * 100;
        let expected = half_away(exact, 100);
        prop_assert_eq!(number(&text), Ok(hundredths(i64::try_from(expected).unwrap())));
    }

    /// A division rounds to ten places, the result once more to two.
    #[test]
    fn divisions_round_twice_as_written(
        a in -100_000_000_000i64..100_000_000_000,
        b in -100_000_000i64..100_000_000,
    ) {
        prop_assume!(b != 0);
        let text = format!("={} / {}", written(a), written(b));
        let ten_places = half_away(i128::from(a) * 10i128.pow(10), i128::from(b));
        let expected = half_away(ten_places, 10i128.pow(8));
        prop_assert_eq!(number(&text), Ok(hundredths(i64::try_from(expected).unwrap())));
        let percent = format!("=PERCENT_OF({}; {})", written(a), written(b));
        let expected = half_away(ten_places, 10i128.pow(7));
        prop_assert_eq!(number(&percent), Ok(tenths(i64::try_from(expected).unwrap())));
    }

    /// Stored, shown in either form and stored again, a formula is the
    /// same text and means the same.
    #[test]
    fn the_users_forms_round_trip(stored in stored_formula()) {
        for form in [NumberForm::DecimalComma, NumberForm::DecimalPoint] {
            let shown = Formula::to_users_form(&stored, form).unwrap();
            prop_assert_eq!(Formula::to_canonical(&shown, form).unwrap(), stored.clone());
            prop_assert_eq!(read_users(&shown, form), read(&stored));
        }
    }

    /// Whatever the text, reading and working it out ends with a result or
    /// a mistake, never a panic or a hang.
    #[test]
    fn any_text_is_a_result_or_a_mistake(
        text in "[=()+*/;,.%@'\" <>0-9a-zA-Z_-]{0,120}",
        noise in ".{0,60}",
    ) {
        for text in [text.clone(), format!("{text}{noise}"), noise.clone()] {
            if let Ok(formula) = Formula::parse(&text) {
                let _ = formula.evaluate(&empty(), autumn(), &Filter::default(), EUR);
            }
            let _ = Formula::to_users_form(&text, NumberForm::DecimalComma);
            let _ = read_users(&text, NumberForm::DecimalPoint);
        }
    }

    /// On random books, a name's figure is what the spending dataset shows,
    /// and a formula of names adds up exactly.
    #[test]
    fn names_agree_with_the_spending_dataset(entries in entries()) {
        let chart = chart();
        let transactions = build(&chart, &entries);
        let whole = period(date(2026, 1, 1), date(2026, 12, 31));
        let source = Source::from_parts(whole, chart.accounts.clone(), chart.tree.clone(), transactions);
        let each = spending_by_category(&source, whole, &Filter::default(), Level::Each).unwrap();
        let top = spending_by_category(&source, whole, &Filter::default(), Level::TopLevel).unwrap();
        let row = |results: &[SpendingByCategory], name: &str| {
            results
                .first()
                .and_then(|result| result.rows.iter().find(|row| row.path.last().is_some_and(|last| last == name)))
                .map_or(0, |row| row.total.minor_units())
        };
        let food = row(&top, "Food");
        let groceries = row(&each, "Groceries");
        let figure = |text: &str| worked_out(&source, text, whole);
        prop_assert_eq!(figure("=Food"), Ok(eur(food)));
        prop_assert_eq!(figure("=Food - Groceries"), Ok(eur(row(&each, "Food"))));
        prop_assert_eq!(figure("=SUM(Food; Groceries; 1.25)"), Ok(eur(food + groceries + 125)));
        // Ten places after the division, then whole cents.
        let ten_places = half_away((i128::from(food) + i128::from(groceries)) * 10i128.pow(8), 2);
        let average = half_away(ten_places, 10i128.pow(8));
        prop_assert_eq!(figure("=AVERAGE(Food; Groceries)"), Ok(eur(i64::try_from(average).unwrap())));
    }
}
