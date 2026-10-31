// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the formula field and the figures that use a formula need from the
//! open book (M6b.7, D-322): the result of a formula as text with what it
//! counted, its mistake in plain codes with the place marked, the functions
//! and the names to complete while the user types. No Qt in this file:
//! `formulas_model.rs` hands the result to QML.
//!
//! A formula is worked out by `kassensturz-report` and reaches QML as text
//! (D-065); the only numbers are the per-mille of a progress bar, which
//! place a mark and are never read back. The Rust side writes no message
//! (D-034): a mistake is a code with its parameters, and `FormError.qml`
//! has the words.

use kassensturz_core::{CurrencyCode, Report};
use kassensturz_report::{
    Counted, FUNCTIONS, Formula, FormulaValue, KnownName, NumberForm, Period, ReportError,
    Source, TextFormat,
};
use kassensturz_store::Book;

use crate::charts::{AppText, ChartsError, period};
use crate::errors::{ErrorText, report_text};
use crate::format::Locale;
use crate::scope::{FilterView, Scope};

/// The most characters of the formula shown before and after a mistake.
const EXCERPT_CHARS: usize = 24;

/// The most names listed inside a category in "counts Pets (with Vet, ...)".
const INSIDE_SHOWN: usize = 3;

/// The most names offered while the user types.
const SUGGESTIONS: usize = 6;

/// What the screen asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    /// `this-month`, `last-month`, `last-12-months`, `this-year`,
    /// `last-year` or a month as `YYYY-MM`: the figure's period.
    pub period: String,
    /// The formula.
    pub text: String,
    /// Whether `text` is written in the user's number format, as the user
    /// types it, rather than in the form the book stores (D-295).
    pub typed: bool,
    /// For a progress bar: the formula of what a full bar stands for, in the
    /// same form as `text`, or empty for none. `None` when the figure is
    /// not shown as a bar.
    pub target: Option<String>,
    /// The filters the dashboard puts on the component (M6c.7, D-350).
    pub scope: Scope,
}

/// One category or group a formula counted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// Its name; a category's path from the top-level category down, joined
    /// by " > ".
    pub name: String,
    /// The first names of what is counted with it.
    pub inside: Vec<String>,
    /// How many more there are than `inside` lists.
    pub inside_more: usize,
    /// Whether it was counted for the figure's own period.
    pub own_period: bool,
    /// The month it was counted for when that is one whole month, else
    /// empty.
    pub month: String,
    /// The first and the last day it was counted for.
    pub first_day: String,
    /// See `first_day`.
    pub last_day: String,
}

/// The result of a formula.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    /// `amount`, `percent` or `number`.
    pub kind: &'static str,
    /// The figure.
    pub value: String,
    /// The figure with all its decimal places, also when the user chose to
    /// see fewer; the same as `value` for a percent or a number.
    pub exact_value: String,
    /// Whether the figure is below zero.
    pub negative: bool,
    /// The currency code of an amount; empty for the others.
    pub currency: String,
    /// For a progress bar: 0 to 1000, how full it is: a percent as it is, or
    /// the figure as a share of the target.
    pub permille: Option<i32>,
    /// The target's figure as text, when the bar has one.
    pub target: String,
    /// What it counted.
    pub counted: Vec<Line>,
    /// The marks and notes about the filters.
    pub filter: FilterView,
}

/// A formula that cannot be worked out, with the place marked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mistake {
    /// The code and its parameters, including how the user's number format
    /// writes a separator (`separator`) and a decimal mark (`decimal`).
    pub error: ErrorText,
    /// The characters before the place of the mistake, with "…" in front
    /// when there are more.
    pub before: String,
    /// The characters from the place on, with "…" at the end when there are
    /// more.
    pub after: String,
}

/// What a formula gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// There is no formula.
    Empty,
    /// The figure and what it counted.
    Shown(Shown),
    /// The filters choose nothing, so there is no figure: the component
    /// says so instead of drawing zeros (D-350).
    Nothing(FilterView),
    /// Why there is no figure.
    Mistake(Mistake),
}

/// How the user's number format writes a formula (D-295).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Writing {
    /// Between the parts of a call.
    pub separator: &'static str,
    /// Between the whole part of a number and its decimal places.
    pub decimal: &'static str,
}

pub(crate) fn form(locale: &impl Locale) -> NumberForm {
    if locale.decimal_point() == "," {
        NumberForm::DecimalComma
    } else {
        NumberForm::DecimalPoint
    }
}

/// The form formulas are written in for the user's number format.
pub fn number_form(locale: &impl Locale) -> Writing {
    match form(locale) {
        NumberForm::DecimalComma => Writing {
            separator: ";",
            decimal: ",",
        },
        NumberForm::DecimalPoint => Writing {
            separator: ",",
            decimal: ".",
        },
    }
}

fn excerpt(text: &str, position: usize) -> (String, String) {
    let chars: Vec<char> = text.chars().collect();
    let at = position.saturating_sub(1).min(chars.len());
    let from = at.saturating_sub(EXCERPT_CHARS);
    let to = (at + EXCERPT_CHARS).min(chars.len());
    let dots = |cut: bool| if cut { "…" } else { "" };
    (
        format!(
            "{}{}",
            dots(from > 0),
            chars[from..at].iter().collect::<String>()
        ),
        format!(
            "{}{}",
            chars[at..to].iter().collect::<String>(),
            dots(to < chars.len())
        ),
    )
}

fn mistake(
    book: &Book,
    locale: &impl Locale,
    text: &str,
    error: &ReportError,
    in_target: bool,
) -> Mistake {
    let report = error.report();
    let mut shown = report_text(book, &report, locale);
    let writing = number_form(locale);
    shown
        .params
        .push(("separator", writing.separator.to_owned()));
    shown.params.push(("decimal", writing.decimal.to_owned()));
    // A mistake of a progress bar's target says so (like a Part's number in
    // a transaction's messages).
    if in_target {
        shown.params.push(("target", "yes".to_owned()));
    }
    let position = shown
        .params
        .iter()
        .find(|(name, _)| *name == "position")
        .and_then(|(_, value)| value.parse::<usize>().ok());
    let (before, after) = position.map_or_else(Default::default, |at| excerpt(text, at));
    Mistake {
        error: shown,
        before,
        after,
    }
}

fn line(
    text: &AppText<'_, impl Locale>,
    locale: &impl Locale,
    counted: &Counted,
    own: Period,
) -> Line {
    let first = counted.period.first();
    let last = counted.period.last();
    let whole_month = first.day() == 1 && last == first.last_of_month();
    Line {
        name: counted.path.join(" > "),
        inside: counted.inside.iter().take(INSIDE_SHOWN).cloned().collect(),
        inside_more: counted.inside.len().saturating_sub(INSIDE_SHOWN),
        own_period: counted.period == own,
        month: if whole_month {
            text.month(first)
        } else {
            String::new()
        },
        first_day: locale.short_date(first),
        last_day: locale.short_date(last),
    }
}

/// The currency of a result with no money at all: the first the book uses.
fn fallback_currency(book: &Book) -> CurrencyCode {
    book.currencies()
        .ok()
        .and_then(|used| used.first().map(|currency| currency.code()))
        .unwrap_or(CurrencyCode::new("EUR").expect("EUR is a valid code"))
}

/// A formula worked out: its figure and what it counted.
struct Worked {
    value: FormulaValue,
    counted: Vec<Line>,
    filter: FilterView,
}

fn parse(text: &str, typed: bool, locale: &impl Locale) -> Result<Formula, ReportError> {
    if typed {
        Formula::parse_users(text, form(locale))
    } else {
        Formula::parse(text)
    }
}

fn work(
    book: &Book,
    locale: &impl Locale,
    formula: &Formula,
    figure: Period,
    scope: &Scope,
) -> Result<Worked, ReportError> {
    let source = Source::read(book, formula.span(figure)?)?;
    let (filter, view) = scope.resolve(book, &source)?;
    let result = formula.evaluate(&source, figure, &filter, fallback_currency(book))?;
    let text = AppText::new(book, locale);
    let counted = result
        .counted
        .iter()
        .map(|counted| line(&text, locale, counted, figure))
        .collect();
    Ok(Worked {
        value: result.value,
        counted,
        filter: view,
    })
}

/// The figure as text, with the fill of a bar and the text of its target.
fn shown(
    book: &Book,
    locale: &impl Locale,
    worked: Worked,
    permille: Option<i32>,
    target: String,
) -> Shown {
    let text = AppText::new(book, locale);
    let counted = worked.counted;
    let filter = worked.filter;
    match worked.value {
        FormulaValue::Amount(money) => Shown {
            kind: "amount",
            value: text.amount(money),
            exact_value: text.exact(money),
            negative: money.is_negative(),
            currency: money.currency().to_string(),
            permille,
            target,
            counted,
            filter,
        },
        FormulaValue::Percent(percent) => {
            let shown = text.percent(percent);
            Shown {
                kind: "percent",
                exact_value: shown.clone(),
                value: shown,
                negative: percent.tenths() < 0,
                currency: String::new(),
                permille,
                target,
                counted,
                filter,
            }
        }
        FormulaValue::Number { hundredths } => {
            let shown = text.number(hundredths);
            Shown {
                kind: "number",
                exact_value: shown.clone(),
                value: shown,
                negative: hundredths < 0,
                currency: String::new(),
                permille,
                target,
                counted,
                filter,
            }
        }
    }
}

/// The text of a figure, as a card writes it.
fn figure_text(book: &Book, locale: &impl Locale, value: FormulaValue) -> String {
    let text = AppText::new(book, locale);
    match value {
        FormulaValue::Amount(money) => text.amount(money),
        FormulaValue::Percent(percent) => text.percent(percent),
        FormulaValue::Number { hundredths } => text.number(hundredths),
    }
}

/// A bar's fill in per-mille: 0 to 1000, never more or less.
fn permille(tenths: i64) -> i32 {
    i32::try_from(tenths.clamp(0, 1000)).unwrap_or(0)
}

/// Works the bar out: the figure and, if the bar has a target, the figure
/// as a share of it. A mistake in the target is told as one in the target.
fn bar(
    book: &Book,
    locale: &impl Locale,
    request: &Request,
    target: &str,
    figure: Period,
) -> Outcome {
    let worked = match parse(&request.text, request.typed, locale)
        .and_then(|formula| work(book, locale, &formula, figure, &request.scope))
    {
        Ok(worked) => worked,
        Err(error) => return Outcome::Mistake(mistake(book, locale, &request.text, &error, false)),
    };
    if worked.filter.nothing {
        return Outcome::Nothing(worked.filter);
    }
    if target.trim().is_empty() {
        // Without a target only a percent fills a bar.
        let fill = match worked.value {
            FormulaValue::Percent(percent) => Some(permille(percent.tenths())),
            _ => None,
        };
        return Outcome::Shown(shown(book, locale, worked, fill, String::new()));
    }
    let measured = match parse(target, request.typed, locale)
        .and_then(|formula| work(book, locale, &formula, figure, &request.scope))
    {
        Ok(measured) => measured,
        Err(error) => return Outcome::Mistake(mistake(book, locale, target, &error, true)),
    };
    match worked.value.fill(measured.value) {
        Ok(fill) => {
            let target_text = figure_text(book, locale, measured.value);
            Outcome::Shown(shown(
                book,
                locale,
                worked,
                Some(permille(fill.tenths())),
                target_text,
            ))
        }
        // The mistake names the target itself; no "Target:" in front.
        Err(error) => Outcome::Mistake(mistake(book, locale, target, &error, false)),
    }
}

/// Works the formula of `request` out on `book`. A formula that is empty
/// has no outcome to show; one with a mistake says where.
pub fn show(book: &Book, locale: &impl Locale, request: &Request) -> Result<Outcome, ChartsError> {
    if request.text.trim().is_empty() {
        return Ok(Outcome::Empty);
    }
    let figure = period(book, &request.period, locale.today())?;
    if let Some(target) = &request.target {
        return Ok(bar(book, locale, request, target, figure));
    }
    Ok(
        match parse(&request.text, request.typed, locale)
            .and_then(|formula| work(book, locale, &formula, figure, &request.scope))
        {
            Ok(worked) if worked.filter.nothing => Outcome::Nothing(worked.filter),
            Ok(worked) => Outcome::Shown(shown(book, locale, worked, None, String::new())),
            Err(error) => Outcome::Mistake(mistake(book, locale, &request.text, &error, false)),
        },
    )
}

/// The formula the user typed, in the form the book stores. It fails when
/// it cannot even be read as a formula (a quote that never closes, a
/// character formulas don't use): then nothing is stored, and the field
/// says why.
pub fn canonical(locale: &impl Locale, typed: &str) -> Result<String, ReportError> {
    Formula::to_canonical(typed, form(locale))
}

/// A stored formula in the form of the user's number format. One that
/// cannot be read is shown as it is stored.
pub fn users_form(form: NumberForm, stored: &str) -> String {
    Formula::to_users_form(stored, form).unwrap_or_else(|_| stored.to_owned())
}

/// A name to complete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    /// What to type: the name as a formula writes it.
    pub insert: String,
    /// What to show: the category's path, or the group's name.
    pub label: String,
    /// Whether it is one of My groups.
    pub group: bool,
}

/// What can be completed at the cursor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Completion {
    /// Where the word begins, in UTF-16 units as QML counts.
    pub start: usize,
    /// Where it ends.
    pub end: usize,
    /// The names that fit, the ones that start with it first.
    pub names: Vec<Suggestion>,
}

/// Where the word at `cursor` (a character count) begins and ends, and what
/// has been typed of it. `None` when the cursor is not in a name: in a
/// number, in a period in double quotes, or after nothing.
fn word_at(chars: &[char], cursor: usize) -> Option<(usize, usize, String)> {
    let cursor = cursor.min(chars.len());
    let mut single: Option<usize> = None;
    let mut double = false;
    for (index, &c) in chars[..cursor].iter().enumerate() {
        match c {
            '\'' if !double => single = if single.is_some() { None } else { Some(index) },
            '"' if single.is_none() => double = !double,
            _ => {}
        }
    }
    if double {
        return None;
    }
    if let Some(open) = single {
        let typed: String = chars[open + 1..cursor].iter().collect();
        // A closing quote right after the cursor belongs to the name.
        let end = if chars.get(cursor) == Some(&'\'') {
            cursor + 1
        } else {
            cursor
        };
        return Some((open, end, typed.replace("''", "'")));
    }
    let mut start = cursor;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }
    // A bare name starts with a letter (D-292): a number is not one.
    if start == cursor || !chars[start].is_alphabetic() {
        return None;
    }
    Some((start, cursor, chars[start..cursor].iter().collect()))
}

fn utf16(chars: &[char], count: usize) -> usize {
    chars[..count].iter().map(|c| c.len_utf16()).sum()
}

fn fits(known: &KnownName, typed: &str) -> Option<bool> {
    let path = known.path.join(" > ").to_lowercase();
    let typed = typed.trim().to_lowercase();
    if !path.contains(&typed) {
        return None;
    }
    let last = known.path.last().map(|name| name.to_lowercase());
    Some(last.is_some_and(|name| name.starts_with(&typed)))
}

/// The names of the book that fit the word at `cursor` (a UTF-16 position,
/// as QML counts) in `text`, so the field can complete what is typed.
pub fn complete(book: &Book, locale: &impl Locale, text: &str, cursor: usize) -> Completion {
    let chars: Vec<char> = text.chars().collect();
    // QML counts UTF-16 units; the word is found in characters.
    let mut units = 0;
    let mut at = chars.len();
    for (index, c) in chars.iter().enumerate() {
        if units >= cursor {
            at = index;
            break;
        }
        units += c.len_utf16();
    }
    let Some((start, end, typed)) = word_at(&chars, at) else {
        return Completion::default();
    };
    let Ok(this_month) = Period::this_month(locale.today()) else {
        return Completion::default();
    };
    let Ok(source) = Source::read(book, this_month) else {
        return Completion::default();
    };
    let mut starting = Vec::new();
    let mut containing = Vec::new();
    for known in Formula::known_names(&source) {
        match fits(&known, &typed) {
            Some(true) => starting.push(known),
            Some(false) => containing.push(known),
            None => {}
        }
    }
    let names = starting
        .into_iter()
        .chain(containing)
        .take(SUGGESTIONS)
        .map(|known| Suggestion {
            label: known.path.join(" > "),
            insert: known.written,
            group: known.group,
        })
        .collect();
    Completion {
        start: utf16(&chars, start),
        end: utf16(&chars, end),
        names,
    }
}

/// A function of the list, with its arguments.
pub fn functions() -> impl Iterator<Item = (&'static str, &'static [&'static str])> {
    FUNCTIONS
        .iter()
        .map(|function| (function.name, function.arguments))
}

#[cfg(test)]
pub(crate) mod tests {
    use kassensturz_core::{
        Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, GroupKind, GroupMember,
        Money, Split, Transaction, TransactionId,
    };

    use super::*;
    use crate::register::tests::TestLocale;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    /// `TestLocale` with a decimal point, as an English number format.
    pub(crate) struct PointLocale;

    impl Locale for PointLocale {
        fn amount_format(&self, currency: CurrencyCode) -> crate::format::AmountFormat {
            let mut format = TestLocale.amount_format(currency);
            format.decimal_point = ".".into();
            format
        }
        fn decimal_point(&self) -> String {
            ".".into()
        }
        fn group_digits(&self, value: u64) -> String {
            TestLocale.group_digits(value)
        }
        fn group_separator(&self) -> String {
            ",".into()
        }
        fn short_date(&self, date: Date) -> String {
            TestLocale.short_date(date)
        }
        fn month_and_year(&self, date: Date) -> String {
            TestLocale.month_and_year(date)
        }
        fn read_date(&self, text: &str) -> Option<Date> {
            TestLocale.read_date(text)
        }
        fn today(&self) -> Date {
            TestLocale.today()
        }
    }

    fn account(
        book: &mut Book,
        name: &str,
        kind: AccountKind,
        parent: Option<AccountId>,
    ) -> AccountId {
        let id = AccountId::generate();
        book.add_account(&Account {
            id,
            name: name.into(),
            kind,
            currency: EUR,
            account_type: (kind == AccountKind::Asset).then_some(AccountType::Checking),
            parent,
            closed: false,
            number: None,
            notes: None,
            institution: None,
        })
        .expect("account added");
        id
    }

    /// Pets (with Vet inside), Travel and a Salary, in September 2026,
    /// which is last month for `TestLocale`, and one more Pets in October.
    pub(crate) fn book() -> Book {
        let mut book = Book::create_in_memory().expect("book created");
        book.use_currency(EUR).expect("currency used");
        let checking = account(&mut book, "Checking account", AccountKind::Asset, None);
        let pets = account(&mut book, "Pets", AccountKind::Expense, None);
        let vet = account(&mut book, "Vet", AccountKind::Expense, Some(pets));
        let travel = account(&mut book, "Travel", AccountKind::Expense, None);
        let salary = account(&mut book, "Salary", AccountKind::Income, None);
        let add = |book: &mut Book, month: i8, other: AccountId, amount: i64| {
            let transaction = Transaction::new(
                TransactionId::generate(),
                Date::new(2026, month, 3).expect("a date"),
                None,
                vec![
                    Split::new(checking, Money::from_minor_units(amount, EUR)),
                    Split::new(other, Money::from_minor_units(-amount, EUR)),
                ],
            )
            .expect("balanced");
            book.add_transaction(&transaction).expect("added");
        };
        add(&mut book, 9, salary, 200_000);
        add(&mut book, 9, pets, -5_000);
        add(&mut book, 9, vet, -3_000);
        add(&mut book, 9, travel, -12_040);
        add(&mut book, 10, pets, -1_000);
        book
    }

    fn request(text: &str) -> Request {
        Request {
            period: "last-month".into(),
            text: text.into(),
            typed: true,
            target: None,
            scope: Scope::default(),
        }
    }

    fn shown(book: &Book, locale: &impl Locale, request: &Request) -> Shown {
        match show(book, locale, request).expect("a request") {
            Outcome::Shown(shown) => shown,
            other => panic!("{request:?} gave {other:?}"),
        }
    }

    fn mistake_of(book: &Book, locale: &impl Locale, request: &Request) -> Mistake {
        match show(book, locale, request).expect("a request") {
            Outcome::Mistake(mistake) => mistake,
            other => panic!("{request:?} gave {other:?}"),
        }
    }

    fn param<'a>(mistake: &'a Mistake, name: &str) -> Option<&'a str> {
        mistake
            .error
            .params
            .iter()
            .find(|(found, _)| *found == name)
            .map(|(_, value)| value.as_str())
    }

    #[test]
    fn a_formula_is_worked_out_as_text_with_what_it_counted() {
        let book = book();
        let pets = shown(&book, &TestLocale, &request("=Pets"));
        // Pets holds Vet; Vet is not counted twice.
        assert_eq!(
            (pets.kind, pets.value.as_str(), pets.currency.as_str()),
            ("amount", "80,00 €", "EUR")
        );
        assert_eq!(pets.exact_value, "80,00 €");
        assert!(!pets.negative);
        assert_eq!(pets.permille, None);
        assert_eq!(pets.counted.len(), 1);
        let line = &pets.counted[0];
        assert_eq!(line.name, "Pets");
        assert_eq!(line.inside, ["Vet"]);
        assert_eq!(line.inside_more, 0);
        assert!(line.own_period);

        let left = shown(&book, &TestLocale, &request("=Pets - Travel"));
        assert_eq!(left.value, "-40,40 €");
        assert!(left.negative);
        let names: Vec<&str> = left.counted.iter().map(|line| line.name.as_str()).collect();
        assert_eq!(names, ["Pets", "Travel"]);

        let share = shown(&book, &TestLocale, &request("=PERCENT_OF(Pets; Salary)"));
        assert_eq!((share.kind, share.value.as_str()), ("percent", "4,0 %"));
        let plain = shown(&book, &TestLocale, &request("=COUNT(Pets) / 4"));
        assert_eq!((plain.kind, plain.value.as_str()), ("number", "0,5"));
        // The stored form reads the same figure.
        let mut stored = request("=PERCENT_OF(Pets; Salary)");
        stored.typed = false;
        assert_eq!(shown(&book, &TestLocale, &stored).value, "4,0 %");
    }

    #[test]
    fn a_name_that_counts_another_month_says_which() {
        let book = book();
        let october = shown(
            &book,
            &TestLocale,
            &request("=Pets@2026-10 + Travel@2026-09-01..2026-09-15"),
        );
        let lines: Vec<(&str, bool, &str)> = october
            .counted
            .iter()
            .map(|line| (line.name.as_str(), line.own_period, line.month.as_str()))
            .collect();
        assert_eq!(
            lines,
            [("Pets", false, "October 2026"), ("Travel", false, "")]
        );
        assert_eq!(october.counted[1].first_day, "2026-09-01");
        assert_eq!(october.counted[1].last_day, "2026-09-15");
    }

    #[test]
    fn the_user_types_in_the_form_of_their_number_format() {
        let book = book();
        // A decimal comma separates arguments with ;, a decimal point with ,.
        assert_eq!(
            shown(&book, &TestLocale, &request("=ROUND(Pets / 3; 1)")).value,
            "26,70 €"
        );
        assert_eq!(
            shown(&book, &PointLocale, &request("=ROUND(Pets / 3, 1)")).value,
            "26.70 €"
        );
        // ; is accepted in every number format (D-295).
        assert_eq!(
            shown(&book, &PointLocale, &request("=ROUND(Pets / 3; 1)")).value,
            "26.70 €"
        );
        assert_eq!(
            shown(&book, &TestLocale, &request("=Pets * 1,5")).value,
            "120,00 €"
        );
        // The comma in a call of the decimal-comma form is a mistake that
        // says which separator to use.
        let wrong = mistake_of(&book, &TestLocale, &request("=SUM(Pets, Travel)"));
        assert_eq!(wrong.error.code, "report.formula-syntax");
        assert_eq!(param(&wrong, "separator"), Some(";"));
        assert_eq!(param(&wrong, "decimal"), Some(","));
        let wrong = mistake_of(&book, &PointLocale, &request("=Pets * 1,5"));
        assert_eq!(param(&wrong, "separator"), Some(","));
        assert_eq!(param(&wrong, "decimal"), Some("."));
        // What is stored does not depend on the form.
        assert_eq!(
            canonical(&TestLocale, "=ROUND(Pets * 1,5; 2)")
                .as_deref()
                .ok(),
            Some("=ROUND(Pets * 1.5; 2)")
        );
        assert_eq!(
            canonical(&PointLocale, "=ROUND(Pets * 1.5, 2)")
                .as_deref()
                .ok(),
            Some("=ROUND(Pets * 1.5; 2)")
        );
        assert_eq!(
            users_form(form(&TestLocale), "=ROUND(Pets * 1.5; 2)"),
            "=ROUND(Pets * 1,5; 2)"
        );
        assert_eq!(
            users_form(form(&PointLocale), "=ROUND(Pets * 1.5; 2)"),
            "=ROUND(Pets * 1.5, 2)"
        );
    }

    #[test]
    fn a_mistake_is_a_code_with_the_place_marked() {
        let book = book();
        let unknown = mistake_of(&book, &TestLocale, &request("=Pets - Nothing"));
        assert_eq!(unknown.error.code, "report.formula-unknown-name");
        assert_eq!(param(&unknown, "position"), Some("9"));
        assert_eq!(param(&unknown, "name"), Some("Nothing"));
        assert_eq!(
            (unknown.before.as_str(), unknown.after.as_str()),
            ("=Pets - ", "Nothing")
        );
        let missing = mistake_of(&book, &TestLocale, &request("=Pets -"));
        assert_eq!(missing.error.code, "report.formula-syntax");
        assert_eq!(param(&missing, "reason"), Some("expected-value"));
        let divided = mistake_of(&book, &TestLocale, &request("=Pets / (Salary - Salary)"));
        assert_eq!(divided.error.code, "report.formula-no-value");
        // Long formulas show a stretch around the place, not the whole.
        let long = format!("={}Nothing", "1 + ".repeat(20));
        let far = mistake_of(&book, &TestLocale, &request(&long));
        assert!(far.before.starts_with('…') && far.before.chars().count() <= 25);
        assert!(far.after.starts_with("Nothing"));
        // Over a limit there is no place.
        let big = mistake_of(
            &book,
            &TestLocale,
            &request(&format!("={}", "1+".repeat(600))),
        );
        assert_eq!(big.error.code, "report.formula-too-big");
        assert_eq!((big.before.as_str(), big.after.as_str()), ("", ""));
    }

    #[test]
    fn nothing_is_asked_of_an_empty_formula_or_a_bad_period() {
        let book = book();
        assert_eq!(
            show(&book, &TestLocale, &request("  ")).expect("a request"),
            Outcome::Empty
        );
        let mut bad = request("=Pets");
        bad.period = "yesterday".into();
        assert!(show(&book, &TestLocale, &bad).is_err());
    }

    #[test]
    fn a_bar_is_filled_by_a_percent_or_by_the_figure_over_its_target() {
        let book = book();
        let bar = |text: &str, target: &str| Request {
            typed: false,
            target: Some(target.into()),
            scope: Scope::default(),
            ..request(text)
        };
        // A percent fills the bar as it is: 4,0 % is 40 per-mille.
        let percent = shown(&book, &TestLocale, &bar("=PERCENT_OF(Pets; Salary)", ""));
        assert_eq!(percent.permille, Some(40));
        assert_eq!(percent.target, "");
        // An amount alone fills nothing.
        let amount = shown(&book, &TestLocale, &bar("=Pets", ""));
        assert_eq!(amount.permille, None);
        // With a target, 80,00 of 200,00 is 40 %.
        let measured = shown(&book, &TestLocale, &bar("=Pets", "=Salary / 10"));
        assert_eq!(measured.value, "80,00 €");
        assert_eq!(measured.permille, Some(400));
        assert_eq!(measured.target, "200,00 €");
        // Over the target the bar is full and the figure says more.
        let over = shown(&book, &TestLocale, &bar("=Salary", "=Pets"));
        assert_eq!(over.permille, Some(1000));
        assert_eq!(over.value, "2000,00 €");
        // Below zero the bar is empty.
        let below = shown(&book, &TestLocale, &bar("=Pets - Travel", "=Salary / 10"));
        assert_eq!(below.permille, Some(0));
        assert!(below.negative);
    }

    #[test]
    fn a_bar_says_what_is_wrong_with_its_target() {
        let book = book();
        let bar = |text: &str, target: &str| Request {
            typed: false,
            target: Some(target.into()),
            scope: Scope::default(),
            ..request(text)
        };
        let kinds = mistake_of(&book, &TestLocale, &bar("=Pets", "=400"));
        assert_eq!(kinds.error.code, "report.formula-target");
        assert_eq!(param(&kinds, "reason"), Some("kinds"));
        assert_eq!(param(&kinds, "target"), None);
        let zero = mistake_of(&book, &TestLocale, &bar("=Pets", "=Salary - Salary"));
        assert_eq!(param(&zero, "reason"), Some("zero"));
        // A mistake in the target is told as one in the target.
        let nothing = mistake_of(&book, &TestLocale, &bar("=Pets", "=Nothing"));
        assert_eq!(nothing.error.code, "report.formula-unknown-name");
        assert_eq!(param(&nothing, "target"), Some("yes"));
        assert_eq!(nothing.after, "Nothing");
        // A mistake in the formula is not.
        let formula = mistake_of(&book, &TestLocale, &bar("=Nothing", "=Pets"));
        assert_eq!(param(&formula, "target"), None);
    }

    fn names(completion: &Completion) -> Vec<&str> {
        completion
            .names
            .iter()
            .map(|name| name.insert.as_str())
            .collect()
    }

    #[test]
    fn names_that_fit_the_word_at_the_cursor_complete_it() {
        let mut book = book();
        let pets = book
            .categories()
            .expect("categories")
            .into_iter()
            .find(|entry| entry.path == ["Pets"])
            .expect("Pets")
            .category
            .id;
        book.add_category_group(
            "Pet things",
            GroupKind::Expense,
            &[GroupMember::Category(pets)],
        )
        .expect("group added");
        let at = |text: &str| complete(&book, &TestLocale, text, text.encode_utf16().count());
        let pe = at("=Pe");
        // Those that start with the word first, the groups before the
        // categories; the group has a space, so it is written in quotes.
        // Vet is found by its path, Pets > Vet.
        assert_eq!(names(&pe), ["'Pet things'", "Pets", "Vet"]);
        assert_eq!((pe.start, pe.end), (1, 3));
        assert!(pe.names[0].group && !pe.names[1].group);
        // A name is found anywhere in its path.
        // Vet starts with the word, Travel only holds it.
        assert_eq!(names(&at("=ve")), ["Vet", "Travel"]);
        assert_eq!(at("=ve").names[0].label, "Pets > Vet");
        // Typing in the middle of a formula completes the word at the cursor.
        let text = "=Pe - Travel";
        let middle = complete(&book, &TestLocale, text, 3);
        assert_eq!(names(&middle), ["'Pet things'", "Pets", "Vet"]);
        assert_eq!((middle.start, middle.end), (1, 3));
        // In single quotes the quote is part of what is replaced.
        let quoted = at("=SUM(Pets; 'Pet t");
        assert_eq!(names(&quoted), ["'Pet things'"]);
        assert_eq!((quoted.start, quoted.end), (11, 17));
        let closed = complete(&book, &TestLocale, "='Pet t' + 1", 7);
        assert_eq!((closed.start, closed.end), (1, 8));
        // Nothing is offered in a number, after nothing, or in the double
        // quotes of a period.
        assert!(at("=12").names.is_empty());
        assert!(at("=1.5").names.is_empty());
        assert!(at("=").names.is_empty());
        assert!(at("=Pets@\"last mo").names.is_empty());
        assert!(at("=Pets + ").names.is_empty());
        assert!(at("=Zzz").names.is_empty());
        // At most six, and a cursor past the end is the end.
        assert!(complete(&book, &TestLocale, "=e", 500).names.len() <= SUGGESTIONS);
    }

    #[test]
    fn positions_are_counted_the_way_qml_counts_them() {
        let mut book = book();
        // A name with a character outside the first plane takes two UTF-16
        // units in QML.
        let id = AccountId::generate();
        book.add_account(&Account {
            id,
            name: "🐟 food".into(),
            kind: AccountKind::Expense,
            currency: EUR,
            account_type: None,
            parent: None,
            closed: false,
            number: None,
            notes: None,
            institution: None,
        })
        .expect("account added");
        let text = "='🐟 fo";
        let units = text.encode_utf16().count();
        assert_eq!(units, 7);
        let completion = complete(&book, &TestLocale, text, units);
        assert_eq!(names(&completion), ["'🐟 food'"]);
        assert_eq!((completion.start, completion.end), (1, 7));
        // A cursor in the middle of a pair counts as after the character.
        let middle = complete(&book, &TestLocale, text, 3);
        assert_eq!((middle.start, middle.end), (1, 4));
    }

    #[test]
    fn the_function_list_has_every_function_once() {
        let listed: Vec<&str> = functions().map(|(name, _)| name).collect();
        let mut sorted = listed.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), listed.len());
        assert_eq!(listed.len(), 18);
        assert_eq!(listed[0], "SUM");
        assert_eq!(number_form(&TestLocale).separator, ";");
        assert_eq!(number_form(&PointLocale).separator, ",");
    }

    #[test]
    fn plain_numbers_are_written_in_the_users_format() {
        let book = book();
        let number = |text: &str| shown(&book, &TestLocale, &request(text)).value;
        assert_eq!(number("=12"), "12");
        assert_eq!(number("=12,5"), "12,5");
        assert_eq!(number("=12,25"), "12,25");
        assert_eq!(number("=-0,05"), "-0,05");
        assert_eq!(number("=0"), "0");
        assert_eq!(
            shown(&book, &PointLocale, &request("=1234.5")).value,
            "1234.5"
        );
    }
}
