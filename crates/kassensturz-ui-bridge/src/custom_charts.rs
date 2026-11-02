// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A custom chart of the open book (M6e.3, D-504 to D-508): the description
//! the user pasted or opened is read by the parser of `kassensturz-report`,
//! the dataset it names is made with the page's filters, flattened into the
//! table a description sees, and drawn by the renderer of
//! `kassensturz-charts`. No Qt in this file: `charts_model.rs` hands the
//! result to QML.
//!
//! The description never reaches the book: the renderer reads the flat table
//! and nothing else, and the table holds only what the dataset's own result
//! holds.

use kassensturz_charts::{ChartProvider, custom_chart};
use kassensturz_core::{AccountId, GroupMember};
use kassensturz_report::spec::{self, FlatTable};
use kassensturz_report::{
    ACCOUNT_BALANCES, BUDGET_UTILIZATION, BudgetGrouping, CASH_FLOW_BY_MONTH, CATEGORY_OVER_TIME,
    INCOME_EXPENSE_BY_MONTH, Level, NET_WORTH_OVER_TIME, Overlap, ReportError,
    SPENDING_BY_CATEGORY, Shown, Source, account_balances, budget_utilization, cash_flow_by_month,
    category_over_time, columns_span, income_expense_by_month, net_worth_over_time,
    spending_by_category_shown,
};
use kassensturz_store::Book;

use crate::charts::{
    AppText, ChartSet, ChartsError, Custom, QuickCharts, Request, is_financial, period,
};
use crate::format::Locale;
use crate::scope::FilterView;
use crate::session;

/// What a description says, read once: the chart, or why there is none.
struct Described {
    chart: spec::ChartSpec,
}

/// Reads the description. A description with parts that are not supported
/// here is drawn only when its owner chose to leave them out (`partial`,
/// D-508); otherwise the place shows `report.spec-unsupported`.
fn describe(custom: &Custom) -> Result<Described, ReportError> {
    let read = spec::read(&custom.spec)?;
    let count = read.unsupported().len();
    if count > 0 && !custom.partial {
        return Err(ReportError::SpecUnsupported { count });
    }
    let why_not = read.why_not();
    match read.into_parts().0 {
        Some(chart) => Ok(Described { chart }),
        None => Err(why_not.unwrap_or(ReportError::SpecInvalid {
            reason: "nothing-to-draw",
            path: String::new(),
        })),
    }
}

/// The screen's word, or the English one while the screen has given none.
fn word<'a>(given: &'a str, english: &'a str) -> &'a str {
    if given.is_empty() { english } else { given }
}

/// The tables of the dataset the description names, one per currency, in
/// the order of the currency codes, made with the filters of the request.
fn tables(
    book: &Book,
    locale: &impl Locale,
    request: &Request,
    dataset: &str,
) -> Result<(Vec<FlatTable>, FilterView, usize), ChartsError> {
    let today = locale.today();
    let period = period(book, &request.period, today)?;
    let span = columns_span(&[], period)?;
    let source = if dataset == BUDGET_UTILIZATION {
        std::rc::Rc::new(Source::read_with_budgets(book, period, today)?)
    } else if is_financial(dataset) {
        session::cached_source(book, span, true, || Source::read_with_balances(book, span))?
    } else {
        session::cached_source(book, span, false, || Source::read(book, span))?
    };
    let (mut filter, view) = request.scope.resolve(book, &source)?;
    if view.nothing {
        return Ok((Vec::new(), view, 0));
    }
    let members = |ids: &[String]| -> Vec<GroupMember> {
        ids.iter()
            .filter_map(|id| source.member_by_id(id))
            .collect()
    };
    let mut missing = 0;
    let tables = match dataset {
        SPENDING_BY_CATEGORY => {
            let shown = Shown {
                overlap: Overlap::Allowed,
                ..Shown::default()
            };
            spending_by_category_shown(&source, period, &filter, Level::Each, &shown)?
                .iter()
                .map(FlatTable::from_spending)
                .collect()
        }
        CATEGORY_OVER_TIME => {
            let series = members(&request.series);
            missing = request.series.len() - series.len();
            category_over_time(&source, period, &filter, &series, Overlap::Allowed)?
                .iter()
                .map(FlatTable::from_over_time)
                .collect()
        }
        INCOME_EXPENSE_BY_MONTH => income_expense_by_month(&source, period, &filter)?
            .iter()
            .map(FlatTable::from_monthly)
            .collect(),
        ACCOUNT_BALANCES => account_balances(&source, period, &filter, None)?
            .results
            .iter()
            .map(FlatTable::from_balances)
            .collect(),
        NET_WORTH_OVER_TIME => net_worth_over_time(&source, period, &filter, None)?
            .results
            .iter()
            .map(FlatTable::from_worth)
            .collect(),
        CASH_FLOW_BY_MONTH => cash_flow_by_month(&source, period, &filter)?
            .iter()
            .map(FlatTable::from_cash_flow)
            .collect(),
        BUDGET_UTILIZATION => {
            // As the budget bars: the categories ticked in the panel limit
            // the budgets unless the page's own filter already does.
            if filter.categories.is_none() && !request.categories.is_empty() {
                let ids: Vec<AccountId> = book
                    .categories()
                    .map_err(ReportError::from)?
                    .into_iter()
                    .map(|entry| entry.category.id)
                    .filter(|id| request.categories.contains(&id.to_string()))
                    .collect();
                filter.categories = Some(ids);
            }
            let budgets =
                budget_utilization(&source, period, today, BudgetGrouping::Budget, &filter)?;
            spec::budget_tables(&budgets)
        }
        _ => return Err(ChartsError::UnknownName("dataset")),
    };
    Ok((tables, view, missing))
}

/// The charts of a custom chart request: one per currency the dataset has
/// money in. An empty list means there was nothing to show.
pub fn custom_charts(
    book: &Book,
    locale: &impl Locale,
    request: &Request,
    custom: &Custom,
) -> Result<ChartSet, ChartsError> {
    let described = describe(custom)?;
    let spec = &described.chart;
    let (tables, filter, missing) = tables(book, locale, request, spec.dataset().id())?;
    let text = AppText::new(book, locale);
    let (yes, no) = (word(&custom.yes, "yes"), word(&custom.no, "no"));
    let mut charts = Vec::new();
    let (mut title, mut description) = (String::new(), String::new());
    for table in &tables {
        let Some(drawn) = custom_chart(spec, table, &text)? else {
            continue;
        };
        if title.is_empty() {
            title = drawn.title.clone().unwrap_or_default();
            description = drawn.description.clone().unwrap_or_default();
        }
        // The marks of the yes/no answers become the screen's words (D-525).
        let mut chart = drawn.chart;
        chart.write_answers(yes, no);
        let prepared = QuickCharts.prepare(&chart)?;
        charts.push((chart, prepared));
    }
    Ok(ChartSet {
        charts,
        missing,
        shared: Vec::new(),
        totals: Vec::new(),
        filter,
        title,
        description,
    })
}
