// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The dashboards an example book comes with (US7, #829, D-788): its own
//! Overview, laid out like the start screen of a household that has used the
//! app for a while (D-946), and *Sonstiges* (*More*) with the spending, the
//! money in and out, and what there is and is coming. Between them they use
//! every kind of tile the app has today, so a person trying an example sees
//! what KassenSturz can show before entering their own money.
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
    spending_heading: &'static str,
    compared_title: &'static str,
    by_month_title: &'static str,
    food_title: &'static str,
    average_title: &'static str,
    this_year_title: &'static str,
    money_heading: &'static str,
    candles_title: &'static str,
    last_year: &'static str,
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
    /// The second tab, with everything the Overview leaves out.
    others: &'static str,
    /// The accounts the Overview shows a tile for, at most six.
    tiles: &'static [&'static str],
    spent_title: &'static str,
    rest_title: &'static str,
    /// What is left of the salary this month: a formula (D-292).
    rest_formula: &'static str,
    against_average_title: &'static str,
}

const GERMAN: Words = Words {
    overview: "Übersicht",
    welcome: "",
    spending_heading: "Wofür das Geld ausgegeben wurde",
    compared_title: "Im Vergleich zum Vorjahr",
    by_month_title: "Die drei großen Bereiche, Monat für Monat",
    food_title: "Essen, Monat für Monat",
    average_title: "Im Schnitt pro Monat in diesem Jahr",
    this_year_title: "Ausgaben in diesem Jahr",
    money_heading: "Woher das Geld kam und wohin es ging",
    candles_title: "Kontostand, Monat für Monat",
    last_year: "Das letzte Jahr",
    plans_heading: "Was da ist, was kommt und worauf zu achten ist",
    goal_title: "Monatsziel fürs Essen",
    warnings_title: "Warnungen",
    account: "",
    categories: ["", "", ""],
    below: "",
    food_goal: "",
    others: "Sonstiges",
    tiles: &[],
    spent_title: "Ausgaben in diesem Monat",
    rest_title: "Rest vom Gehalt",
    rest_formula: "=Gehalt - Spending.ALL",
    against_average_title: "Dieser Monat gegen den Schnitt der drei Monate davor",
};

const ENGLISH: Words = Words {
    overview: "Overview",
    welcome: "",
    spending_heading: "What the money was spent on",
    compared_title: "Compared with a year earlier",
    by_month_title: "The three big areas, month by month",
    food_title: "Food, month by month",
    average_title: "On average per month this year",
    this_year_title: "Spent this year",
    money_heading: "Where the money came from and where it went",
    candles_title: "Balance, month by month",
    last_year: "Last year",
    plans_heading: "What there is, what is coming and what to watch",
    goal_title: "Monthly goal for food",
    warnings_title: "Warnings",
    account: "",
    categories: ["", "", ""],
    below: "",
    food_goal: "",
    others: "More",
    tiles: &[],
    spent_title: "Spent this month",
    rest_title: "Left of the salary",
    rest_formula: "=Salary - Spending.ALL",
    against_average_title: "This month against the average of the three months before",
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
            tiles: &[
                "Girokonto",
                "Girokonto Jonas",
                "Haushaltskonto",
                "Sparkonto",
                "Tagesgeld",
                "Kreditkarte",
            ],
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
            tiles: &["Vereinskonto", "Rücklage", "Gemeinschaftskasse"],
            rest_title: "Rest der Einnahmen",
            rest_formula: "=Income.ALL - Spending.ALL",
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
            tiles: &[
                "Gehaltskonto",
                "Dienstkasse Dollar",
                "Dienstkonto London",
                "Sparbuch",
                "Nummernkonto Zürich",
                "Bargeld Rubel",
            ],
            ..GERMAN
        },
        // The English Suburban Family (D-843): the German book's figures.
        "suburban-family-en" => Words {
            welcome: "Welcome to the Summerfield family! Here is how the household money stands.",
            account: "Checking account",
            categories: ["Food", "Home", "Free time"],
            below: "2000.00",
            food_goal: "1500",
            tiles: &[
                "Checking account",
                "Jonas's checking account",
                "Household account",
                "Savings account",
                "Instant access savings",
                "Credit card",
            ],
            ..ENGLISH
        },
        "space-colony" => Words {
            welcome: "Welcome to the colony! Here is how the household money stands.",
            account: "Everyday Checking",
            categories: ["Food", "Habitat", "Leisure"],
            below: "1000.00",
            food_goal: "700",
            tiles: &[
                "Everyday Checking",
                "Ceres Savings Cooperative",
                "Dome Petty Cash",
                "Earth Trip Wallet",
                "Shuttle Travel Fund",
                "Shipyard Loan",
            ],
            rest_formula: "='Colony Salary and Allowances' - Spending.ALL",
            ..ENGLISH
        },
        _ => return None,
    })
}

/// The tabs of the dashboards, the period header and the filter bar, which
/// every dashboard starts with (D-452, D-474, D-529).
fn page_top() -> &'static str {
    r#"{"id": "dashboard-tabs", "type": "page.tabs",
     "frame": {"col": 0, "row": 0, "cols": 12, "rows": 1}},
    {"id": "period-header", "type": "page.period",
     "frame": {"col": 0, "row": 1, "cols": 12, "rows": 1}},
    {"id": "filter-bar", "type": "page.filters",
     "frame": {"col": 0, "row": 2, "cols": 12, "rows": 1}}"#
}

/// A heading across the page in row 3, where each part of *Sonstiges*
/// starts before [`shifted`] moves it down.
fn heading(text: &str) -> String {
    format!(
        r#"{{"id": "heading", "type": "text.heading", "props": {{"text": {text}}},
     "frame": {{"col": 0, "row": 3, "cols": 12, "rows": 1}}}}"#,
        text = quoted(text)
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

/// The example's Overview, the one the book opens on (D-448), laid out like
/// a household that has used the app for a while: a welcome, a tile for
/// each account, the budget bars, this month's spending and what is left of
/// the salary (a formula), the main account's balance as candles, where the
/// money went as a donut, each area against the average of the three months
/// before, and the money flow from the income through the accounts to the
/// areas. It is marked as made from the Overview template.
fn overview(words: &Words) -> String {
    // Three tiles to a row, each three rows high so its figure and its
    // change have room.
    let count = u32::try_from(words.tiles.len()).expect("a few tiles");
    let tile_rows = count.div_ceil(3) * 3;
    let tiles: Vec<String> = words
        .tiles
        .iter()
        .zip(0..)
        .map(|(account, place)| {
            format!(
                r#"{{"id": "account-{place}", "type": "kpi.card", "dataset": "account_balances",
     "params": {{"measure": "net_worth", "accounts": ["@account:{account}@"]}},
     "props": {{"title": {title}}},
     "frame": {{"col": {col}, "row": {row}, "cols": 4, "rows": 3}}}}"#,
                title = quoted(account),
                col = place % 3 * 4,
                row = 4 + place / 3 * 3,
            )
        })
        .collect();
    let account = words.account;
    let below = format!(
        r#"{{"id": "budgets", "type": "budget.progress", "dataset": "budget_utilization",
     "frame": {{"col": 0, "row": 6, "cols": 12, "rows": 4}}}},
    {{"id": "spent-this-month", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {{"measure": "expenses", "compare": "previous_period"}},
     "props": {{"title": {spent_title}}},
     "frame": {{"col": 0, "row": 10, "cols": 6, "rows": 2}}}},
    {{"id": "rest-of-salary", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {{"measure": "formula", "compare": "previous_period"}},
     "props": {{"title": {rest_title}, "formula": {rest_formula}}},
     "frame": {{"col": 6, "row": 10, "cols": 6, "rows": 2}}}},
    {{"id": "candles", "type": "chart.candlestick", "dataset": "cash_flow_by_month",
     "params": {{"accounts": ["@account:{account}@"]}},
     "props": {{"title": {candles_title}, "period": "last-12-months"}},
     "frame": {{"col": 0, "row": 12, "cols": 6, "rows": 5}}}},
    {{"id": "spending", "type": "chart.donut", "dataset": "spending_by_category",
     "params": {{"grouping": "top_level_category"}},
     "frame": {{"col": 6, "row": 12, "cols": 6, "rows": 5}}}},
    {{"id": "against-average", "type": "table.rows", "dataset": "spending_by_category",
     "params": {{"grouping": "top_level_category", "compare": "average-3"}},
     "props": {{"title": {average_title}}},
     "frame": {{"col": 0, "row": 17, "cols": 12, "rows": 6}}}},
    {{"id": "flow", "type": "chart.flow", "dataset": "money_flow",
     "params": {{"show_accounts": true}},
     "frame": {{"col": 0, "row": 23, "cols": 12, "rows": 7}}}},
    {{"id": "transfers", "type": "notice.transfers",
     "frame": {{"col": 0, "row": 30, "cols": 12, "rows": 1}}}},
    {{"id": "unsorted", "type": "notice.unsorted",
     "frame": {{"col": 0, "row": 31, "cols": 12, "rows": 2}}}}"#,
        spent_title = quoted(words.spent_title),
        rest_title = quoted(words.rest_title),
        rest_formula = quoted(words.rest_formula),
        candles_title = quoted(words.candles_title),
        average_title = quoted(words.against_average_title),
    );
    // What is below the tiles was written for one row of tiles two rows
    // high, from row 6 on.
    let below = rewrite_numbers(&below, r#""row": "#, tile_rows.max(2) - 2);
    let components = format!(
        r#"{top},
    {{"id": "welcome", "type": "text.heading", "props": {{"text": {welcome}}},
     "frame": {{"col": 0, "row": 3, "cols": 12, "rows": 1}}}},
    {tiles},
    {below}"#,
        top = page_top(),
        welcome = quoted(words.welcome),
        tiles = tiles.join(",\n    "),
    );
    let document = dashboard(words.overview, &components);
    let marked = format!(
        r#""kind": "dashboard", "template": {template}, "default": true,"#,
        template = quoted(kassensturz_report::templates::OVERVIEW)
    );
    document.replacen(r#""kind": "dashboard","#, &marked, 1)
}

/// `components`, as written for a page of their own from row 3 on, moved
/// `rows` further down, with `prefix` in front of every ID so that the
/// parts of one page keep different IDs.
fn shifted(components: &str, rows: u32, prefix: &str) -> String {
    let moved = rewrite_numbers(components, r#""row": "#, rows);
    moved.replace(r#""id": ""#, &format!(r#""id": "{prefix}-"#))
}

/// `text` with the number after each `key` raised by `by`.
fn rewrite_numbers(text: &str, key: &str, by: u32) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(key) {
        let (before, after) = rest.split_at(at + key.len());
        out.push_str(before);
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        let number: u32 = after[..digits].parse().expect("a row number");
        out.push_str(&(number + by).to_string());
        rest = &after[digits..];
    }
    out.push_str(rest);
    out
}

/// The highest row a component of `components` reaches, plus one.
fn bottom(components: &str) -> u32 {
    let rows: Vec<u32> = numbers_after(components, r#""row": "#);
    let heights: Vec<u32> = numbers_after(components, r#""rows": "#);
    rows.iter()
        .zip(&heights)
        .map(|(row, rows)| row + rows)
        .max()
        .unwrap_or(0)
}

fn numbers_after(text: &str, key: &str) -> Vec<u32> {
    text.match_indices(key)
        .map(|(at, _)| {
            let after = &text[at + key.len()..];
            let digits = after.chars().take_while(char::is_ascii_digit).count();
            after[..digits].parse().expect("a number")
        })
        .collect()
}

/// *Sonstiges* (*More* in English): everything the Overview leaves out, one
/// part under the other, each with its heading: what the money was spent
/// on, where it came from and went, and what there is and is coming.
/// Between them they use every kind of tile the app has (D-788).
fn others(words: &Words) -> String {
    let mut parts = vec![page_top().to_owned()];
    let mut row = 0;
    for (prefix, components) in [
        ("spending", spending(words)),
        ("money", money(words)),
        ("plans", plans(words)),
    ] {
        let moved = shifted(&components, row, prefix);
        row = bottom(&moved) - 3;
        parts.push(moved);
    }
    dashboard(words.others, &parts.join(",\n    "))
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
        top = heading(words.spending_heading),
        compared_title = quoted(words.compared_title),
        by_month_title = quoted(words.by_month_title),
        average_title = quoted(words.average_title),
        this_year_title = quoted(words.this_year_title),
        food_title = quoted(words.food_title),
    );
    components
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
        top = heading(words.money_heading),
        candles_title = quoted(words.candles_title),
        last_year = quoted(words.last_year),
    );
    components
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
        top = heading(words.plans_heading),
        warnings_title = quoted(words.warnings_title),
        goal_title = quoted(words.goal_title),
        formula = quoted(&format!("'{food}'")),
        below = words.below,
        goal = words.food_goal,
    );
    components
}

/// The dashboards of the example book `slug`, in the order the tabs show
/// them, with the book's accounts and categories still named
/// (`@account:Name@`, `@category:Name@`, see [`resolve_names`]). Empty for a
/// slug that is not an example book.
pub fn example_dashboards(slug: &str) -> Vec<String> {
    let Some(words) = words(slug) else {
        return Vec::new();
    };
    vec![overview(&words), others(&words)]
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
