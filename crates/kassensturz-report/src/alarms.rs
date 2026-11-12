// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The rules of the warnings tile and what they find (US4.9, D-721 to
//! D-726; design in `docs/REPORT_DESIGNER_US4_ALARMS.md`).
//!
//! A rule is a way of looking at the book, nothing more: this module only
//! reads accounts, balances and budgets, and never changes a booking. Every
//! comparison is on exact integer minor units of one currency; a share is
//! compared by multiplying both sides in 128-bit integers, never by
//! dividing, so nothing rounds or overflows (D-723). The words the user
//! reads are made in the app from the fixed words here (D-034); none of
//! them is advice (AGENTS.md §3.1 rule 11).

use kassensturz_core::{AccountId, Budget, BudgetEvery, BudgetId, CurrencyCode, Date, Money};
use kassensturz_store::Book;
use serde_json::{Map, Value};

use crate::balances::Moment;
use crate::{
    BudgetGrouping, BudgetPeriod, Filter, Period, ReportError, Source, budget_utilization,
};

/// The most rules one tile holds (D-722).
pub const MAX_ALARMS: usize = 20;

/// The highest share a budget rule takes, in percent (D-722).
pub const MAX_SHARE: u16 = 1000;

/// The most periods of a budget a reserve takes (D-722).
pub const MAX_TIMES: u8 = 24;

/// The longest amount, in characters (D-722).
pub const MAX_AMOUNT_CHARS: usize = 20;

/// The longest ID, in characters; an ID is a UUID written as text.
const MAX_ID_CHARS: usize = 64;

/// What a rule watches, by the fixed word documents use (D-723).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlarmReason {
    /// `budget_over`: the budget's period that contains today has used more
    /// than `share` percent of what is available.
    BudgetOver,
    /// `budget_under`: the budget's last finished period used less than
    /// `share` percent of what was available.
    BudgetUnder,
    /// `account_below`: the account's balance is under `amount`.
    AccountBelow,
    /// `account_reserve`: the account's balance is under `times` times what
    /// the budget plans for its current period (D-725).
    AccountReserve,
}

impl AlarmReason {
    /// Every reason, in the order the rule editor offers them.
    pub const ALL: [Self; 4] = [
        Self::BudgetOver,
        Self::BudgetUnder,
        Self::AccountBelow,
        Self::AccountReserve,
    ];

    /// The fixed word documents use.
    pub const fn id(self) -> &'static str {
        match self {
            Self::BudgetOver => "budget_over",
            Self::BudgetUnder => "budget_under",
            Self::AccountBelow => "account_below",
            Self::AccountReserve => "account_reserve",
        }
    }

    /// The reason with this word.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reason| reason.id() == id)
    }

    /// The fields a rule of this reason holds besides `reason`, in the order
    /// a document writes them. `lowest` may be left out.
    const fn fields(self) -> &'static [&'static str] {
        match self {
            Self::BudgetOver | Self::BudgetUnder => &["budget", "share"],
            Self::AccountBelow => &["account", "amount", "lowest"],
            Self::AccountReserve => &["account", "budget", "times", "lowest"],
        }
    }

    /// Whether it watches an account (and so takes `lowest`).
    pub const fn on_account(self) -> bool {
        matches!(self, Self::AccountBelow | Self::AccountReserve)
    }

    /// Whether it names a budget.
    pub const fn on_budget(self) -> bool {
        !matches!(self, Self::AccountBelow)
    }
}

/// One rule of a warnings tile (D-722). The fields a reason doesn't use are
/// empty, zero or `false` and are not written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlarmRule {
    /// What it watches.
    pub reason: AlarmReason,
    /// The account's ID as text, for an account rule.
    pub account: String,
    /// The budget's ID as text, for a budget rule and a reserve.
    pub budget: String,
    /// The limit of `account_below` as a plain decimal: an optional minus,
    /// digits, and a dot before the minor digits (`"500.00"`). It is read
    /// in the account's currency with the digits the book stores for it.
    pub amount: String,
    /// The share of a budget rule, in whole percent, 1 to [`MAX_SHARE`].
    pub share: u16,
    /// The periods of the budget a reserve takes, 1 to [`MAX_TIMES`].
    pub times: u8,
    /// Whether an account rule compares this month's lowest end-of-day
    /// balance instead of today's (D-724).
    pub lowest: bool,
}

impl AlarmRule {
    /// A new rule of `reason` with its defaults (design §2): a share of 100
    /// for over and 80 for under, one period, and the lowest point for a
    /// reserve only.
    pub fn new(reason: AlarmReason) -> Self {
        Self {
            reason,
            account: String::new(),
            budget: String::new(),
            amount: String::new(),
            share: match reason {
                AlarmReason::BudgetOver => 100,
                AlarmReason::BudgetUnder => 80,
                _ => 0,
            },
            times: if reason == AlarmReason::AccountReserve {
                1
            } else {
                0
            },
            lowest: reason == AlarmReason::AccountReserve,
        }
    }

    /// Reads one rule as a document writes it; `None` for anything that is
    /// not exactly the fields of its reason with values of the right kind.
    /// The values' ranges are checked by [`AlarmRule::check`].
    pub fn read(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let reason = AlarmReason::from_id(object.get("reason")?.as_str()?)?;
        let fields = reason.fields();
        if object
            .keys()
            .any(|key| key != "reason" && !fields.contains(&key.as_str()))
        {
            return None;
        }
        let mut rule = Self::new(reason);
        for &field in fields {
            let Some(value) = object.get(field) else {
                if field == "lowest" {
                    continue;
                }
                return None;
            };
            match field {
                "account" => rule.account = value.as_str()?.to_owned(),
                "budget" => rule.budget = value.as_str()?.to_owned(),
                "amount" => rule.amount = value.as_str()?.to_owned(),
                "share" => rule.share = u16::try_from(value.as_u64()?).ok()?,
                "times" => rule.times = u8::try_from(value.as_u64()?).ok()?,
                "lowest" => rule.lowest = value.as_bool()?,
                _ => return None,
            }
        }
        Some(rule)
    }

    /// The rule as a document writes it: `reason` and the fields of its
    /// reason only.
    pub fn to_value(&self) -> Value {
        let mut object = Map::new();
        object.insert("reason".into(), self.reason.id().into());
        for &field in self.reason.fields() {
            let value: Value = match field {
                "account" => self.account.clone().into(),
                "budget" => self.budget.clone().into(),
                "amount" => self.amount.clone().into(),
                "share" => self.share.into(),
                "times" => self.times.into(),
                _ => self.lowest.into(),
            };
            object.insert(field.into(), value);
        }
        Value::Object(object)
    }

    /// Whether every field holds a value it may hold; the word of what is
    /// wrong otherwise (`bad-text`, `bad-amount`, `out-of-range`).
    pub fn check(&self) -> Result<(), &'static str> {
        let id_ok = |id: &str| {
            !id.is_empty()
                && id.chars().count() <= MAX_ID_CHARS
                && !id.chars().any(char::is_control)
        };
        if self.reason.on_account() && !id_ok(&self.account) {
            return Err("bad-text");
        }
        if self.reason.on_budget() && !id_ok(&self.budget) {
            return Err("bad-text");
        }
        match self.reason {
            AlarmReason::BudgetOver | AlarmReason::BudgetUnder => {
                if !(1..=MAX_SHARE).contains(&self.share) {
                    return Err("out-of-range");
                }
            }
            AlarmReason::AccountBelow => {
                if read_amount(&self.amount).is_none() {
                    return Err("bad-amount");
                }
            }
            AlarmReason::AccountReserve => {
                if !(1..=MAX_TIMES).contains(&self.times) {
                    return Err("out-of-range");
                }
            }
        }
        Ok(())
    }
}

/// A plain decimal of a rule (`-?digits(.digits)?`, at most
/// [`MAX_AMOUNT_CHARS`] characters) as its sign, whole part and fraction
/// digits; `None` for any other text.
fn read_amount(text: &str) -> Option<(bool, &str, &str)> {
    if text.is_empty() || text.len() > MAX_AMOUNT_CHARS {
        return None;
    }
    let (negative, rest) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (whole, fraction) = match rest.split_once('.') {
        Some((whole, fraction)) => {
            if fraction.is_empty() {
                return None;
            }
            (whole, fraction)
        }
        None => (rest, ""),
    };
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if whole.is_empty() || !digits(whole) || !digits(fraction) {
        return None;
    }
    Some((negative, whole, fraction))
}

/// The amount of a rule as `Money` in `currency` with `digits` minor digits;
/// `None` if it has more nonzero fraction digits than the currency or does
/// not fit.
pub fn amount_money(text: &str, currency: CurrencyCode, digits: u8) -> Option<Money> {
    let (negative, whole, fraction) = read_amount(text)?;
    let digits = usize::from(digits);
    let (kept, extra) = fraction.split_at(fraction.len().min(digits));
    if extra.bytes().any(|byte| byte != b'0') {
        return None;
    }
    let mut minor: i64 = 0;
    for byte in whole.bytes().chain(kept.bytes()) {
        minor = minor.checked_mul(10)?.checked_add(i64::from(byte - b'0'))?;
    }
    for _ in kept.len()..digits {
        minor = minor.checked_mul(10)?;
    }
    if negative {
        minor = minor.checked_neg()?;
    }
    Some(Money::from_minor_units(minor, currency))
}

/// The plain decimal a rule stores for `amount` (D-722): the minor digits
/// the currency has, a dot before them, a minus for a negative amount.
pub fn amount_text(amount: Money, digits: u8) -> String {
    let minor = i128::from(amount.minor_units());
    let sign = if minor < 0 { "-" } else { "" };
    let magnitude = minor.unsigned_abs();
    if digits == 0 {
        return format!("{sign}{magnitude}");
    }
    let scale = 10u128.pow(u32::from(digits));
    format!(
        "{sign}{}.{:0width$}",
        magnitude / scale,
        magnitude % scale,
        width = usize::from(digits)
    )
}

/// Why a rule could not be checked (D-725).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CantCheck {
    /// `currencies`: the account and the budget are in different currencies,
    /// which are never converted for a target.
    Currencies,
    /// `amount`: the rule's amount has more decimal places than the
    /// account's currency, or is too large.
    Amount,
}

impl CantCheck {
    /// The fixed word the app looks the sentence up by.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Currencies => "currencies",
            Self::Amount => "amount",
        }
    }
}

/// What a rule found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlarmState {
    /// `triggered`: the limit is crossed.
    Triggered,
    /// `fine`: it is not, or there is nothing to check yet (a budget with no
    /// finished period for `budget_under`).
    Fine,
    /// `gone`: its account or budget is no longer in the book; it is never
    /// dropped silently.
    Gone,
    /// `cant-check`: it could not be compared, and why; never counted as
    /// fine.
    CantCheck(CantCheck),
}

impl AlarmState {
    /// The fixed word the app reads.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Triggered => "triggered",
            Self::Fine => "fine",
            Self::Gone => "gone",
            Self::CantCheck(_) => "cant-check",
        }
    }
}

/// The budget figures a budget rule or a reserve looked at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetLook {
    /// The budget.
    pub budget: BudgetId,
    /// How long its periods are, for "this month" or "this quarter".
    pub every: BudgetEvery,
    /// The period the figures are of.
    pub period: BudgetPeriod,
}

/// What one rule found, with the figures its sentence names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlarmResult {
    /// The rule, as it was asked.
    pub rule: AlarmRule,
    /// What it found.
    pub state: AlarmState,
    /// The account of an account rule, if the book still has it.
    pub account: Option<AccountId>,
    /// The balance compared: today's, or this month's lowest (D-724).
    pub balance: Option<Money>,
    /// The day of that balance: today, or the day of the lowest point.
    pub balance_day: Option<Date>,
    /// The limit an account rule compared against: its amount, or `times`
    /// times what the budget plans (D-725).
    pub limit: Option<Money>,
    /// The budget figures, for a budget rule or a reserve.
    pub budget: Option<BudgetLook>,
}

impl AlarmResult {
    fn new(rule: &AlarmRule, state: AlarmState) -> Self {
        Self {
            rule: rule.clone(),
            state,
            account: None,
            balance: None,
            balance_day: None,
            limit: None,
            budget: None,
        }
    }
}

/// What each rule finds in `book` on `today`, in the order of the rules
/// (D-721): never the page's period or filters.
pub fn evaluate(
    book: &Book,
    rules: &[AlarmRule],
    today: Date,
) -> Result<Vec<AlarmResult>, ReportError> {
    let budgets = book.budgets()?;
    let accounts = book.all_accounts()?;
    let mut month: Option<MonthBalances> = None;
    let mut results = Vec::with_capacity(rules.len());
    for rule in rules {
        let budget = budgets
            .iter()
            .find(|budget| budget.id.to_string() == rule.budget);
        let account = accounts
            .iter()
            .find(|account| account.id.to_string() == rule.account);
        if (rule.reason.on_budget() && budget.is_none())
            || (rule.reason.on_account() && account.is_none())
        {
            results.push(AlarmResult::new(rule, AlarmState::Gone));
            continue;
        }
        let result = match (rule.reason, budget, account) {
            (AlarmReason::BudgetOver, Some(budget), _) => {
                let look = budget_now(book, budget, today)?;
                budget_result(rule, look, |period| {
                    over_share(period.spent, period.available, rule.share)
                })
            }
            (AlarmReason::BudgetUnder, Some(budget), _) => {
                match finished_day(budget, today) {
                    None => AlarmResult::new(rule, AlarmState::Fine),
                    Some(day) => {
                        let look = budget_at(book, budget, day)?;
                        budget_result(rule, look, |period| {
                            // Nothing to spend is never "under" (design §3).
                            period.available.minor_units() > 0
                                && under_share(period.spent, period.available, rule.share)
                        })
                    }
                }
            }
            (reason, budget, Some(account)) => {
                let month = match &mut month {
                    Some(month) => month,
                    None => month.insert(MonthBalances::read(book, today)?),
                };
                let (balance, day) = month.balance(account.id, account.currency, rule.lowest)?;
                let mut result = AlarmResult::new(rule, AlarmState::Fine);
                result.account = Some(account.id);
                result.balance = Some(balance);
                result.balance_day = Some(day);
                let limit = match (reason, budget) {
                    (AlarmReason::AccountReserve, Some(budget)) => {
                        let look = budget_now(book, budget, today)?;
                        result.budget = look;
                        match look {
                            None => None,
                            Some(look) if look.period.planned.currency() != account.currency => {
                                result.state = AlarmState::CantCheck(CantCheck::Currencies);
                                None
                            }
                            Some(look) => Some(Money::from_minor_units(
                                look.period
                                    .planned
                                    .minor_units()
                                    .checked_mul(i64::from(rule.times))
                                    .ok_or_else(overflow)?,
                                account.currency,
                            )),
                        }
                    }
                    _ => {
                        let digits = month.source.minor_digits(account.currency);
                        let limit = amount_money(&rule.amount, account.currency, digits);
                        if limit.is_none() {
                            result.state = AlarmState::CantCheck(CantCheck::Amount);
                        }
                        limit
                    }
                };
                if let Some(limit) = limit {
                    result.limit = Some(limit);
                    if balance.minor_units() < limit.minor_units() {
                        result.state = AlarmState::Triggered;
                    }
                }
                result
            }
            // Every reason names an account or a budget, and a missing one
            // was handled above.
            _ => AlarmResult::new(rule, AlarmState::Gone),
        };
        results.push(result);
    }
    Ok(results)
}

fn overflow() -> ReportError {
    ReportError::Store(kassensturz_store::StoreError::AmountOverflow)
}

/// `spent × 100 > available × share`, in 128-bit integers (D-723).
fn over_share(spent: Money, available: Money, share: u16) -> bool {
    i128::from(spent.minor_units()) * 100 > i128::from(available.minor_units()) * i128::from(share)
}

/// `spent × 100 < available × share`, in 128-bit integers (D-723).
fn under_share(spent: Money, available: Money, share: u16) -> bool {
    i128::from(spent.minor_units()) * 100 < i128::from(available.minor_units()) * i128::from(share)
}

fn budget_result(
    rule: &AlarmRule,
    look: Option<BudgetLook>,
    crossed: impl Fn(&BudgetPeriod) -> bool,
) -> AlarmResult {
    let mut result = AlarmResult::new(rule, AlarmState::Fine);
    if let Some(look) = look {
        if !look.period.starts_later && crossed(&look.period) {
            result.state = AlarmState::Triggered;
        }
        result.budget = Some(look);
    }
    result
}

/// The budget's figures as `budget_utilization` gives them for the period
/// that contains today (or its first or last one, D-710).
fn budget_now(
    book: &Book,
    budget: &Budget,
    today: Date,
) -> Result<Option<BudgetLook>, ReportError> {
    budget_at(book, budget, today)
}

/// The budget's figures as they stand on `day`, with the grouping `budget`,
/// so the tile and the bars agree (D-723).
fn budget_at(book: &Book, budget: &Budget, day: Date) -> Result<Option<BudgetLook>, ReportError> {
    let period = Period::new(day, day)?;
    let source = Source::read_for_budgets(book, vec![budget.clone()], period, day)?;
    let shown = budget_utilization(
        &source,
        period,
        day,
        BudgetGrouping::Budget,
        &Filter::default(),
        Some(&[budget.id]),
    )?;
    Ok(shown.into_iter().next().and_then(|shown| {
        shown.periods.first().map(|period| BudgetLook {
            budget: shown.budget,
            every: shown.every,
            period: *period,
        })
    }))
}

/// The day whose figures are the budget's last finished period (D-723):
/// the day before the period that contains today, or today for a budget
/// that has ended (its last period, D-710). `None` while no period is
/// finished yet.
fn finished_day(budget: &Budget, today: Date) -> Option<Date> {
    if budget.first_day > today {
        return None;
    }
    match budget.period_containing(today) {
        Some((start, _)) => {
            let day = start.yesterday().ok()?;
            (day >= budget.first_day).then_some(day)
        }
        // Started and not containing today: it has ended.
        None => Some(today),
    }
}

/// The own accounts' end-of-day balances from the end of the month before
/// to today, read once for every account rule.
struct MonthBalances {
    /// What the balances were read from; it also knows the currencies'
    /// decimal places (D-027).
    source: Source,
    /// The days, oldest first: the end of the month before, then each day
    /// of this month up to today.
    days: Vec<Date>,
    /// The balances at the end of each of `days`.
    balances: Vec<std::collections::HashMap<AccountId, Money>>,
}

impl MonthBalances {
    fn read(book: &Book, today: Date) -> Result<Self, ReportError> {
        let first = today.first_of_month();
        let period = Period::new(first, today)?;
        let source = Source::read_with_balances(book, period)?;
        let mut moments = vec![Moment::Start(first)];
        let mut days = vec![first.yesterday().unwrap_or(first)];
        let mut day = first;
        loop {
            moments.push(Moment::End(day));
            days.push(day);
            if day >= today {
                break;
            }
            day = day.tomorrow().map_err(|_| ReportError::PeriodNotRead)?;
        }
        let balances = source.balances_at(&moments)?;
        Ok(Self {
            source,
            days,
            balances,
        })
    }

    /// The account's balance today, or its lowest end-of-day balance this
    /// month with the day of it (the first such day if several tie: the
    /// day it went down to that).
    fn balance(
        &self,
        account: AccountId,
        currency: CurrencyCode,
        lowest: bool,
    ) -> Result<(Money, Date), ReportError> {
        let at = |index: usize| {
            self.balances[index]
                .get(&account)
                .copied()
                .unwrap_or_else(|| Money::zero(currency))
        };
        let last = self.days.len() - 1;
        if !lowest {
            return Ok((at(last), self.days[last]));
        }
        let mut found = (at(0), self.days[0]);
        for index in 1..=last {
            let balance = at(index);
            if balance.minor_units() < found.0.minor_units() {
                found = (balance, self.days[index]);
            }
        }
        Ok(found)
    }
}
