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
    BalancesBy, ChartError, ChartKind, ChartModel, ChartProvider, CustomError, Prepared,
    account_balances as balances_chart, balance_candles as candles_chart,
    cash_flow_by_month as cash_flow_chart, category_over_time, income_expense_by_month,
    net_worth_over_time as net_worth_chart, spending_by_category, spending_by_category_as,
    spending_by_category_compared,
};
use kassensturz_core::{CurrencyCode, Date, ErrorReport, GroupMember, Money, Report};
use kassensturz_report::{
    ACCOUNT_BALANCES, Average, CASH_FLOW_BY_MONTH, CATEGORY_OVER_TIME, CandleLength, Column,
    CompareWith, Filter, INCOME_EXPENSE_BY_MONTH, Level, NET_WORTH_OVER_TIME, Overlap, Per,
    Percent, Period, ReportError, SPENDING_BY_CATEGORY, SharedCategory, Shown, Source, TextFormat,
    account_balances as balances_dataset, balance_candles as candles_dataset,
    cash_flow_by_month as cash_flow_dataset, category_over_time as over_time_dataset,
    columns_over_time, columns_span, columns_spending, compared_early, counted_early,
    income_expense_by_month as monthly_dataset, income_expense_by_series,
    net_worth_over_time as net_worth_dataset, shared_categories,
    spending_by_category_shown as spending_dataset, spending_compared,
};
use kassensturz_store::Book;

use crate::format::Locale;
use crate::register::AmountWriter;
use crate::scope::{FilterView, NoteView, Scope};
use crate::session;

/// What the user asked for, by the fixed names QML uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    /// `spending_by_category`, `income_expense_by_month`,
    /// `category_over_time`, `account_balances`, `net_worth_over_time` or
    /// `cash_flow_by_month`.
    pub dataset: String,
    /// `line`, `bar`, `stacked-bar`, `area`, `pie`, `donut` or
    /// `candlestick` (US4.10, only of `cash_flow_by_month`).
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
    /// The filters the dashboard puts on the component: its own, else the
    /// filter bar's (M6c.7, D-350). The default narrows nothing.
    pub scope: Scope,
    /// A custom chart (M6e.3): the description the chart is drawn from. Then
    /// `dataset` is the one it names, `kind` and `grouping` are not used,
    /// and `series` and `categories` are the choices of the dataset it
    /// names (Q-353).
    pub custom: Option<Custom>,
}

/// The description of a custom chart and whether to draw it without the
/// parts that are not supported here (D-508).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Custom {
    /// `props.spec`, exactly as the user wrote it.
    pub spec: String,
    /// `props.partial`.
    pub partial: bool,
    /// The screen's words for the answers "yes" and "no" in the user's
    /// language (D-525); empty for the English words.
    pub yes: String,
    /// See `yes`.
    pub no: String,
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
    /// The marks and notes about the filters (D-350).
    pub filter: FilterView,
    /// The `title` of a custom chart's description, the user's own text;
    /// empty for a built-in chart and a description without one.
    pub title: String,
    /// The `description` of a custom chart's description, for a screen
    /// reader; empty otherwise.
    pub description: String,
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
    /// A custom chart couldn't be drawn from its description (M6e.3).
    Custom(CustomError),
}

impl ChartsError {
    /// The error as a code for QML.
    pub fn report(&self) -> Option<ErrorReport> {
        match self {
            Self::UnknownName(_) => None,
            Self::Report(error) => Some(error.report()),
            Self::Chart(error) => Some(error.report()),
            Self::Custom(error) => Some(error.report()),
        }
    }
}

impl From<ReportError> for ChartsError {
    fn from(error: ReportError) -> Self {
        Self::Report(error)
    }
}

impl From<CustomError> for ChartsError {
    fn from(error: CustomError) -> Self {
        Self::Custom(error)
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

    fn day(&self, day: Date) -> String {
        self.locale.short_date(day)
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

/// The period a request names (D-332): `today`, a month as `YYYY-MM`, or a
/// stored period word ([`Period::named`]): a fixed name, a rolling period
/// (D-644) or a typed range of days, whose open start is the first day of
/// the book. Charts, tables, number cards, formulas and budgets all read
/// their period here, so one word counts the same days everywhere.
pub(crate) fn period(book: &Book, key: &str, today: Date) -> Result<Period, ChartsError> {
    // The day itself: what the accounts hold today (D-370).
    if key == "today" {
        return Ok(Period::new(today, today)?);
    }
    if let Some(month) = month_period(key) {
        return Ok(month);
    }
    let earliest = || {
        book.booking_date_range()
            .ok()
            .flatten()
            .map(|(first, _)| first)
    };
    let earliest = if key.contains("..") { earliest() } else { None };
    Period::named(key, today, earliest)
        .ok_or(ChartsError::UnknownName("period"))?
        .map_err(ChartsError::from)
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
    if let Some(custom) = &request.custom {
        return crate::custom_charts::custom_charts(book, locale, request, custom);
    }
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
    // Spending bars and tables may compare each category with earlier
    // periods (US3.1a, D-687), so the book is read for those too.
    let compare = CompareWith::read(&request.scope.compare).filter(|_| {
        request.dataset == SPENDING_BY_CATEGORY
            && kind == ChartKind::Bar
            && request.columns.is_empty()
    });
    let span = match compare {
        Some(compare) => compare.read_span(period)?,
        None => span,
    };
    // The balances of the accounts take longer to read, so only the datasets
    // made from them ask for them (D-365). Columns decide what a chart shows,
    // whatever its dataset says, and they are made of spending.
    let source = if request.columns.is_empty() && is_financial(&request.dataset) {
        session::cached_source(book, span, true, || Source::read_with_balances(book, span))?
    } else {
        session::cached_source(book, span, false, || Source::read(book, span))?
    };
    let text = AppText::new(book, locale);
    let (filter, mut filter_view) = request.scope.resolve(book, &source)?;
    // Money in and money out with filters of their own (D-646): each series
    // is counted with its own lists, and the chart says what both did.
    let series_filters = if request.dataset == INCOME_EXPENSE_BY_MONTH
        && request.scope.has_series()
        && request.columns.is_empty()
    {
        let (income, income_view) = request.scope.for_series("income").resolve(book, &source)?;
        let (expenses, expenses_view) = request
            .scope
            .for_series("expenses")
            .resolve(book, &source)?;
        filter_view = income_view.merge(expenses_view);
        Some((income, expenses))
    } else {
        None
    };
    // A filter that chooses nothing says so instead of drawing zeros.
    if filter_view.nothing {
        return Ok(ChartSet {
            charts: Vec::new(),
            missing: 0,
            shared: Vec::new(),
            totals: Vec::new(),
            filter: filter_view,
            title: String::new(),
            description: String::new(),
        });
    }
    if !request.columns.is_empty() {
        let fallback = fallback_currency(book);
        let mut set = columns_charts(
            &source,
            &text,
            kind,
            period,
            &request.columns,
            &filter,
            fallback,
        )?;
        set.filter = filter_view;
        return Ok(set);
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
    // Spending as an average per month, quarter or year (US1.13, D-648):
    // counted up to today, divided in the book's rounding mode.
    let average = match Per::read(&request.scope.average) {
        Some(per) if request.dataset == SPENDING_BY_CATEGORY => Some(Average::new(
            per,
            period,
            locale.today(),
            book.rounding_mode().map_err(ReportError::Store)?,
        )?),
        _ => None,
    };
    // An average that counts months before the first transaction says so
    // under the chart (M6-B207, #590, D-668); the figure stays as it is.
    if let Some(average) = &average
        && let Some(early) =
            counted_early(book, &source, period, &filter, average.per, locale.today())?
    {
        filter_view.notes.push(NoteView {
            kind: "average-early",
            dimension: "period",
            id: early.per.word().into(),
            counted: early.counted,
            before: early.before,
        });
    }
    // A comparison that counts periods before the first transaction says so
    // too (US3.1a, as D-668); its figures stay as they are.
    if let Some(compare) = compare
        && let Some(early) = compared_early(book, &source, period, &filter, compare)?
    {
        filter_view.notes.push(NoteView {
            kind: "compare-early",
            dimension: "period",
            id: compare.word().into(),
            counted: early.counted,
            before: early.before,
        });
    }
    let models = if let Some(compare) = compare {
        let earlier = compare.divisor(
            period,
            average.map(|average| average.per),
            locale.today(),
            book.rounding_mode().map_err(ReportError::Store)?,
        )?;
        spending_compared(&source, period, &filter, level, &shown, compare)?
            .iter()
            .map(|data| {
                spending_by_category_compared(
                    data,
                    compare,
                    kind,
                    &text,
                    average.as_ref(),
                    &earlier,
                )
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        match request.dataset.as_str() {
            SPENDING_BY_CATEGORY => spending_dataset(&source, period, &filter, level, &shown)?
                .iter()
                .map(|data| spending_by_category_as(data, kind, &text, average.as_ref()))
                .collect::<Result<Vec<_>, _>>()?,
            CATEGORY_OVER_TIME => over_time_dataset(&source, period, &filter, &series, overlap)?
                .iter()
                .map(|data| category_over_time(data, kind, &text))
                .collect::<Result<Vec<_>, _>>()?,
            INCOME_EXPENSE_BY_MONTH => match &series_filters {
                Some((income, expenses)) => {
                    income_expense_by_series(&source, period, income, expenses)?
                }
                None => monthly_dataset(&source, period, &filter)?,
            }
            .iter()
            .map(|data| income_expense_by_month(data, kind, &text))
            .collect::<Result<Vec<_>, _>>()?,
            ACCOUNT_BALANCES => {
                let by = if request.grouping == "account-kind" {
                    BalancesBy::Kind
                } else {
                    BalancesBy::Account
                };
                balances_dataset(&source, period, &filter, None)?
                    .results
                    .iter()
                    .map(|data| balances_chart(data, by, kind, &text))
                    .collect::<Result<Vec<_>, _>>()?
            }
            NET_WORTH_OVER_TIME => net_worth_dataset(&source, period, &filter, None)?
                .results
                .iter()
                .map(|data| net_worth_chart(data, kind, &text))
                .collect::<Result<Vec<_>, _>>()?,
            // The chosen accounts' balance as candles (US4.10, D-718,
            // D-719): the same accounts as the cash flow.
            CASH_FLOW_BY_MONTH if kind == ChartKind::Candlestick => {
                let length = CandleLength::read(&request.scope.candle);
                candles_dataset(&source, period, &filter, length)?
                    .iter()
                    .map(|data| candles_chart(data, &text))
                    .collect::<Result<Vec<_>, _>>()?
            }
            CASH_FLOW_BY_MONTH => cash_flow_dataset(&source, period, &filter)?
                .iter()
                .map(|data| cash_flow_chart(data, kind, &text))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(ChartsError::UnknownName("dataset")),
        }
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
        filter: filter_view,
        title: String::new(),
        description: String::new(),
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
    filter: &Filter,
    fallback: CurrencyCode,
) -> Result<Vec<(String, Vec<String>)>, ChartsError> {
    let results = columns_spending(source, period, filter, columns, fallback)?;
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
    filter: &Filter,
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
    let models = if matches!(
        kind,
        ChartKind::Line | ChartKind::Area | ChartKind::StackedBar
    ) {
        columns_over_time(source, period, filter, &columns, fallback)?
            .iter()
            .map(|data| category_over_time(data, kind, text))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        columns_spending(source, period, filter, &columns, fallback)?
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
        totals: column_totals(source, text, period, &columns, filter, fallback)?,
        filter: FilterView::default(),
        title: String::new(),
        description: String::new(),
    })
}
