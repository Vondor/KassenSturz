// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The commands for Transfers between the user's own accounts: the
//! suggested ones, joining two transactions into one (D-021, D-087,
//! D-691), and the monthly moves joined by themselves (D-749).

use std::io::Write;

use kassensturz_core::{Date, RecurringMove, RecurringMoveId, Timestamp};
use kassensturz_store::{Book, NewRecurringMove};

use crate::amount::{money_text, parse_amount};
use crate::args::Arguments;
use crate::error::CliError;
use crate::lookup::{find_account, find_transaction, short_id};
use crate::text::{plain, table};

/// `transfer suggest|join|monthly`.
pub(crate) fn transfer(
    book: &mut Book,
    args: &[String],
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return Err(CliError::usage(
            "transfer needs a command: suggest, join or monthly",
        ));
    };
    match command.as_str() {
        "suggest" => {
            Arguments::parse(rest, &[], &[])?.exactly([])?;
            suggest(book, out)
        }
        "join" => {
            let args = Arguments::parse(rest, &[], &[])?;
            let [first, second] = args.exactly(["id", "other id"])?;
            let first = find_transaction(book, first)?;
            let second = find_transaction(book, second)?;
            let joined = book.join_transfer(first, second)?;
            writeln!(out, "Joined them into the Transfer {}.", short_id(joined))?;
            Ok(())
        }
        "monthly" => monthly(book, rest, out),
        other => Err(CliError::usage(format!(
            "`transfer {other}` isn't a command"
        ))),
    }
}

/// `transfer suggest`: the pairs of transactions that look like the two
/// sides of one Transfer, with the category a Rule put a side in, which
/// joining takes away (D-691). Nothing is changed; `transfer join` joins a
/// pair, or any two sides the user picks.
fn suggest(book: &Book, out: &mut dyn Write) -> Result<(), CliError> {
    let suggestions = book.transfer_suggestions()?;
    if suggestions.is_empty() {
        writeln!(
            out,
            "No transactions look like the two sides of a Transfer."
        )?;
        return Ok(());
    }
    let mut rows = vec![vec![
        "Out".to_owned(),
        "From".to_owned(),
        "In".to_owned(),
        "To".to_owned(),
        "Amount".to_owned(),
        "IDs".to_owned(),
        "Sorted by a Rule".to_owned(),
    ]];
    for found in suggestions {
        let (outgoing, incoming) = (found.outgoing, found.incoming);
        let mut sorted = Vec::new();
        for category in [outgoing.category, incoming.category].into_iter().flatten() {
            sorted.push(plain(&book.account(category)?.name));
        }
        rows.push(vec![
            outgoing.booking_date.to_string(),
            plain(&book.account(outgoing.account)?.name),
            incoming.booking_date.to_string(),
            plain(&book.account(incoming.account)?.name),
            money_text(Some(book), incoming.amount),
            format!(
                "{} {}",
                short_id(outgoing.transaction),
                short_id(incoming.transaction)
            ),
            sorted.join(", "),
        ]);
    }
    write!(out, "{}", table(&rows, &[4]))?;
    writeln!(
        out,
        "Check each one: `transfer join ID ID` joins the two into one Transfer; \
         a category a Rule chose goes."
    )?;
    Ok(())
}

/// `transfer monthly list|add|remove`: the monthly moves between the user's
/// accounts that imports join by themselves (D-749).
fn monthly(book: &mut Book, args: &[String], out: &mut dyn Write) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return Err(CliError::usage(
            "transfer monthly needs a command: list, add or remove",
        ));
    };
    match command.as_str() {
        "list" => {
            Arguments::parse(rest, &[], &[])?.exactly([])?;
            list_monthly(book, out)
        }
        "add" => {
            let args = Arguments::parse(rest, &["iban", "name"], &[])?;
            let [from, to, amount, day] = args.exactly(["from", "to", "amount", "day"])?;
            let from = find_account(book, from)?;
            let to = find_account(book, to)?;
            let digits = book.currency_minor_digits(from.currency)?;
            let amount = parse_amount(amount, from.currency, digits)?;
            let day_of_month = day
                .parse::<u8>()
                .ok()
                .filter(|day| (1..=31).contains(day))
                .ok_or_else(|| {
                    CliError::usage(format!(
                        "the day is a day of the month from 1 to 31, not `{}`",
                        plain(day)
                    ))
                })?;
            let id = book.add_recurring_move(&NewRecurringMove {
                from_account: from.id,
                to_account: to.id,
                amount,
                day_of_month,
                from_party_iban: args.value("iban")?.map(str::to_owned),
                from_party_name: args.value("name")?.map(str::to_owned),
                created_on: today(),
            })?;
            writeln!(
                out,
                "Imports now join {} from {} to {} around the {} of each month by \
                 themselves; `undo` takes this back. The move is {}.",
                money_text(Some(book), amount),
                plain(&from.name),
                plain(&to.name),
                day_of_month,
                short_id(id)
            )?;
            Ok(())
        }
        "remove" => {
            let args = Arguments::parse(rest, &[], &[])?;
            let [id] = args.exactly(["id"])?;
            let id = find_move(book, id)?;
            book.remove_recurring_move(id)?;
            writeln!(
                out,
                "Removed the move. Moves already joined stay joined; from now on each one \
                 is suggested again."
            )?;
            Ok(())
        }
        other => Err(CliError::usage(format!(
            "`transfer monthly {other}` isn't a command"
        ))),
    }
}

/// `transfer monthly list`.
fn list_monthly(book: &Book, out: &mut dyn Write) -> Result<(), CliError> {
    let moves = book.recurring_moves()?;
    if moves.is_empty() {
        writeln!(out, "No moves are joined by themselves.")?;
        return Ok(());
    }
    let mut rows = vec![vec![
        "From".to_owned(),
        "To".to_owned(),
        "Amount".to_owned(),
        "Day".to_owned(),
        "Other party".to_owned(),
        "Since".to_owned(),
        "ID".to_owned(),
    ]];
    for moved in &moves {
        rows.push(vec![
            plain(&book.account(moved.from_account)?.name),
            plain(&book.account(moved.to_account)?.name),
            money_text(Some(book), moved.amount),
            moved.day_of_month.to_string(),
            party_text(moved),
            moved.created_on.to_string(),
            short_id(moved.id),
        ]);
    }
    write!(out, "{}", table(&rows, &[2, 3]))?;
    Ok(())
}

/// The other party a move remembers, as the bank delivered it.
fn party_text(moved: &RecurringMove) -> String {
    [
        moved.from_party_name.as_deref(),
        moved.from_party_iban.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(plain)
    .collect::<Vec<_>>()
    .join(", ")
}

/// The move whose ID ends with `text`, as `transfer monthly list` shows it.
fn find_move(book: &Book, text: &str) -> Result<RecurringMoveId, CliError> {
    let wanted = text.trim().to_lowercase();
    let mut found = book
        .recurring_moves()?
        .into_iter()
        .filter(|moved| wanted.len() >= 6 && moved.id.to_string().ends_with(&wanted));
    match (found.next(), found.next()) {
        (Some(moved), None) => Ok(moved.id),
        (None, _) => Err(CliError::message(format!(
            "`{}` isn't a move's ID. `transfer monthly list` shows them",
            plain(text)
        ))),
        (Some(_), Some(_)) => Err(CliError::message(format!(
            "more than one move's ID ends with `{}`; type more of it",
            plain(text)
        ))),
    }
}

/// Today, in UTC, the day a move is remembered on.
pub(crate) fn today() -> Date {
    Timestamp::now().to_string()[..10]
        .parse()
        .unwrap_or(Date::constant(2026, 1, 1))
}
