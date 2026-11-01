// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the shared filters (M6c.6, D-350, D-351): where each dimension
//! of a component comes from in a real dashboard document, the document's
//! version 2, the checks against the book, and the filters applied to the
//! real datasets of M6a.1, M6c.1 and M6c.4.

use kassensturz_core::{AccountId, AccountKind, BudgetEvery, GroupKind, GroupMember};
use kassensturz_store::Book;
use serde_json::{Value, json};

use super::*;
use crate::tests::{Sample, autumn, date, money, period, sample};

/// The sample book's starting-balance account.
fn opening(book: &Book) -> AccountId {
    book.all_accounts()
        .expect("accounts")
        .into_iter()
        .find(|account| account.kind == AccountKind::Equity)
        .expect("the sample's starting balances")
        .id
}

fn text(id: AccountId) -> String {
    id.to_string()
}

/// A dashboard with a period box over the first two rows and, inside it, a
/// pie without settings of its own, a pie with its own period and
/// categories, and a key figure card with its own accounts; below the box a
/// line of the balances, a heading, a component of a type this version
/// doesn't know, and the filter bar, without which the page's filters don't
/// apply (D-475).
fn dashboard(filters: &Value) -> Document {
    let text = json!({
        "format": FORMAT, "version": VERSION, "kind": "dashboard", "name": "Overview",
        "filters": filters,
        "components": [
            {"id": "box", "type": "group.period", "props": {"period": "last-year"},
             "frame": {"col": 0, "row": 0, "cols": 12, "rows": 8}},
            {"id": "plain", "type": "chart.pie", "dataset": "spending_by_category",
             "frame": {"col": 0, "row": 0, "cols": 6, "rows": 4}},
            {"id": "own", "type": "chart.pie", "dataset": "spending_by_category",
             "params": {"categories": ["c-1"]}, "props": {"period": "this-month"},
             "frame": {"col": 6, "row": 0, "cols": 6, "rows": 4}},
            {"id": "card", "type": "kpi.card", "dataset": "account_balances",
             "params": {"accounts": ["a-1", "a-2"]},
             "frame": {"col": 0, "row": 4, "cols": 3, "rows": 2}},
            {"id": "worth", "type": "chart.line", "dataset": "net_worth_over_time",
             "frame": {"col": 0, "row": 8, "cols": 12, "rows": 4}},
            {"id": "title", "type": "text.heading", "props": {"text": "Money"},
             "frame": {"col": 0, "row": 12, "cols": 4, "rows": 1}},
            {"id": "later", "type": "chart.sankey", "dataset": "spending_by_category",
             "frame": {"col": 4, "row": 12, "cols": 4, "rows": 1}},
            {"id": "bar", "type": "page.filters",
             "frame": {"col": 0, "row": 13, "cols": 12, "rows": 1}}
        ]
    })
    .to_string();
    Document::read(&text).expect("a valid dashboard")
}

fn chosen(ids: &[&str], origin: Origin) -> Option<Chosen> {
    Some(Chosen {
        ids: ids.iter().map(|id| (*id).to_owned()).collect(),
        origin,
    })
}

#[test]
fn an_own_setting_wins_for_its_dimension_only() {
    let page = dashboard(&json!({
        "period": "last-12-months",
        "accounts": ["a-9"],
        "categories": ["c-9"]
    }));
    let bar = page.filters();
    assert_eq!(bar.period.as_deref(), Some("last-12-months"));
    assert_eq!(bar.accounts, ["a-9"]);
    assert_eq!(bar.categories, ["c-9"]);

    // In the box, with nothing of its own: the box's period, the bar's
    // accounts and categories.
    let plain = page.asked("plain", &bar).expect("a pie asks");
    assert_eq!(
        (plain.period.as_str(), plain.period_origin),
        ("last-year", Origin::PeriodBox)
    );
    assert_eq!(plain.accounts, chosen(&["a-9"], Origin::FilterBar));
    assert_eq!(plain.categories, chosen(&["c-9"], Origin::FilterBar));
    assert_eq!(plain.takes, Takes::ALL);

    // Its own period and categories win over the box and the bar; the
    // accounts still follow the bar.
    let own = page.asked("own", &bar).expect("a pie asks");
    assert_eq!(
        (own.period.as_str(), own.period_origin),
        ("this-month", Origin::Own)
    );
    assert_eq!(own.categories, chosen(&["c-1"], Origin::Own));
    assert_eq!(own.accounts, chosen(&["a-9"], Origin::FilterBar));

    let card = page.asked("card", &bar).expect("a card asks");
    assert_eq!(card.accounts, chosen(&["a-1", "a-2"], Origin::Own));
    // Balances take no categories; the bar's are passed on, and applying
    // them notes that they do not apply.
    assert_eq!(card.takes, Takes::NO_CATEGORIES);
    assert_eq!(card.categories, chosen(&["c-9"], Origin::FilterBar));

    // Outside the box: the bar's period.
    let worth = page.asked("worth", &bar).expect("a line asks");
    assert_eq!(
        (worth.period.as_str(), worth.period_origin),
        ("last-12-months", Origin::FilterBar)
    );

    // Nothing to ask for: a heading, the box itself, an unknown type, an ID
    // the document doesn't have.
    for id in ["title", "box", "later", "nobody"] {
        assert_eq!(page.asked(id, &bar), None, "{id}");
    }

    // Without a bar: the page's own period, everything.
    let empty = Filters::default();
    let worth = page.asked("worth", &empty).expect("a line asks");
    assert_eq!(
        (worth.period.as_str(), worth.period_origin),
        ("page", Origin::Default)
    );
    assert_eq!((worth.accounts, worth.categories), (None, None));
    // A choice for now replaces the document's own filters.
    let for_now = Filters {
        accounts: vec!["a-5".into()],
        ..Filters::default()
    };
    let plain = page.asked("plain", &for_now).expect("a pie asks");
    assert_eq!(plain.accounts, chosen(&["a-5"], Origin::FilterBar));
    assert_eq!(plain.categories, None);
}

#[test]
fn version_2_keeps_the_filters_and_version_1_is_still_read() {
    let page = dashboard(&json!({"accounts": ["a-1"], "categories": ["c-1", "c-2"]}));
    let written = page.to_json();
    let value: Value = serde_json::from_str(&written).unwrap();
    assert_eq!(value["version"], VERSION);
    assert_eq!(
        value["filters"],
        json!({"accounts": ["a-1"], "categories": ["c-1", "c-2"]})
    );
    // Every component that counts accounts carries its own list.
    let card = value["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["id"] == "card")
        .unwrap();
    assert_eq!(card["params"]["accounts"], json!(["a-1", "a-2"]));
    assert_eq!(Document::read(&written).unwrap(), page);

    // Empty lists are not written, and nothing at all leaves `filters` out.
    let bare = dashboard(&json!({"accounts": [], "categories": []}));
    assert!(bare.accounts.is_empty() && bare.categories.is_empty());
    let value: Value = serde_json::from_str(&bare.to_json()).unwrap();
    assert_eq!(value.get("filters"), None);

    // A version 1 document of an earlier build opens, with no filters, and
    // is written back as the current version.
    let old = Document::read(
        &json!({"format": FORMAT, "version": 1, "kind": "dashboard", "name": "Old",
            "components": []})
        .to_string(),
    )
    .expect("a version 1 dashboard");
    assert_eq!(old.filters(), Filters::default());
    let value: Value = serde_json::from_str(&old.to_json()).unwrap();
    assert_eq!(value["version"], VERSION);

    // A newer one is refused by its version, not as a reading error.
    let newer = written.replace(
        &format!("\"version\":{VERSION}"),
        &format!("\"version\":{}", VERSION + 1),
    );
    assert!(matches!(
        Document::read(&newer),
        Err(ReportError::UnsupportedDocumentVersion { version }) if version == i64::from(VERSION + 1)
    ));
}

fn read_with(filters: &Value) -> Result<Document, ReportError> {
    let text = json!({
        "format": FORMAT, "version": 2, "kind": "dashboard", "name": "Overview",
        "filters": filters, "components": []
    })
    .to_string();
    Document::read(&text)
}

fn reason_of(result: Result<Document, ReportError>) -> (&'static str, String) {
    match result {
        Err(ReportError::DocumentInvalid { reason, path }) => (reason, path),
        Err(ReportError::FilterInvalid { reason, dimension }) => (reason, dimension.to_owned()),
        other => panic!("refused, not {other:?}"),
    }
}

#[test]
fn a_document_with_a_bad_filter_list_is_refused() {
    let many: Vec<String> = (0..=MAX_FILTER_IDS).map(|n| format!("a-{n}")).collect();
    let enough = &many[..MAX_FILTER_IDS];
    assert_eq!(
        read_with(&json!({"accounts": enough}))
            .unwrap()
            .accounts
            .len(),
        MAX_FILTER_IDS
    );
    for (filters, refused) in [
        (json!({"accounts": many}), ("too-many", "accounts")),
        (
            json!({"categories": ["c-1", "C-1"]}),
            ("repeated", "categories"),
        ),
        (
            json!({"accounts": "a-1"}),
            ("wrong-type", "filters.accounts"),
        ),
        (json!({"accounts": [1]}), ("wrong-type", "filters.accounts")),
        (
            json!({"categories": [""]}),
            ("bad-text", "filters.categories"),
        ),
        (
            json!({"categories": ["c\n1"]}),
            ("bad-text", "filters.categories"),
        ),
        (
            json!({"payees": ["p-1"]}),
            ("unknown-property", "filters.payees"),
        ),
    ] {
        let (reason, path) = reason_of(read_with(&filters));
        assert_eq!((reason, path.as_str()), refused, "{filters}");
    }
    // A component's own accounts follow the same bound.
    let mut card = Component::new(ComponentType::KpiCard, crate::ACCOUNT_BALANCES).unwrap();
    assert!(card.set("accounts", Setting::Ids(many.clone())).is_err());
    card.set("accounts", Setting::Ids(enough.to_vec())).unwrap();
}

/// What `asked` keeps of the sample book.
fn applied(base: &Sample, asked: &Asked) -> Result<Applied, ReportError> {
    let source = Source::read(&base.book, autumn()).expect("read");
    asked.apply(&source)
}

fn asked(accounts: &[String], categories: &[String], takes: Takes) -> Asked {
    let list = |ids: &[String]| {
        (!ids.is_empty()).then(|| Chosen {
            ids: ids.to_vec(),
            origin: Origin::FilterBar,
        })
    };
    Asked {
        period: "page".into(),
        period_origin: Origin::Default,
        accounts: list(accounts),
        categories: list(categories),
        takes,
    }
}

#[test]
fn the_ids_are_checked_against_the_book() {
    let mut base = sample();
    let opening = opening(&base.book);
    let household = base
        .book
        .add_category_group(
            "Household",
            GroupKind::Expense,
            &[
                GroupMember::Category(base.rent),
                GroupMember::Category(base.food),
            ],
        )
        .expect("group added");
    let gone = AccountId::generate();

    // Accounts and categories as the book writes them, in any capitals; a
    // group counts its members, each category once.
    let kept = applied(
        &base,
        &asked(
            &[text(base.savings).to_uppercase(), text(base.checking)],
            &[
                household.to_string(),
                text(base.rent),
                text(gone),
                text(base.travel),
            ],
            Takes::ALL,
        ),
    )
    .unwrap();
    assert_eq!(
        kept.filter.accounts,
        Some(vec![base.savings, base.checking])
    );
    // The group's members in the order of the category tree (D-291).
    let mut categories = kept.filter.categories.clone().unwrap();
    assert_eq!(categories.len(), 4, "{categories:?}");
    assert_eq!(categories.pop(), Some(base.travel));
    categories.sort();
    let mut expected = vec![base.food, base.groceries, base.rent];
    expected.sort();
    assert_eq!(categories, expected);
    assert_eq!(kept.filtered, [Dimension::Accounts, Dimension::Categories]);
    assert!(kept.own.is_empty());
    assert_eq!(
        kept.notes,
        [FilterNote::Gone {
            dimension: Dimension::Categories,
            id: text(gone)
        }]
    );
    assert!(!kept.chooses_nothing());

    // What can't be narrowed by is refused, by a fixed word.
    for (accounts, categories, refused) in [
        (
            vec![text(base.food)],
            vec![],
            ("not-an-account", "accounts"),
        ),
        (vec![text(opening)], vec![], ("not-an-account", "accounts")),
        (
            vec![household.to_string()],
            vec![],
            ("not-an-account", "accounts"),
        ),
        (
            vec![],
            vec![text(base.checking)],
            ("not-a-category", "categories"),
        ),
        (
            vec![],
            vec![text(opening)],
            ("not-a-category", "categories"),
        ),
        (
            vec![text(base.savings), text(base.savings).to_uppercase()],
            vec![],
            ("repeated", "accounts"),
        ),
    ] {
        let result = applied(&base, &asked(&accounts, &categories, Takes::ALL));
        match result {
            Err(ReportError::FilterInvalid { reason, dimension }) => {
                assert_eq!((reason, dimension), refused);
            }
            other => panic!("{refused:?} refused, not {other:?}"),
        }
    }
    // Refused even where the dataset does not take the dimension: the bar
    // is the same for every component.
    assert!(applied(&base, &asked(&[text(base.food)], &[], Takes::NO_ACCOUNTS)).is_err());
    let many: Vec<String> = (0..=MAX_FILTER_IDS)
        .map(|_| text(AccountId::generate()))
        .collect();
    assert!(matches!(
        applied(&base, &asked(&many, &[], Takes::ALL)),
        Err(ReportError::FilterInvalid {
            reason: "too-many",
            dimension: "accounts"
        })
    ));

    // Only gone IDs: the filter chooses nothing, and says so.
    let nothing = applied(&base, &asked(&[text(gone)], &[], Takes::ALL)).unwrap();
    assert_eq!(nothing.filter.accounts, Some(Vec::new()));
    assert_eq!(
        nothing.notes,
        [
            FilterNote::Gone {
                dimension: Dimension::Accounts,
                id: text(gone)
            },
            FilterNote::ChoosesNothing(Dimension::Accounts)
        ]
    );
    assert!(nothing.chooses_nothing());
}

#[test]
fn the_spending_follows_the_filter_bar_and_a_filter_that_chooses_nothing_draws_nothing() {
    let base = sample();
    let source = Source::read(&base.book, autumn()).expect("read");
    let food_from_checking = asked(
        &[text(base.checking)],
        &[text(base.food)],
        crate::dataset(SPENDING_BY_CATEGORY).unwrap().takes,
    )
    .apply(&source)
    .unwrap();
    let results = spending_by_category(
        &source,
        autumn(),
        &food_from_checking.filter,
        Level::TopLevel,
    )
    .unwrap();
    // Groceries 53.20 + 40.00, Food itself 10.00, less a refund of 15.00;
    // nothing of rent, travel or money not sorted yet.
    let rows: Vec<_> = results
        .iter()
        .flat_map(|result| result.rows.iter())
        .map(|row| (row.category, row.total.minor_units()))
        .collect();
    assert_eq!(rows, [(Some(base.food), 8_820)]);

    // The dollar account spent nothing on food: no figures at all.
    let none = asked(&[text(base.dollars)], &[text(base.food)], Takes::ALL)
        .apply(&source)
        .unwrap();
    assert!(
        spending_by_category(&source, autumn(), &none.filter, Level::TopLevel)
            .unwrap()
            .is_empty()
    );
    // A list left with nothing is an empty dataset, which the component
    // says instead of drawing zeros.
    let gone = asked(&[text(AccountId::generate())], &[], Takes::ALL)
        .apply(&source)
        .unwrap();
    assert!(gone.chooses_nothing());
    assert!(
        income_expense_by_month(&source, autumn(), &gone.filter)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn the_balances_take_the_accounts_and_say_they_are_not_filtered_by_category() {
    let base = sample();
    let source = Source::read_with_balances(&base.book, autumn()).expect("read");
    let takes = crate::dataset(ACCOUNT_BALANCES).unwrap().takes;
    let savings = asked(&[text(base.savings)], &[text(base.food)], takes)
        .apply(&source)
        .unwrap();
    // The categories are not applied, and the note says so.
    assert_eq!(savings.filter.categories, None);
    assert_eq!(savings.filtered, [Dimension::Accounts]);
    assert_eq!(
        savings.notes,
        [FilterNote::NotApplied(Dimension::Categories)]
    );

    let balances = account_balances(&source, autumn(), &savings.filter, None).unwrap();
    let [eur] = balances.results.as_slice() else {
        panic!("only the euro account is chosen: {balances:?}");
    };
    let rows: Vec<_> = eur
        .rows
        .iter()
        .map(|row| (row.account, row.balance.minor_units()))
        .collect();
    // The Transfer of 300.00 from checking on 21 October.
    assert_eq!(rows, [(base.savings, 30_000)]);
    assert_eq!(eur.worth.net_worth, money(30_000));

    // Each month's worth of the chosen account only.
    let worth = net_worth_over_time(&source, autumn(), &savings.filter, None).unwrap();
    let months: Vec<_> = worth.results[0]
        .rows
        .iter()
        .map(|row| (row.month, row.worth.net_worth.minor_units()))
        .collect();
    assert_eq!(
        months,
        [
            (date(2026, 8, 1), 0),
            (date(2026, 9, 1), 0),
            (date(2026, 10, 1), 30_000)
        ]
    );

    // Money moved in from checking, which is not chosen, is money in.
    let takes = crate::dataset(CASH_FLOW_BY_MONTH).unwrap().takes;
    let chosen = asked(&[text(base.savings)], &[], takes)
        .apply(&source)
        .unwrap();
    let flow = cash_flow_by_month(&source, autumn(), &chosen.filter).unwrap();
    let total = &flow[0].total;
    assert_eq!(
        (total.start, total.money_in, total.money_out, total.end),
        (money(0), money(30_000), money(0), money(30_000))
    );
    // Both accounts chosen: the Transfer between them is neither in nor out.
    let both = asked(&[text(base.savings), text(base.checking)], &[], takes)
        .apply(&source)
        .unwrap();
    let flow = cash_flow_by_month(&source, autumn(), &both.filter).unwrap();
    let october = flow[0].rows.last().unwrap();
    assert_eq!(october.month, date(2026, 10, 1));
    // Rent 800.00 and 12.50 not sorted out; salary 2,500.00 and 7.00 in.
    assert_eq!(
        (october.money_in, october.money_out),
        (money(250_700), money(81_250))
    );
}

#[test]
fn a_budget_counts_every_account_and_takes_the_categories() {
    let mut base = sample();
    let first = date(2026, 8, 1);
    let food = base
        .book
        .add_budget(
            base.food,
            BudgetEvery::Month,
            first,
            None,
            money(5_000),
            false,
        )
        .unwrap();
    let rent = base
        .book
        .add_budget(
            base.rent,
            BudgetEvery::Month,
            first,
            None,
            money(80_000),
            false,
        )
        .unwrap();
    let today = date(2026, 12, 1);
    let shown = period(date(2026, 9, 1), date(2026, 9, 30));
    let source = Source::read_with_budgets(&base.book, shown, today).expect("read");
    let takes = crate::dataset(BUDGET_UTILIZATION)
        .expect("in the catalog")
        .takes;
    assert_eq!(takes, Takes::NO_ACCOUNTS);

    // The bar's accounts are not applied: both budgets, all their spending.
    let every = asked(&[text(base.savings)], &[], takes)
        .apply(&source)
        .unwrap();
    assert_eq!(every.filter, Filter::default());
    assert!(every.filtered.is_empty());
    assert_eq!(every.notes, [FilterNote::NotApplied(Dimension::Accounts)]);
    let spent = |filter: &Filter| -> Vec<_> {
        budget_utilization(&source, shown, today, BudgetGrouping::Budget, filter)
            .unwrap()
            .iter()
            .map(|result| (result.budget, result.periods[0].spent.minor_units()))
            .collect()
    };
    // Food: 40.00 + 10.00 − 15.00 in September; rent 800.00.
    assert_eq!(spent(&every.filter), [(food, 3_500), (rent, 80_000)]);

    // The categories choose the budgets, and a gone account in the list
    // that is not applied leaves no note behind.
    let only_rent = asked(&[text(AccountId::generate())], &[text(base.rent)], takes)
        .apply(&source)
        .unwrap();
    assert_eq!(
        only_rent.notes,
        [FilterNote::NotApplied(Dimension::Accounts)]
    );
    assert_eq!(spent(&only_rent.filter), [(rent, 80_000)]);
}

#[test]
fn every_dataset_says_what_it_takes() {
    let takes = |id: &str| crate::dataset(id).map(|dataset| dataset.takes);
    for dataset in crate::CATALOG.iter() {
        assert!(dataset.takes.period, "{} takes the period", dataset.id);
    }
    assert_eq!(takes(SPENDING_BY_CATEGORY), Some(Takes::ALL));
    assert_eq!(takes(INCOME_EXPENSE_BY_MONTH), Some(Takes::ALL));
    assert_eq!(takes(CATEGORY_OVER_TIME), Some(Takes::ALL));
    for id in [ACCOUNT_BALANCES, NET_WORTH_OVER_TIME, CASH_FLOW_BY_MONTH] {
        assert_eq!(takes(id), Some(Takes::NO_CATEGORIES), "{id}");
    }
    assert_eq!(takes(BUDGET_UTILIZATION), Some(Takes::NO_ACCOUNTS));

    // The cards the page fills take what the dataset they come from takes;
    // what shows no figures takes nothing.
    let of = |kind: ComponentType| crate::takes(&Component::new(kind, "").unwrap());
    assert_eq!(of(ComponentType::NetWorth), Takes::NO_CATEGORIES);
    assert_eq!(of(ComponentType::IncomeSources), Takes::ALL);
    for kind in [
        ComponentType::Heading,
        ComponentType::Separator,
        ComponentType::PeriodBox,
        ComponentType::FetchNotice,
        ComponentType::TransfersNote,
        ComponentType::PeriodHeader,
        ComponentType::FilterBar,
    ] {
        assert_eq!(of(kind), Takes::NOTHING, "{kind:?}");
    }

    // Every component that shows data has its own accounts.
    for kind in ComponentType::ALL {
        assert!(
            kind.properties()
                .iter()
                .any(|property| property.key == "accounts" && property.kind == Kind::Accounts),
            "{kind:?}"
        );
    }
    assert_eq!(
        [
            Dimension::Period,
            Dimension::Accounts,
            Dimension::Categories
        ]
        .map(Dimension::word),
        ["period", "accounts", "categories"]
    );
}

#[test]
fn an_own_period_is_marked_and_the_box_period_is_not() {
    let base = sample();
    let page = dashboard(&json!({"accounts": [text(base.checking)]}));
    let source = Source::read(&base.book, autumn()).expect("read");
    let bar = page.filters();
    let own = page.asked("own", &bar).unwrap();
    // Its own categories name a category the book doesn't have.
    let applied = own.apply(&source).unwrap();
    assert_eq!(applied.own, [Dimension::Period, Dimension::Categories]);
    assert_eq!(applied.filter.accounts, Some(vec![base.checking]));
    assert_eq!(
        applied.notes,
        [
            FilterNote::Gone {
                dimension: Dimension::Categories,
                id: "c-1".into()
            },
            FilterNote::ChoosesNothing(Dimension::Categories)
        ]
    );
    let plain = page.asked("plain", &bar).unwrap().apply(&source).unwrap();
    assert!(plain.own.is_empty());
    assert_eq!(plain.filtered, [Dimension::Accounts]);
    assert!(plain.notes.is_empty());
}
