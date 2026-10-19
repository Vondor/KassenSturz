// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The commands for Transfers between the user's own accounts: the
//! suggested ones, and joining two transactions into one (D-021, D-087).

use std::io::Write;

use kassensturz_store::Book;

use crate::amount::money_text;
use crate::args::Arguments;
use crate::error::CliError;
use crate::lookup::{find_transaction, short_id};
use crate::text::{plain, table};

/// `transfer suggest|join`.
pub(crate) fn transfer(
    book: &mut Book,
    args: &[String],
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return Err(CliError::usage("transfer needs a command: suggest or join"));
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
        other => Err(CliError::usage(format!(
            "`transfer {other}` isn't a command"
        ))),
    }
}

/// `transfer suggest`: the pairs of transactions that look like the two
/// sides of one Transfer. Nothing is changed; `transfer join` joins a pair.
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
    ]];
    for found in suggestions {
        let (outgoing, incoming) = (found.outgoing, found.incoming);
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
        ]);
    }
    write!(out, "{}", table(&rows, &[4]))?;
    writeln!(
        out,
        "Check each one: `transfer join ID ID` joins the two into one Transfer."
    )?;
    Ok(())
}
