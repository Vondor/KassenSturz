// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The warnings tile (US4.9, D-721 to D-726): what the rules of a tile find
//! in the open book today, as text for the screen, and the amount a rule's
//! editor typed, in the plain form the tile keeps.
//!
//! Every figure and every comparison comes from `kassensturz-report`
//! (`evaluate_alarms`); this file only names the accounts and budgets and
//! turns amounts into text (D-065). The sentences are made in QML from the
//! fixed words here (D-034). No Qt in this file: `alarms_model.rs` hands the
//! result to QML.

use std::collections::HashMap;

use kassensturz_core::Money;
use kassensturz_report::{
    AlarmReason, AlarmResult, AlarmRule, AlarmState, ReportError, TextFormat, amount_text,
    evaluate_alarms,
};
use kassensturz_store::{Book, StoreError};

use crate::budgets::{choices, every_key};
use crate::charts::AppText;
use crate::format::Locale;
use crate::parse::AmountReader;

/// Why the rules couldn't be looked at.
#[derive(Debug)]
pub enum AlarmsError {
    /// The report layer refused.
    Report(ReportError),
    /// The book refused it.
    Store(StoreError),
}

impl From<StoreError> for AlarmsError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<ReportError> for AlarmsError {
    fn from(error: ReportError) -> Self {
        Self::Report(error)
    }
}

/// One rule as the tile and the rule editor show it. Amounts are text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AlarmRow {
    /// Its place in the tile's list, from 0, so the editor changes the
    /// right one.
    pub index: usize,
    /// The reason's fixed word.
    pub reason: &'static str,
    /// `triggered`, `fine`, `gone` or `cant-check`.
    pub state: &'static str,
    /// Why it couldn't be checked (`currencies`, `amount`); empty otherwise.
    pub why: &'static str,
    /// The account's ID as the rule names it; empty for a budget rule.
    pub account: String,
    /// The account's name; empty if it is gone or for a budget rule.
    pub account_name: String,
    /// The budget's ID as the rule names it; empty for `account_below`.
    pub budget: String,
    /// The budget's name; empty if it is gone or for `account_below`.
    pub budget_name: String,
    /// The budget's period word (`month`, `quarter`, `year`,
    /// `salary-month`, `range`); empty without a budget.
    pub every: &'static str,
    /// Whether the budget's figures are of a period that has ended (the
    /// last finished one, for `budget_under`).
    pub ended: bool,
    /// The account's balance compared; empty without one.
    pub balance: String,
    /// The day of that balance; empty without one.
    pub balance_day: String,
    /// Whether the balance is this month's lowest point (D-724).
    pub lowest: bool,
    /// The limit an account rule compared against; empty without one.
    pub limit: String,
    /// The amount of `account_below` as the user types amounts, to put in
    /// the editor's field; empty for other reasons.
    pub amount_input: String,
    /// What the budget planned, had available and spent in the period.
    pub planned: String,
    /// See `planned`.
    pub available: String,
    /// See `planned`.
    pub spent: String,
    /// By how much it is over, positive; empty unless it is.
    pub over: String,
    /// Whether the budget had nothing to spend in the period.
    pub nothing_to_spend: bool,
    /// The rule's share in percent; 0 for an account rule.
    pub share: u16,
    /// The periods of the budget a reserve takes; 0 for other reasons.
    pub times: u8,
}

/// What the rules of one tile found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AlarmsView {
    /// Every rule, in the tile's order.
    pub rows: Vec<AlarmRow>,
    /// How many are triggered, or gone, or can't be checked: everything the
    /// tile lists as a line of its own.
    pub shown: usize,
}

/// What `rules` find in `book` today.
pub fn view(
    book: &Book,
    locale: &impl Locale,
    rules: &[AlarmRule],
) -> Result<AlarmsView, AlarmsError> {
    let results = evaluate_alarms(book, rules, locale.today())?;
    let names: HashMap<String, String> = book
        .all_accounts()?
        .into_iter()
        .map(|account| (account.id.to_string(), account.name))
        .collect();
    let budget_names: HashMap<String, String> = choices(book)?
        .into_iter()
        .map(|choice| (choice.id, choice.name))
        .collect();
    let text = AppText::new(book, locale);
    let rows: Vec<AlarmRow> = results
        .iter()
        .enumerate()
        .map(|(index, result)| row(book, locale, &text, index, result, &names, &budget_names))
        .collect::<Result<_, StoreError>>()?;
    let shown = rows.iter().filter(|row| row.state != "fine").count();
    Ok(AlarmsView { rows, shown })
}

fn row<L: Locale>(
    book: &Book,
    locale: &L,
    text: &AppText<'_, L>,
    index: usize,
    result: &AlarmResult,
    names: &HashMap<String, String>,
    budget_names: &HashMap<String, String>,
) -> Result<AlarmRow, StoreError> {
    let rule = &result.rule;
    let amount = |money: Option<Money>| money.map(|money| text.amount(money)).unwrap_or_default();
    let mut row = AlarmRow {
        index,
        reason: rule.reason.id(),
        state: result.state.id(),
        why: match result.state {
            AlarmState::CantCheck(why) => why.id(),
            _ => "",
        },
        account: rule.account.clone(),
        // Looked up by the rule, so a rule whose budget is gone still names
        // its account.
        account_name: if rule.reason.on_account() {
            names.get(&rule.account).cloned().unwrap_or_default()
        } else {
            String::new()
        },
        budget: rule.budget.clone(),
        budget_name: if rule.reason.on_budget() {
            budget_names.get(&rule.budget).cloned().unwrap_or_default()
        } else {
            String::new()
        },
        balance: amount(result.balance),
        balance_day: result
            .balance_day
            .map(|day| locale.short_date(day))
            .unwrap_or_default(),
        lowest: rule.lowest,
        limit: amount(result.limit),
        share: rule.share,
        times: rule.times,
        ..AlarmRow::default()
    };
    if rule.reason == AlarmReason::AccountBelow
        && let Some(id) = result.account
        && let Some(account) = book.all_accounts()?.into_iter().find(|a| a.id == id)
    {
        let digits = book.currency_minor_digits(account.currency)?;
        // The amount as the user types it: in their format, all its
        // decimal places, without the symbol, so the editor can read it back.
        row.amount_input = kassensturz_report::amount_money(&rule.amount, account.currency, digits)
            .map(|money| plain_amount(locale, money, digits))
            .unwrap_or_else(|| rule.amount.clone());
    }
    if let Some(look) = result.budget {
        let period = look.period;
        row.every = every_key(look.every);
        row.ended = period.ended || period.counted_to == period.last_day;
        row.planned = text.amount(period.planned);
        row.available = text.amount(period.available);
        row.spent = text.amount(period.spent);
        row.over = amount(period.over());
        row.nothing_to_spend = period.available.minor_units() <= 0;
    }
    Ok(row)
}

/// `money` in the user's number format without a currency symbol, with all
/// `digits` decimal places: what the editor's amount field shows.
fn plain_amount(locale: &impl Locale, money: Money, digits: u8) -> String {
    let plain = amount_text(money, digits);
    let (sign, rest) = plain
        .strip_prefix('-')
        .map_or(("", plain.as_str()), |rest| ("-", rest));
    let (whole, fraction) = rest.split_once('.').unwrap_or((rest, ""));
    let grouped = whole
        .parse::<u64>()
        .map_or_else(|_| whole.to_owned(), |value| locale.group_digits(value));
    if fraction.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}{}{fraction}", locale.decimal_point())
    }
}

/// The amount the user typed for a rule on `account`, in the plain form the
/// tile keeps (D-722), or `None` when it isn't an amount in the account's
/// currency or the account is not in the book.
pub fn read_amount(
    book: &Book,
    locale: &impl Locale,
    typed: &str,
    account: &str,
) -> Result<Option<String>, StoreError> {
    let Some(found) = book
        .all_accounts()?
        .into_iter()
        .find(|candidate| candidate.id.to_string() == account)
    else {
        return Ok(None);
    };
    let format = locale.amount_format(found.currency);
    let group_separator = locale.group_separator();
    let digits = book.currency_minor_digits(found.currency)?;
    let reader = AmountReader {
        format: &format,
        group_separator: &group_separator,
        currency: found.currency,
        minor_digits: digits,
    };
    Ok(reader
        .read(typed.trim())
        .ok()
        .map(|money| amount_text(money, digits)))
}

#[cfg(test)]
pub(crate) mod tests {
    use kassensturz_core::{
        Account, AccountId, AccountKind, AccountType, BudgetEvery, CurrencyCode, Date, Split,
        Transaction, TransactionId,
    };

    use super::*;
    use crate::register::tests::TestLocale;

    pub(crate) const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    pub(crate) struct Fixture {
        pub(crate) book: Book,
        pub(crate) shared: AccountId,
        pub(crate) budget: String,
    }

    fn account(book: &mut Book, name: &str, kind: AccountKind) -> AccountId {
        let id = AccountId::generate();
        book.add_account(&Account {
            id,
            name: name.into(),
            kind,
            currency: EUR,
            account_type: (kind == AccountKind::Asset).then_some(AccountType::Checking),
            parent: None,
            closed: false,
            number: None,
            notes: None,
            institution: None,
        })
        .expect("account");
        id
    }

    fn euros(minor: i64) -> Money {
        Money::from_minor_units(minor, EUR)
    }

    /// A shared account with 1,000.00 from 1 October and rent of 800.00 on
    /// the 3rd; a budget *Household* of 900.00 a month on the rent. Today
    /// (the test locale's) is 19 October.
    pub(crate) fn fixture() -> Fixture {
        let mut book = Book::create_in_memory().expect("book");
        book.use_currency(EUR).expect("currency");
        let shared = account(&mut book, "Shared account", AccountKind::Asset);
        let opening = account(&mut book, "Opening", AccountKind::Equity);
        let rent = account(&mut book, "Rent", AccountKind::Expense);
        let day = |d| Date::new(2026, 10, d).expect("date");
        // The opening is in September, so the month starts at 1,000.00.
        let september = Date::new(2026, 9, 15).expect("date");
        for (when, from, to, minor) in [
            (september, opening, shared, 100_000),
            (day(3), shared, rent, 80_000),
        ] {
            let splits = vec![
                Split::new(to, euros(minor)),
                Split::new(from, euros(-minor)),
            ];
            book.add_transaction(
                &Transaction::new(TransactionId::generate(), when, None, splits).expect("balanced"),
            )
            .expect("added");
        }
        let budget = book
            .add_named_budget(
                Some("Household"),
                &[rent],
                &[],
                BudgetEvery::Month,
                day(1),
                None,
                euros(90_000),
                false,
            )
            .expect("budget")
            .to_string();
        Fixture {
            book,
            shared,
            budget,
        }
    }

    pub(crate) fn rules(f: &Fixture) -> Vec<AlarmRule> {
        vec![
            AlarmRule {
                budget: f.budget.clone(),
                share: 80,
                ..AlarmRule::new(AlarmReason::BudgetOver)
            },
            AlarmRule {
                account: f.shared.to_string(),
                amount: "1500.00".into(),
                ..AlarmRule::new(AlarmReason::AccountBelow)
            },
            AlarmRule {
                account: f.shared.to_string(),
                budget: f.budget.clone(),
                times: 1,
                lowest: true,
                ..AlarmRule::new(AlarmReason::AccountReserve)
            },
            AlarmRule {
                budget: AccountId::generate().to_string(),
                share: 100,
                ..AlarmRule::new(AlarmReason::BudgetOver)
            },
        ]
    }

    #[test]
    fn each_rule_comes_with_its_names_and_figures_as_text() {
        let f = fixture();
        let shown = view(&f.book, &TestLocale, &rules(&f)).expect("view");
        assert_eq!(shown.rows.len(), 4);
        // 800.00 of 900.00 is 88.9 %: more than 80 %.
        let over = &shown.rows[0];
        assert_eq!(
            (over.reason, over.state, over.budget_name.as_str()),
            ("budget_over", "triggered", "Household")
        );
        assert_eq!(over.spent, "800,00 €");
        assert_eq!(over.available, "900,00 €");
        assert_eq!(over.every, "month");
        assert!(!over.ended);
        assert_eq!(over.share, 80);
        // 200.00 today, under 1,500.00.
        let below = &shown.rows[1];
        assert_eq!(below.state, "triggered");
        assert_eq!(below.account_name, "Shared account");
        assert_eq!(below.balance, "200,00 €");
        assert_eq!(below.limit, "1500,00 €");
        assert_eq!(below.amount_input, "1500,00");
        // The lowest point is 200.00 on the 3rd, under 1 × 900.00.
        let reserve = &shown.rows[2];
        assert_eq!(reserve.state, "triggered");
        assert_eq!(reserve.balance_day, "2026-10-03");
        assert_eq!(reserve.limit, "900,00 €");
        assert_eq!(reserve.planned, "900,00 €");
        assert_eq!(reserve.times, 1);
        assert!(reserve.lowest);
        // A budget that is gone is a line of its own.
        assert_eq!(shown.rows[3].state, "gone");
        assert_eq!(shown.rows[3].budget_name, "");
        assert_eq!(shown.shown, 4);
    }

    #[test]
    fn a_typed_amount_is_kept_in_the_plain_form_of_the_tile() {
        let f = fixture();
        let account = f.shared.to_string();
        let read = |typed| read_amount(&f.book, &TestLocale, typed, &account).expect("read");
        assert_eq!(read("1500,5"), Some("1500.50".into()));
        assert_eq!(read("-12"), Some("-12.00".into()));
        assert_eq!(read("500 €"), Some("500.00".into()));
        // Too many decimal places or not a number: refused, never rounded.
        assert_eq!(read("1,005"), None);
        assert_eq!(read("five"), None);
        assert_eq!(read(""), None);
        let gone = read_amount(&f.book, &TestLocale, "5", "not an account").expect("read");
        assert_eq!(gone, None);
    }
}
