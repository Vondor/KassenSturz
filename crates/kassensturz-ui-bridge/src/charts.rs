// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Charts of the open book, built from the datasets of `kassensturz-report`
//! and the chart models of `kassensturz-charts` (M6a.3, D-276). No Qt in
//! this file: `charts_model.rs` hands the result to QML.
//!
//! QML gets every label, name and amount as text (D-065). The only numbers
//! are the drawing values of [`ChartProvider::prepare`], which place marks
//! and are never read back.

use kassensturz_charts::{
    BalancesBy, ChartError, ChartKind, ChartModel, ChartProvider, Prepared,
    account_balances as balances_chart, cash_flow_by_month as cash_flow_chart, category_over_time,
    income_expense_by_month, net_worth_over_time as net_worth_chart, spending_by_category,
};
use kassensturz_core::{CurrencyCode, Date, ErrorReport, GroupMember, Money, Report};
use kassensturz_report::{
    ACCOUNT_BALANCES, CASH_FLOW_BY_MONTH, CATEGORY_OVER_TIME, Column, Filter,
    INCOME_EXPENSE_BY_MONTH, Level, NET_WORTH_OVER_TIME, Overlap, Percent, Period, ReportError,
    SPENDING_BY_CATEGORY, SharedCategory, Shown, Source, TextFormat,
    account_balances as balances_dataset, cash_flow_by_month as cash_flow_dataset,
    category_over_time as over_time_dataset, columns_over_time, columns_span, columns_spending,
    income_expense_by_month as monthly_dataset, net_worth_over_time as net_worth_dataset,
    shared_categories, spending_by_category_shown as spending_dataset,
};
use kassensturz_store::Book;

use crate::format::Locale;
use crate::register::AmountWriter;
use crate::session;

/// What the user asked for, by the fixed names QML uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    /// `spending_by_category`, `income_expense_by_month`,
    /// `category_over_time`, `account_balances`, `net_worth_over_time` or
    /// `cash_flow_by_month`.
    pub dataset: String,
    /// `line`, `bar`, `stacked-bar`, `area`, `pie` or `donut`.
    pub kind: String,
    /// `this-month`, `last-month`, `last-12-months`, `this-year` or
    /// `last-year`.
    pub period: String,
    /// For spending: `top-level` adds subcategories to the category they sit
    /// in, `each` lists every category, `groups` shows each of `groups` as
    /// one row and every other category on its own. For the balances of the
    /// accounts: `each` lists every account, `account-kind` what you have,
    /// what you owe and what you are worth.
    pub grouping: String,
    /// For spending: the IDs of My groups to show as one row each
    /// (`params.groups`), as the document writes them.
    pub groups: Vec<String>,
    /// For spending: the IDs of the categories and groups the chart is
    /// limited to (`params.categories`, D-331); empty for all.
    pub categories: Vec<String>,
    /// For `category_over_time`: the IDs of the categories and groups to
    /// draw, one series each (`params.series`).
    pub series: Vec<String>,
    /// Show the hidden categories anyway (`props.show_hidden`).
    pub show_hidden: bool,
    /// The columns of the chart (`params.columns`, D-339): each a name and
    /// the IDs of the categories and groups under it. When there are some,
    /// they are the series or rows and the dataset's own are not used.
    pub columns: Vec<(String, Vec<String>, String)>,
}

/// What a request gave: the charts, and how many of the IDs it named the
/// book no longer knows (the screen says "A group in this chart no longer
/// exists", D-291).
#[derive(Debug)]
pub struct ChartSet {
    /// One chart per currency; empty when there was nothing to show.
    pub charts: Vec<(ChartModel, Prepared)>,
    /// IDs in `groups`, `series` or a column that the book doesn't have.
    pub missing: usize,
    /// The categories that more than one column counts, so the screen can
    /// say plainly that they are counted twice (D-339).
    pub shared: Vec<SharedCategory>,
    /// What each column adds up to over the period, one text per currency,
    /// in the order of the columns; a column with no money has none (D-341).
    pub totals: Vec<(String, Vec<String>)>,
}

/// Why a request couldn't be answered.
#[derive(Debug)]
pub enum ChartsError {
    /// A name QML sent isn't one of the fixed names. QML only sends fixed
    /// names, so this is a mistake in the screen, not something to tell the
    /// user.
    UnknownName(&'static str),
    /// The dataset couldn't be made.
    Report(ReportError),
    /// The chart couldn't be made.
    Chart(ChartError),
}

impl ChartsError {
    /// The error as a code for QML.
    pub fn report(&self) -> Option<ErrorReport> {
        match self {
            Self::UnknownName(_) => None,
            Self::Report(error) => Some(error.report()),
            Self::Chart(error) => Some(error.report()),
        }
    }
}

impl From<ReportError> for ChartsError {
    fn from(error: ReportError) -> Self {
        Self::Report(error)
    }
}

impl From<ChartError> for ChartsError {
    fn from(error: ChartError) -> Self {
        Self::Chart(error)
    }
}

/// Writes the amounts and months of a dataset in the user's regional format.
pub(crate) struct AppText<'a, L> {
    book: &'a Book,
    locale: &'a L,
}

impl<'a, L> AppText<'a, L> {
    pub(crate) const fn new(book: &'a Book, locale: &'a L) -> Self {
        Self { book, locale }
    }
}

impl<L: Locale> AppText<'_, L> {
    /// `money` with all its decimal places, also when the user chose to see
    /// fewer (D-002).
    pub(crate) fn exact(&self, money: Money) -> String {
        match AmountWriter::new(self.book, money.currency(), self.locale) {
            Ok(writer) => writer.text(money).exact,
            Err(_) => self.amount(money),
        }
    }
}

impl<L: Locale> TextFormat for AppText<'_, L> {
    fn amount(&self, money: Money) -> String {
        match AmountWriter::new(self.book, money.currency(), self.locale) {
            Ok(writer) => writer.text(money).text,
            // The dataset only lists currencies the book uses, so this is
            // for a book that changed under the read. Raw minor units would
            // read as a wrong figure, so only the currency is shown.
            Err(_) => format!("? {}", money.currency()),
        }
    }

    fn month(&self, first_day: Date) -> String {
        self.locale.month_and_year(first_day)
    }

    fn percent(&self, percent: Percent) -> String {
        let tenths = percent.tenths();
        let sign = if tenths < 0 { "-" } else { "" };
        let magnitude = tenths.unsigned_abs();
        format!(
            "{sign}{}{}{} %",
            magnitude / 10,
            self.locale.decimal_point(),
            magnitude % 10
        )
    }

    fn number(&self, hundredths: i64) -> String {
        let sign = if hundredths < 0 { "-" } else { "" };
        let magnitude = hundredths.unsigned_abs();
        let whole = self.locale.group_digits(magnitude / 100);
        let places = match magnitude % 100 {
            0 => String::new(),
            places if places % 10 == 0 => format!("{}{}", self.locale.decimal_point(), places / 10),
            places => format!("{}{places:02}", self.locale.decimal_point()),
        };
        format!("{sign}{whole}{places}")
    }

    /// Empty: the screen writes the translated name at the chart's
    /// `unsorted` place (D-034).
    fn unsorted(&self) -> String {
        String::new()
    }
}

/// The provider the app draws with (D-275). It takes every kind of chart
/// the model has; the drawing itself is QML (M6a.4).
pub struct QuickCharts;

impl ChartProvider for QuickCharts {
    fn name(&self) -> &'static str {
        "kquickcharts"
    }

    fn supports(&self, _kind: ChartKind) -> bool {
        true
    }
}

/// The calendar month named `YYYY-MM`, which the overview passes on for the
/// month it shows (M6a.8).
fn month_period(key: &str) -> Option<Period> {
    let (year, month) = key.split_once('-')?;
    let digits = |text: &str| text.bytes().all(|byte| byte.is_ascii_digit());
    if year.len() != 4 || month.len() != 2 || !digits(year) || !digits(month) {
        return None;
    }
    let first = Date::new(year.parse().ok()?, month.parse().ok()?, 1).ok()?;
    Period::new(first, first.last_of_month()).ok()
}

/// The period a request names (D-332): a fixed name, `today`, a month as
/// `YYYY-MM` or a typed range of days, whose open start is the first day of
/// the book.
pub(crate) fn period(book: &Book, key: &str, today: Date) -> Result<Period, ChartsError> {
    if key.contains("..") {
        let earliest = book
            .booking_date_range()
            .ok()
            .flatten()
            .map(|(first, _)| first);
        return Period::from_range_word(key, today, earliest)
            .ok_or(ChartsError::UnknownName("period"))?
            .map_err(ChartsError::from);
    }
    Ok(match key {
        // The day itself: what the accounts hold today (D-370).
        "today" => Period::new(today, today)?,
        "this-month" => Period::this_month(today)?,
        "last-month" => Period::last_month(today)?,
        "last-12-months" => Period::last_12_months(today)?,
        "this-year" => Period::this_year(today)?,
        "last-year" => Period::last_year(today)?,
        _ => month_period(key).ok_or(ChartsError::UnknownName("period"))?,
    })
}

/// Whether the dataset is made from the balances of the accounts, which the
/// `Source` reads only when asked to (D-365).
pub(crate) fn is_financial(dataset: &str) -> bool {
    matches!(
        dataset,
        ACCOUNT_BALANCES | NET_WORTH_OVER_TIME | CASH_FLOW_BY_MONTH
    )
}

/// One chart per currency the dataset has money in, in the order of the
/// currency codes. An empty list means there was nothing to show.
pub fn charts(
    book: &Book,
    locale: &impl Locale,
    request: &Request,
) -> Result<ChartSet, ChartsError> {
    let kind = ChartKind::from_key(&request.kind).ok_or(ChartsError::UnknownName("kind"))?;
    let period = period(book, &request.period, locale.today())?;
    let level = match request.grouping.as_str() {
        "top-level" => Level::TopLevel,
        "each" | "groups" | "account-kind" => Level::Each,
        _ => return Err(ChartsError::UnknownName("grouping")),
    };
    // A formula of a column may name periods of its own, so the book is read
    // for all of them (D-341).
    let span = columns_span(&formula_columns(&request.columns), period)?;
    // The balances of the accounts take longer to read, so only the datasets
    // made from them ask for them (D-365). Columns decide what a chart shows,
    // whatever its dataset says, and they are made of spending.
    let source = if request.columns.is_empty() && is_financial(&request.dataset) {
        session::cached_source(book, span, true, || Source::read_with_balances(book, span))?
    } else {
        session::cached_source(book, span, false, || Source::read(book, span))?
    };
    let text = AppText::new(book, locale);
    if !request.columns.is_empty() {
        let fallback = fallback_currency(book);
        return columns_charts(&source, &text, kind, period, &request.columns, fallback);
    }
    // Parts of a whole count a category once; separate series may share it.
    let overlap = if kind.is_round() || kind == ChartKind::StackedBar {
        Overlap::Refused
    } else {
        Overlap::Allowed
    };
    // Groups matter only for the grouping that shows them as rows.
    let chosen_groups = if request.grouping == "groups" {
        request.groups.as_slice()
    } else {
        &[]
    };
    let groups: Vec<GroupMember> = chosen_groups
        .iter()
        .filter_map(|id| source.member_by_id(id))
        .filter(|member| matches!(member, GroupMember::Group(_)))
        .collect();
    let series: Vec<GroupMember> = request
        .series
        .iter()
        .filter_map(|id| source.member_by_id(id))
        .collect();
    let only: Vec<GroupMember> = request
        .categories
        .iter()
        .filter_map(|id| source.member_by_id(id))
        .collect();
    let (named, known) = match request.dataset.as_str() {
        SPENDING_BY_CATEGORY => (
            chosen_groups.len() + request.categories.len(),
            groups.len() + only.len(),
        ),
        CATEGORY_OVER_TIME => (request.series.len(), series.len()),
        _ => (0, 0),
    };
    let shown = Shown {
        groups: groups
            .iter()
            .filter_map(|member| match member {
                GroupMember::Group(id) => Some(*id),
                GroupMember::Category(_) => None,
            })
            .collect(),
        only,
        show_hidden: request.show_hidden,
        overlap,
    };
    let models = match request.dataset.as_str() {
        SPENDING_BY_CATEGORY => {
            spending_dataset(&source, period, &Filter::default(), level, &shown)?
                .iter()
                .map(|data| spending_by_category(data, kind, &text))
                .collect::<Result<Vec<_>, _>>()?
        }
        CATEGORY_OVER_TIME => {
            over_time_dataset(&source, period, &Filter::default(), &series, overlap)?
                .iter()
                .map(|data| category_over_time(data, kind, &text))
                .collect::<Result<Vec<_>, _>>()?
        }
        INCOME_EXPENSE_BY_MONTH => monthly_dataset(&source, period, &Filter::default())?
            .iter()
            .map(|data| income_expense_by_month(data, kind, &text))
            .collect::<Result<Vec<_>, _>>()?,
        ACCOUNT_BALANCES => {
            let by = if request.grouping == "account-kind" {
                BalancesBy::Kind
            } else {
                BalancesBy::Account
            };
            balances_dataset(&source, period, &Filter::default(), None)?
                .results
                .iter()
                .map(|data| balances_chart(data, by, kind, &text))
                .collect::<Result<Vec<_>, _>>()?
        }
        NET_WORTH_OVER_TIME => net_worth_dataset(&source, period, &Filter::default(), None)?
            .results
            .iter()
            .map(|data| net_worth_chart(data, kind, &text))
            .collect::<Result<Vec<_>, _>>()?,
        CASH_FLOW_BY_MONTH => cash_flow_dataset(&source, period, &Filter::default())?
            .iter()
            .map(|data| cash_flow_chart(data, kind, &text))
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(ChartsError::UnknownName("dataset")),
    };
    let charts = models
        .into_iter()
        .map(|model| {
            let prepared = QuickCharts.prepare(&model)?;
            Ok((model, prepared))
        })
        .collect::<Result<Vec<_>, ChartError>>()?;
    Ok(ChartSet {
        charts,
        missing: named - known,
        shared: Vec::new(),
        totals: Vec::new(),
    })
}

/// The charts of a request that has columns (D-339): the lines, areas and
/// stacked bars draw one series per column month by month, everything else
/// shows one row per column for the whole period. A category in two columns
/// counts in both; the result names it so the screen can say so.
/// The columns of a request with only what the book's span needs: their
/// formulas.
fn formula_columns(chosen: &[(String, Vec<String>, String)]) -> Vec<Column> {
    chosen
        .iter()
        .map(|(name, _, formula)| Column {
            name: name.clone(),
            members: Vec::new(),
            formula: (!formula.is_empty()).then(|| formula.clone()),
        })
        .collect()
}

/// The currency of a result with no money at all: the first the book uses.
fn fallback_currency(book: &Book) -> CurrencyCode {
    book.currencies()
        .ok()
        .and_then(|used| used.first().map(|currency| currency.code()))
        .unwrap_or(CurrencyCode::new("EUR").expect("EUR is a valid code"))
}

/// What each column adds up to over `period`, in every currency it has money
/// in, written for the screen.
fn column_totals(
    source: &Source,
    text: &impl TextFormat,
    period: Period,
    columns: &[Column],
    fallback: CurrencyCode,
) -> Result<Vec<(String, Vec<String>)>, ChartsError> {
    let results = columns_spending(source, period, &Filter::default(), columns, fallback)?;
    Ok(columns
        .iter()
        .map(|column| {
            let amounts = results
                .iter()
                .filter_map(|result| {
                    // The rows keep the columns' names, and a name is unique.
                    let row = result
                        .rows
                        .iter()
                        .find(|row| row.path.first() == Some(&column.name))?;
                    Some(text.amount(row.total))
                })
                .collect();
            (column.name.clone(), amounts)
        })
        .collect())
}

fn columns_charts(
    source: &Source,
    text: &impl TextFormat,
    kind: ChartKind,
    period: Period,
    chosen: &[(String, Vec<String>, String)],
    fallback: CurrencyCode,
) -> Result<ChartSet, ChartsError> {
    let columns: Vec<Column> = chosen
        .iter()
        .map(|(name, members, formula)| Column {
            name: name.clone(),
            members: members
                .iter()
                .filter_map(|id| source.member_by_id(id))
                .collect(),
            formula: (!formula.is_empty()).then(|| formula.clone()),
        })
        .collect();
    let named: usize = chosen.iter().map(|(_, members, _)| members.len()).sum();
    let known: usize = columns.iter().map(|column| column.members.len()).sum();
    let filter = Filter::default();
    let models = if matches!(
        kind,
        ChartKind::Line | ChartKind::Area | ChartKind::StackedBar
    ) {
        columns_over_time(source, period, &filter, &columns, fallback)?
            .iter()
            .map(|data| category_over_time(data, kind, text))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        columns_spending(source, period, &filter, &columns, fallback)?
            .iter()
            .map(|data| spending_by_category(data, kind, text))
            .collect::<Result<Vec<_>, _>>()?
    };
    let charts = models
        .into_iter()
        .map(|model| {
            let prepared = QuickCharts.prepare(&model)?;
            Ok((model, prepared))
        })
        .collect::<Result<Vec<_>, ChartError>>()?;
    Ok(ChartSet {
        charts,
        missing: named - known,
        shared: shared_categories(source, &columns),
        totals: column_totals(source, text, period, &columns, fallback)?,
    })
}
