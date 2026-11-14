// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The dashboards an example book comes with (US7, #829, D-788): its own
//! Overview, then *Spending*, *Money in and out* and *Worth and plans*. Between
//! them they use every kind of tile the app has today, so a person trying an
//! example sees what KassenSturz can show before entering their own money.
//! Each example book is in one language, and so are its dashboards' names,
//! headings and titles; the other tiles show their own translated words.
//!
//! The documents name the book's accounts and categories by their name, as
//! `@account:Name@` and `@category:Name@`; [`resolve_names`] puts in their
//! IDs once the book's accounts have them. Two kinds of tile are left out:
//! *Update all accounts* is offered only where bank connections can be used
//! (D-054), and a custom chart is a description the user writes (D-504).
//!
//! All names are made up (AGENTS.md §3.1 rule 2, #734).

use kassensturz_core::Account;

/// What differs between the example books: the words, the account the
/// candles and the warning watch, the three categories the stacked bars
/// show, and the figures of the warning and the goal.
struct Words {
    overview: &'static str,
    welcome: &'static str,
    spending: &'static str,
    spending_heading: &'static str,
    compared_title: &'static str,
    by_month_title: &'static str,
    food_title: &'static str,
    average_title: &'static str,
    this_year_title: &'static str,
    money: &'static str,
    money_heading: &'static str,
    candles_title: &'static str,
    last_year: &'static str,
    plans: &'static str,
    plans_heading: &'static str,
    goal_title: &'static str,
    warnings_title: &'static str,
    /// The account the candles and the warning watch.
    account: &'static str,
    /// The three categories of the stacked bars; the first is the food one.
    categories: [&'static str; 3],
    /// The warning's limit, as a plain decimal in the book's currency.
    below: &'static str,
    /// The monthly goal for the food category, a whole amount.
    food_goal: &'static str,
}

const GERMAN: Words = Words {
    overview: "Übersicht",
    welcome: "",
    spending: "Ausgaben",
    spending_heading: "Wofür das Geld ausgegeben wurde",
    compared_title: "Im Vergleich zum Vorjahr",
    by_month_title: "Die drei großen Bereiche, Monat für Monat",
    food_title: "Essen, Monat für Monat",
    average_title: "Im Schnitt pro Monat in diesem Jahr",
    this_year_title: "Ausgaben in diesem Jahr",
    money: "Geld rein und raus",
    money_heading: "Woher das Geld kam und wohin es ging",
    candles_title: "Kontostand, Monat für Monat",
    last_year: "Das letzte Jahr",
    plans: "Vermögen und Planung",
    plans_heading: "Was da ist, was kommt und worauf zu achten ist",
    goal_title: "Monatsziel fürs Essen",
    warnings_title: "Warnungen",
    account: "",
    categories: ["", "", ""],
    below: "",
    food_goal: "",
};

/// The words of the example book `slug`, as `ExampleBookChooser.qml` and
/// `example_books.rs` in the bridge know the slugs; `None` for another.
fn words(slug: &str) -> Option<Words> {
    Some(match slug {
        "suburban-family" => Words {
            welcome: "Willkommen bei Familie Sommerfeld! So steht es um das Haushaltsgeld.",
            account: "Girokonto",
            categories: ["Essen", "Wohnen", "Freizeit"],
            below: "2000.00",
            food_goal: "1500",
            ..GERMAN
        },
        "flower-utopia" => Words {
            welcome: "Willkommen in der Blumenwiese! So steht es um die Gemeinschaftskasse.",
            food_title: "Essen und Trinken, Monat für Monat",
            goal_title: "Monatsziel für Essen und Trinken",
            account: "Vereinskonto",
            categories: ["Essen und Trinken", "Saatgut und Pflanzen", "Wasser"],
            // Above what the club account holds: this warning applies, so
            // the example shows one that does (D-721).
            below: "3000.00",
            food_goal: "50",
            ..GERMAN
        },
        "udssr-agent" => Words {
            welcome: "Willkommen in der Hauptverwaltung für Windstille! So steht es um die Kassen.",
            food_title: "Verpflegung, Monat für Monat",
            goal_title: "Monatsziel für die Verpflegung",
            account: "Gehaltskonto",
            categories: ["Verpflegung", "Wohnen", "Dienstreisen"],
            below: "1000.00",
            food_goal: "200",
            ..GERMAN
        },
        "space-colony" => Words {
            overview: "Overview",
            welcome: "Welcome to the colony! Here is how the household money stands.",
            spending: "Spending",
            spending_heading: "What the money was spent on",
            compared_title: "Compared with a year earlier",
            by_month_title: "The three big areas, month by month",
            food_title: "Food, month by month",
            average_title: "On average per month this year",
            this_year_title: "Spent this year",
            money: "Money in and out",
            money_heading: "Where the money came from and where it went",
            candles_title: "Balance, month by month",
            last_year: "Last year",
            plans: "Worth and plans",
            plans_heading: "What there is, what is coming and what to watch",
            goal_title: "Monthly goal for food",
            warnings_title: "Warnings",
            account: "Everyday Checking",
            categories: ["Food", "Habitat", "Leisure"],
            below: "1000.00",
            food_goal: "700",
        },
        _ => return None,
    })
}

/// The tabs of the dashboards, the period header and the filter bar, which
/// every dashboard starts with (D-452, D-474, D-529), and a heading.
fn page_top(heading: &str) -> String {
    format!(
        r#"{{"id": "dashboard-tabs", "type": "page.tabs",
     "frame": {{"col": 0, "row": 0, "cols": 12, "rows": 1}}}},
    {{"id": "period-header", "type": "page.period",
     "frame": {{"col": 0, "row": 1, "cols": 12, "rows": 1}}}},
    {{"id": "filter-bar", "type": "page.filters",
     "frame": {{"col": 0, "row": 2, "cols": 12, "rows": 1}}}},
    {{"id": "heading", "type": "text.heading", "props": {{"text": {heading}}},
     "frame": {{"col": 0, "row": 3, "cols": 12, "rows": 1}}}}"#,
        heading = quoted(heading)
    )
}

/// `text` as a JSON string.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn dashboard(name: &str, components: &str) -> String {
    format!(
        r#"{{
  "format": "kassensturz-dashboard", "version": 4, "kind": "dashboard",
  "name": {name},
  "components": [
    {components}
  ]
}}"#,
        name = quoted(name)
    )
}

/// The example's Overview: the built-in one (`DEFAULT_OVERVIEW`) with a
/// welcome under the filter bar, marked as the one the book opens on and as
/// made from the Overview template (D-448).
fn overview(words: &Words) -> String {
    let built_in: &str = kassensturz_report::DEFAULT_OVERVIEW;
    let mut document = kassensturz_report::Document::read(built_in)
        .expect("the built-in Overview is a valid dashboard");
    for entry in &mut document.entries {
        if let Some(frame) = &mut entry.frame
            && frame.row >= 3
        {
            frame.row += 1;
        }
    }
    let welcome = format!(
        r#"{{"id": "welcome", "type": "text.heading", "props": {{"text": {text}}},
     "frame": {{"col": 0, "row": 3, "cols": 12, "rows": 1}}}}"#,
        text = quoted(words.welcome)
    );
    let welcome =
        kassensturz_report::Document::read(&dashboard("welcome", &welcome)).expect("a heading");
    let at = document
        .entries
        .iter()
        .position(|entry| entry.id == "fetch-gaps")
        .unwrap_or(document.entries.len());
    document
        .entries
        .insert(at, welcome.entries.into_iter().next().expect("one entry"));
    words.overview.clone_into(&mut document.name);
    document.template = Some(kassensturz_report::templates::OVERVIEW.to_owned());
    document.default = true;
    document.to_json()
}

/// *Spending*: the share of each area, each category against a year
/// earlier, every area against the month before, the three big areas month
/// by month as stacked bars, two key figures, the food category as a line,
/// each area's average month this year and the newest transactions.
fn spending(words: &Words) -> String {
    let [food, second, third] = words.categories;
    let components = format!(
        r#"{top},
    {{"id": "share", "type": "chart.pie", "dataset": "spending_by_category",
     "params": {{"grouping": "top_level_category"}},
     "frame": {{"col": 0, "row": 4, "cols": 6, "rows": 5}}}},
    {{"id": "compared", "type": "table.rows", "dataset": "spending_by_category",
     "params": {{"grouping": "category", "compare": "same_last_year"}},
     "props": {{"title": {compared_title}, "neutral": "5"}},
     "frame": {{"col": 6, "row": 4, "cols": 6, "rows": 5}}}},
    {{"id": "against-last-month", "type": "chart.bar", "dataset": "spending_by_category",
     "params": {{"grouping": "top_level_category", "compare": "previous_period"}},
     "frame": {{"col": 0, "row": 9, "cols": 12, "rows": 5}}}},
    {{"id": "areas", "type": "chart.stacked_bar", "dataset": "category_over_time",
     "params": {{"series": ["@category:{food}@", "@category:{second}@", "@category:{third}@"]}},
     "props": {{"title": {by_month_title}, "period": "last-12-months"}},
     "frame": {{"col": 0, "row": 14, "cols": 12, "rows": 5}}}},
    {{"id": "this-month", "type": "kpi.card", "dataset": "spending_by_category",
     "params": {{"measure": "total", "compare": "same_last_year"}},
     "frame": {{"col": 0, "row": 19, "cols": 6, "rows": 2}}}},
    {{"id": "this-year", "type": "kpi.card", "dataset": "spending_by_category",
     "params": {{"measure": "total"}},
     "props": {{"title": {this_year_title}, "period": "this-year"}},
     "frame": {{"col": 6, "row": 19, "cols": 6, "rows": 2}}}},
    {{"id": "food", "type": "chart.line", "dataset": "category_over_time",
     "params": {{"series": ["@category:{food}@"]}},
     "props": {{"title": {food_title}, "period": "last-12-months"}},
     "frame": {{"col": 0, "row": 21, "cols": 6, "rows": 5}}}},
    {{"id": "average", "type": "table.rows", "dataset": "spending_by_category",
     "params": {{"grouping": "top_level_category", "average": "per-month"}},
     "props": {{"title": {average_title}, "period": "this-year"}},
     "frame": {{"col": 6, "row": 21, "cols": 6, "rows": 5}}}},
    {{"id": "newest", "type": "table.transactions",
     "frame": {{"col": 0, "row": 26, "cols": 12, "rows": 5}}}}"#,
        top = page_top(words.spending_heading),
        compared_title = quoted(words.compared_title),
        by_month_title = quoted(words.by_month_title),
        average_title = quoted(words.average_title),
        this_year_title = quoted(words.this_year_title),
        food_title = quoted(words.food_title),
    );
    dashboard(words.spending, &components)
}

/// *Money in and out*: the money flow, money in and out by month, the
/// account's balance as candles, what the household is worth as an area,
/// last year's key figures in a period box, where the money came from and
/// the months as a table.
fn money(words: &Words) -> String {
    let account = words.account;
    let components = format!(
        r#"{top},
    {{"id": "flow", "type": "chart.flow", "dataset": "money_flow",
     "frame": {{"col": 0, "row": 4, "cols": 12, "rows": 6}}}},
    {{"id": "in-and-out", "type": "chart.bar", "dataset": "income_expense_by_month",
     "props": {{"period": "last-12-months"}},
     "frame": {{"col": 0, "row": 10, "cols": 12, "rows": 4}}}},
    {{"id": "candles", "type": "chart.candlestick", "dataset": "cash_flow_by_month",
     "params": {{"accounts": ["@account:{account}@"]}},
     "props": {{"title": {candles_title}, "period": "last-12-months"}},
     "frame": {{"col": 0, "row": 14, "cols": 6, "rows": 4}}}},
    {{"id": "worth", "type": "chart.area", "dataset": "net_worth_over_time",
     "props": {{"period": "last-12-months"}},
     "frame": {{"col": 6, "row": 14, "cols": 6, "rows": 4}}}},
    {{"id": "last-year", "type": "group.period",
     "props": {{"title": {last_year}, "period": "last-year"}},
     "frame": {{"col": 0, "row": 18, "cols": 12, "rows": 4}}}},
    {{"id": "year-income", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {{"measure": "income", "compare": "previous_period"}},
     "frame": {{"col": 0, "row": 19, "cols": 3, "rows": 2}}}},
    {{"id": "year-expenses", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {{"measure": "expenses", "compare": "previous_period"}},
     "frame": {{"col": 3, "row": 19, "cols": 3, "rows": 2}}}},
    {{"id": "year-left", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {{"measure": "net", "compare": "previous_period"}},
     "frame": {{"col": 6, "row": 19, "cols": 3, "rows": 2}}}},
    {{"id": "year-kept", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {{"measure": "savings-rate", "compare": "previous_period"}},
     "frame": {{"col": 9, "row": 19, "cols": 3, "rows": 2}}}},
    {{"id": "sources", "type": "table.income_sources",
     "frame": {{"col": 0, "row": 22, "cols": 6, "rows": 4}}}},
    {{"id": "months", "type": "table.rows", "dataset": "cash_flow_by_month",
     "props": {{"period": "last-12-months"}},
     "frame": {{"col": 6, "row": 22, "cols": 6, "rows": 4}}}},
    {{"id": "transfers", "type": "notice.transfers",
     "frame": {{"col": 0, "row": 26, "cols": 12, "rows": 1}}}}"#,
        top = page_top(words.money_heading),
        candles_title = quoted(words.candles_title),
        last_year = quoted(words.last_year),
    );
    dashboard(words.money, &components)
}

/// *Worth and plans*: what the household has, owes and is worth, each
/// account as bars and as a table, the budget bars, *Left this month* and
/// *Coming up*, a warning about the account, a goal for the food category,
/// and the transactions without a category.
fn plans(words: &Words) -> String {
    let account = words.account;
    let food = words.categories[0];
    let components = format!(
        r#"{top},
    {{"id": "have", "type": "kpi.card", "dataset": "account_balances",
     "params": {{"measure": "assets"}},
     "frame": {{"col": 0, "row": 4, "cols": 4, "rows": 2}}}},
    {{"id": "owe", "type": "kpi.card", "dataset": "account_balances",
     "params": {{"measure": "liabilities"}},
     "frame": {{"col": 4, "row": 4, "cols": 4, "rows": 2}}}},
    {{"id": "worth", "type": "kpi.card", "dataset": "account_balances",
     "params": {{"measure": "net_worth"}},
     "frame": {{"col": 8, "row": 4, "cols": 4, "rows": 2}}}},
    {{"id": "accounts-bars", "type": "chart.bar", "dataset": "account_balances",
     "params": {{"grouping": "account"}},
     "frame": {{"col": 0, "row": 6, "cols": 6, "rows": 4}}}},
    {{"id": "accounts", "type": "table.rows", "dataset": "account_balances",
     "params": {{"grouping": "account"}},
     "frame": {{"col": 6, "row": 6, "cols": 6, "rows": 4}}}},
    {{"id": "budgets", "type": "budget.progress", "dataset": "budget_utilization",
     "frame": {{"col": 0, "row": 10, "cols": 12, "rows": 4}}}},
    {{"id": "left-this-month", "type": "forecast.month",
     "frame": {{"col": 0, "row": 14, "cols": 6, "rows": 3}}}},
    {{"id": "coming-up", "type": "plan.coming_up",
     "frame": {{"col": 6, "row": 14, "cols": 6, "rows": 3}}}},
    {{"id": "warnings", "type": "notice.alarms",
     "props": {{"title": {warnings_title}, "alarms": [
       {{"reason": "account_below", "account": "@account:{account}@", "amount": "{below}"}}]}},
     "frame": {{"col": 0, "row": 17, "cols": 6, "rows": 3}}}},
    {{"id": "goal", "type": "progress.bar", "dataset": "income_expense_by_month",
     "props": {{"title": {goal_title}, "formula": {formula}, "target": "{goal}"}},
     "frame": {{"col": 6, "row": 17, "cols": 6, "rows": 1}}}},
    {{"id": "unsorted", "type": "notice.unsorted",
     "frame": {{"col": 6, "row": 18, "cols": 6, "rows": 2}}}},
    {{"id": "line", "type": "text.separator",
     "frame": {{"col": 0, "row": 20, "cols": 12, "rows": 1}}}},
    {{"id": "fetch-gaps", "type": "notice.fetch_gap",
     "frame": {{"col": 0, "row": 21, "cols": 12, "rows": 1}}}}"#,
        top = page_top(words.plans_heading),
        warnings_title = quoted(words.warnings_title),
        goal_title = quoted(words.goal_title),
        formula = quoted(&format!("'{food}'")),
        below = words.below,
        goal = words.food_goal,
    );
    dashboard(words.plans, &components)
}

/// The dashboards of the example book `slug`, in the order the tabs show
/// them, with the book's accounts and categories still named
/// (`@account:Name@`, `@category:Name@`, see [`resolve_names`]). Empty for a
/// slug that is not an example book.
pub fn example_dashboards(slug: &str) -> Vec<String> {
    let Some(words) = words(slug) else {
        return Vec::new();
    };
    vec![
        overview(&words),
        spending(&words),
        money(&words),
        plans(&words),
    ]
}

/// `text` with each `@account:Name@` replaced by the ID of the account
/// called so and each `@category:Name@` by the ID of the top-level category
/// called so, both from `accounts`. `None` if one of them isn't there, so a
/// dashboard that would point at nothing is left out instead.
pub fn resolve_names(text: &str, accounts: &[Account]) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('@') {
        let (before, marked) = rest.split_at(start);
        out.push_str(before);
        let inner = &marked[1..];
        let Some(end) = inner.find('@') else {
            out.push_str(marked);
            rest = "";
            break;
        };
        let token = &inner[..end];
        let id = if let Some(name) = token.strip_prefix("account:") {
            accounts
                .iter()
                .find(|account| account.kind.is_account() && account.name == name)
        } else if let Some(name) = token.strip_prefix("category:") {
            accounts.iter().find(|account| {
                account.kind.is_category() && account.parent.is_none() && account.name == name
            })
        } else {
            out.push('@');
            rest = inner;
            continue;
        };
        out.push_str(&id?.id.to_string());
        rest = &inner[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}
