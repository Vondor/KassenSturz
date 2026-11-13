// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The commands for planned payments (D-769, `docs/SCHEDULES.md`) and the
//! forecast that reads them (D-770). A plan never moves money by itself:
//! imports match the entries the bank delivers to it, and only
//! `schedule book` creates an entry, on the user's word.

use std::io::Write;

use kassensturz_core::{
    Account, AccountId, Date, Money, OccurrenceState, Schedule, ScheduleId, ScheduleUnit,
};
use kassensturz_store::{Book, NewSchedule};

use crate::amount::{money_text, parse_amount};
use crate::args::Arguments;
use crate::error::CliError;
use crate::input::parse_date;
use crate::lookup::{find_account, find_category, find_transaction, short_id};
use crate::text::{plain, table};
use crate::transfer::today;

/// The options `schedule add` and `schedule edit` share.
const PLAN_OPTIONS: &[&str] = &[
    "every",
    "unit",
    "from",
    "track-from",
    "until",
    "category",
    "tolerance",
    "iban",
    "party",
];

/// `schedule list|add|edit|pause|resume|remove|due|match|skip|unskip|book`.
pub(crate) fn schedule(
    book: &mut Book,
    args: &[String],
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return Err(CliError::usage(
            "schedule needs a command: list, add, edit, pause, resume, remove, due, match, \
             skip, unskip or book",
        ));
    };
    match command.as_str() {
        "list" => {
            Arguments::parse(rest, &[], &[])?.exactly([])?;
            list(book, out)
        }
        "add" => add(book, rest, out),
        "edit" => edit(book, rest, out),
        "pause" | "resume" => {
            let args = Arguments::parse(rest, &[], &[])?;
            let [id] = args.exactly(["id"])?;
            let id = find_schedule(book, id)?;
            let paused = command == "pause";
            book.set_schedule_paused(id, paused)?;
            if paused {
                writeln!(
                    out,
                    "Paused the plan: imports don't match it and the forecast leaves it out."
                )?;
            } else {
                writeln!(out, "The plan is expected again.")?;
            }
            Ok(())
        }
        "remove" => {
            let args = Arguments::parse(rest, &[], &[])?;
            let [id] = args.exactly(["id"])?;
            let id = find_schedule(book, id)?;
            book.remove_schedule(id)?;
            writeln!(
                out,
                "Removed the plan. The entries it matched stay as they are; `undo` brings \
                 the plan back."
            )?;
            Ok(())
        }
        "due" => due(book, rest, out),
        "match" => {
            let args = Arguments::parse(rest, &[], &[])?;
            let [id, due_on, transaction] = args.exactly(["id", "due date", "transaction"])?;
            let id = find_schedule(book, id)?;
            let due_on = counted_date(book, id, parse_date(due_on)?)?;
            let transaction = find_transaction(book, transaction)?;
            book.match_schedule_occurrence(id, due_on, transaction)?;
            writeln!(
                out,
                "The payment due {due_on} is the transaction {}.",
                short_id(transaction)
            )?;
            Ok(())
        }
        "skip" => {
            let args = Arguments::parse(rest, &[], &[])?;
            let [id, due_on] = args.exactly(["id", "due date"])?;
            let id = find_schedule(book, id)?;
            let due_on = counted_date(book, id, parse_date(due_on)?)?;
            book.skip_schedule_occurrence(id, due_on)?;
            writeln!(
                out,
                "Skipped the payment due {due_on}: it isn't expected any more."
            )?;
            Ok(())
        }
        "unskip" => {
            let args = Arguments::parse(rest, &[], &[])?;
            let [id, due_on] = args.exactly(["id", "due date"])?;
            let id = find_schedule(book, id)?;
            let due_on = counted_date(book, id, parse_date(due_on)?)?;
            book.unskip_schedule_occurrence(id, due_on)?;
            writeln!(out, "The payment due {due_on} is expected again.")?;
            Ok(())
        }
        "book" => book_occurrence(book, rest, out),
        other => Err(CliError::usage(format!(
            "`schedule {other}` isn't a command"
        ))),
    }
}

/// `schedule list`: every plan, paused ones too, with its next payment.
fn list(book: &Book, out: &mut dyn Write) -> Result<(), CliError> {
    let plans = book.schedules()?;
    if plans.is_empty() {
        writeln!(out, "No payments are planned.")?;
        return Ok(());
    }
    let today = today();
    let mut rows = vec![vec![
        "Name".to_owned(),
        "Account".to_owned(),
        "Amount".to_owned(),
        "Repeats".to_owned(),
        "Next".to_owned(),
        "Category".to_owned(),
        "ID".to_owned(),
    ]];
    for plan in &plans {
        let next = if plan.paused {
            "paused".to_owned()
        } else {
            match book.next_due(plan.id, today)? {
                Some(due) => due.date.to_string(),
                None => "none".to_owned(),
            }
        };
        rows.push(vec![
            plain(&plan.name),
            plain(&book.account(plan.account)?.name),
            amount_text(book, plan),
            repeats_text(plan),
            next,
            match plan.category {
                Some(category) => plain(&book.account(category)?.name),
                None => String::new(),
            },
            short_id(plan.id),
        ]);
    }
    write!(out, "{}", table(&rows, &[2]))?;
    Ok(())
}

/// The plan's amount, with its tolerance if it has one: `-50.00 EUR ± 5.00`.
fn amount_text(book: &Book, plan: &Schedule) -> String {
    let amount = money_text(Some(book), plan.amount);
    if plan.tolerance.is_zero() {
        amount
    } else {
        let tolerance = money_text(Some(book), plan.tolerance);
        let number = tolerance.split(' ').next().unwrap_or_default();
        format!("{amount} ± {number}")
    }
}

/// How often the plan repeats, from when and until when.
fn repeats_text(plan: &Schedule) -> String {
    let unit = plan.unit.word();
    let every = if plan.every == 1 {
        format!("every {unit}")
    } else {
        format!("every {} {unit}s", plan.every)
    };
    match plan.ends_on {
        Some(ends_on) => format!("{every} from {} until {ends_on}", plan.anchor),
        None => format!("{every} from {}", plan.anchor),
    }
}

/// `schedule add ACCOUNT AMOUNT NAME --from DATE [...]`.
fn add(book: &mut Book, args: &[String], out: &mut dyn Write) -> Result<(), CliError> {
    let args = Arguments::parse(args, PLAN_OPTIONS, &[])?;
    let [account, amount, name] = args.exactly(["account", "amount", "name"])?;
    let account = find_account(book, account)?;
    let amount = amount_in(book, &account, amount)?;
    let anchor = parse_date(args.required("from")?)?;
    let mut plan = Schedule {
        id: ScheduleId::generate(),
        name: name.to_owned(),
        account: account.id,
        amount,
        tolerance: Money::zero(account.currency),
        category: None,
        party_iban: None,
        party_name: None,
        unit: ScheduleUnit::Month,
        every: 1,
        anchor,
        track_from: anchor,
        ends_on: None,
        paused: false,
    };
    apply_options(book, &account, &args, &mut plan)?;
    let new = NewSchedule {
        name: plan.name,
        account: plan.account,
        amount: plan.amount,
        tolerance: plan.tolerance,
        category: plan.category,
        party_iban: plan.party_iban,
        party_name: plan.party_name,
        unit: plan.unit,
        every: plan.every,
        anchor: plan.anchor,
        track_from: plan.track_from,
        ends_on: plan.ends_on,
    };
    let id = book.add_schedule(&new)?;
    writeln!(
        out,
        "Planned {} as {}; imports match it from now on and `undo` takes it back. \
         The plan is {}.",
        money_text(Some(book), amount),
        plain(new.name.trim()),
        short_id(id)
    )?;
    Ok(())
}

/// `schedule edit ID [--name NAME] [--amount AMOUNT] [...]`. A changed start
/// date also moves the date tracking starts from, unless `--track-from`
/// says otherwise.
fn edit(book: &mut Book, args: &[String], out: &mut dyn Write) -> Result<(), CliError> {
    let mut options = PLAN_OPTIONS.to_vec();
    options.extend(["name", "amount"]);
    let args = Arguments::parse(args, &options, &["no-category", "no-end"])?;
    let [id] = args.exactly(["id"])?;
    let id = find_schedule(book, id)?;
    let mut plan = book.schedule(id)?;
    let account = book.account(plan.account)?;
    if let Some(name) = args.value("name")? {
        name.clone_into(&mut plan.name);
    }
    if let Some(amount) = args.value("amount")? {
        plan.amount = amount_in(book, &account, amount)?;
    }
    apply_options(book, &account, &args, &mut plan)?;
    if args.flag("no-category") {
        plan.category = None;
    }
    if args.flag("no-end") {
        plan.ends_on = None;
    }
    book.edit_schedule(&plan)?;
    writeln!(
        out,
        "Changed the plan {}. Payments already matched or skipped stay so.",
        plain(&plan.name)
    )?;
    Ok(())
}

/// Puts the options `add` and `edit` share into `plan`.
fn apply_options(
    book: &Book,
    account: &Account,
    args: &Arguments,
    plan: &mut Schedule,
) -> Result<(), CliError> {
    if let Some(every) = args.value("every")? {
        plan.every = every.parse::<u8>().map_err(|_| {
            CliError::usage(format!(
                "--every takes a number from 1 to 52, not `{}`",
                plain(every)
            ))
        })?;
    }
    if let Some(unit) = args.value("unit")? {
        plan.unit = ScheduleUnit::from_word(unit).ok_or_else(|| {
            CliError::usage(format!(
                "--unit is week, month or year, not `{}`",
                plain(unit)
            ))
        })?;
    }
    if let Some(from) = args.value("from")? {
        let anchor = parse_date(from)?;
        if plan.track_from == plan.anchor {
            plan.track_from = anchor;
        }
        plan.anchor = anchor;
    }
    if let Some(track_from) = args.value("track-from")? {
        plan.track_from = parse_date(track_from)?;
    }
    if let Some(until) = args.value("until")? {
        plan.ends_on = Some(parse_date(until)?);
    }
    if let Some(category) = args.value("category")? {
        plan.category = Some(find_category(book, category)?.id);
    }
    if let Some(tolerance) = args.value("tolerance")? {
        plan.tolerance = amount_in(book, account, tolerance)?;
    }
    if let Some(iban) = args.value("iban")? {
        plan.party_iban = Some(iban.to_owned()).filter(|iban| !iban.trim().is_empty());
    }
    if let Some(party) = args.value("party")? {
        plan.party_name = Some(party.to_owned()).filter(|party| !party.trim().is_empty());
    }
    Ok(())
}

/// `text` as an amount in `account`'s currency.
fn amount_in(book: &Book, account: &Account, text: &str) -> Result<Money, CliError> {
    let digits = book.currency_minor_digits(account.currency)?;
    parse_amount(text, account.currency, digits)
}

/// `schedule due [--from DATE] [--to DATE]`: the payments of the expected
/// plans in that time, from 31 days ago to 31 days ahead if not said.
fn due(book: &Book, args: &[String], out: &mut dyn Write) -> Result<(), CliError> {
    let args = Arguments::parse(args, &["from", "to"], &[])?;
    args.exactly([])?;
    let today = today();
    let from = match args.value("from")? {
        Some(from) => parse_date(from)?,
        None => days_from(today, -31),
    };
    let to = match args.value("to")? {
        Some(to) => parse_date(to)?,
        None => days_from(today, 31),
    };
    let occurrences = book.planned_occurrences(from, to, today)?;
    if occurrences.is_empty() {
        writeln!(out, "No payments are planned from {from} to {to}.")?;
        return Ok(());
    }
    let mut rows = vec![vec![
        "Due".to_owned(),
        "Counted".to_owned(),
        "Plan".to_owned(),
        "Amount".to_owned(),
        "State".to_owned(),
        "Transaction".to_owned(),
        "Plan ID".to_owned(),
    ]];
    for occurrence in &occurrences {
        let plan = book.schedule(occurrence.schedule)?;
        rows.push(vec![
            occurrence.due.date.to_string(),
            occurrence.due.counted.to_string(),
            plain(&plan.name),
            money_text(Some(book), plan.amount),
            state_text(occurrence.state).to_owned(),
            occurrence.transaction.map(short_id).unwrap_or_default(),
            short_id(plan.id),
        ]);
    }
    write!(out, "{}", table(&rows, &[3]))?;
    writeln!(
        out,
        "`schedule match`, `skip`, `unskip` and `book` take the plan's ID and the \
         due date."
    )?;
    Ok(())
}

/// The state as the user reads it.
fn state_text(state: OccurrenceState) -> &'static str {
    match state {
        OccurrenceState::Matched => "paid",
        OccurrenceState::Skipped => "skipped",
        OccurrenceState::Upcoming => "upcoming",
        OccurrenceState::Due => "due",
        OccurrenceState::Overdue => "overdue",
    }
}

/// `date` moved by `days`, or `date` itself at the end of the calendar.
fn days_from(date: Date, days: i32) -> Date {
    let mut moved = date;
    for _ in 0..days.unsigned_abs() {
        let next = if days < 0 {
            moved.yesterday()
        } else {
            moved.tomorrow()
        };
        match next {
            Ok(next) => moved = next,
            Err(_) => break,
        }
    }
    moved
}

/// `schedule book ID DUE [--date DATE] [--amount AMOUNT]`.
fn book_occurrence(book: &mut Book, args: &[String], out: &mut dyn Write) -> Result<(), CliError> {
    let args = Arguments::parse(args, &["date", "amount"], &[])?;
    let [id, due_on] = args.exactly(["id", "due date"])?;
    let id = find_schedule(book, id)?;
    let due_on = counted_date(book, id, parse_date(due_on)?)?;
    let plan = book.schedule(id)?;
    let account = book.account(plan.account)?;
    let date = match args.value("date")? {
        Some(date) => parse_date(date)?,
        None => today(),
    };
    let amount = match args.value("amount")? {
        Some(amount) => amount_in(book, &account, amount)?,
        None => plan.amount,
    };
    let transaction = book.book_schedule_occurrence(id, due_on, date, amount)?;
    writeln!(
        out,
        "Booked {} on {date} in {} as the payment due {due_on}; `undo` takes it back. \
         The transaction is {}.",
        money_text(Some(book), amount),
        plain(&account.name),
        short_id(transaction)
    )?;
    Ok(())
}

/// The counted date of `id`'s payment due on `date`: `date` itself if a
/// payment is counted on it, else the counted date of the one moved to
/// `date` from a weekend, else `date`, which the book then refuses as no
/// payment of the plan.
fn counted_date(book: &Book, id: ScheduleId, date: Date) -> Result<Date, CliError> {
    let plan = book.schedule(id)?;
    let dues = kassensturz_core::due_dates(&plan, days_from(date, -3), days_from(date, 3));
    if dues.iter().any(|due| due.counted == date) {
        return Ok(date);
    }
    Ok(dues
        .iter()
        .find(|due| due.date == date)
        .map_or(date, |due| due.counted))
}

/// The plan whose ID ends with `text`, as `schedule list` shows it.
fn find_schedule(book: &Book, text: &str) -> Result<ScheduleId, CliError> {
    let wanted = text.trim().to_lowercase();
    let mut found = book
        .schedules()?
        .into_iter()
        .filter(|plan| wanted.len() >= 6 && plan.id.to_string().ends_with(&wanted));
    match (found.next(), found.next()) {
        (Some(plan), None) => Ok(plan.id),
        (None, _) => Err(CliError::message(format!(
            "`{}` isn't a plan's ID. `schedule list` shows them",
            plain(text)
        ))),
        (Some(_), Some(_)) => Err(CliError::message(format!(
            "more than one plan's ID ends with `{}`; type more of it",
            plain(text)
        ))),
    }
}

/// `forecast [--months N] [--account NAME ...]`: what is left this month
/// after the planned payments, and the months after (D-770). Without
/// `--account`, the open checking and cash accounts.
pub(crate) fn forecast(book: &Book, args: &[String], out: &mut dyn Write) -> Result<(), CliError> {
    let args = Arguments::parse(args, &["months", "account"], &[])?;
    args.exactly([])?;
    let months = match args.value("months")? {
        Some(months) => months
            .parse::<usize>()
            .ok()
            .filter(|months| *months <= kassensturz_core::MAX_FORECAST_MONTHS)
            .ok_or_else(|| {
                CliError::usage(format!(
                    "--months takes a number from 0 to {}, not `{}`",
                    kassensturz_core::MAX_FORECAST_MONTHS,
                    plain(months)
                ))
            })?,
        None => 3,
    };
    let named = args.all("account");
    let accounts: Vec<AccountId> = if named.is_empty() {
        book.default_forecast_accounts()?
    } else {
        let mut accounts = Vec::new();
        for name in named {
            accounts.push(find_account(book, name)?.id);
        }
        accounts
    };
    let Some(first) = accounts.first() else {
        writeln!(
            out,
            "There is no open checking or cash account to look ahead for; name one with \
             --account."
        )?;
        return Ok(());
    };
    let currency = book.account(*first)?.currency;
    let today = today();
    let found = book.forecast(&accounts, currency, today, months)?;
    let forecast = &found.forecast;
    let mut names = Vec::new();
    for account in &found.accounts {
        names.push(plain(&book.account(*account)?.name));
    }
    writeln!(out, "Looking ahead for {} on {today}.", names.join(", "))?;
    let rows = vec![
        vec![
            "Balance now".to_owned(),
            money_text(Some(book), forecast.balance_now),
        ],
        vec![
            "Still to pay this month".to_owned(),
            money_text(Some(book), forecast.out_due),
        ],
        vec![
            "Left after that".to_owned(),
            money_text(Some(book), forecast.left),
        ],
        vec![
            "Still coming in this month".to_owned(),
            money_text(Some(book), forecast.in_due),
        ],
        vec![
            "Expected at the end of the month".to_owned(),
            money_text(Some(book), forecast.expected_end),
        ],
    ];
    write!(out, "{}", table(&rows, &[1]))?;
    if !forecast.overdue.is_empty() {
        writeln!(
            out,
            "Overdue, and not counted above until you match, skip or book them:"
        )?;
        for overdue in &forecast.overdue {
            let plan = book.schedule(overdue.schedule)?;
            writeln!(
                out,
                "  {}  {}  {}  plan {}",
                overdue.due.counted,
                plain(&plan.name),
                money_text(Some(book), plan.amount),
                short_id(plan.id)
            )?;
        }
    }
    if !forecast.months.is_empty() {
        let mut rows = vec![vec![
            "Month".to_owned(),
            "Start".to_owned(),
            "In".to_owned(),
            "Out".to_owned(),
            "End".to_owned(),
        ]];
        for month in &forecast.months {
            rows.push(vec![
                month.month.to_string()[..7].to_owned(),
                money_text(Some(book), month.start),
                money_text(Some(book), month.planned_in),
                money_text(Some(book), month.planned_out),
                money_text(Some(book), month.end),
            ]);
        }
        write!(out, "{}", table(&rows, &[1, 2, 3, 4]))?;
    }
    if !found.left_out.is_empty() {
        let mut left_out = Vec::new();
        for account in &found.left_out {
            left_out.push(plain(&book.account(*account)?.name));
        }
        writeln!(
            out,
            "Left out, in another currency: {}.",
            left_out.join(", ")
        )?;
    }
    if !found.has_plans {
        writeln!(
            out,
            "No payments are planned yet, so this is the balance alone; \
             `schedule add` plans one."
        )?;
    }
    writeln!(
        out,
        "Only planned payments are counted; the figures are a guide, not a promise."
    )?;
    Ok(())
}
