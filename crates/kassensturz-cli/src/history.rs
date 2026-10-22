// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The commands for the change history: undo, redo, the list of changes and
//! how long the book keeps them (D-066, D-086).

use std::io::Write;

use kassensturz_store::{Action, Book, Change, HistoryRetention};

use crate::args::Arguments;
use crate::error::CliError;
use crate::text::{counted, plain, table};

/// `undo`: undoes the newest change that isn't undone yet, also one made by
/// an earlier command.
pub(crate) fn undo(book: &mut Book, out: &mut dyn Write) -> Result<(), CliError> {
    match book.undo()? {
        Some(change) => writeln!(out, "Undid the change: {}.", change_text(book, &change)?)?,
        None => writeln!(out, "There is nothing to undo.")?,
    }
    Ok(())
}

/// `redo`: makes the change undone last again.
pub(crate) fn redo(book: &mut Book, out: &mut dyn Write) -> Result<(), CliError> {
    match book.redo()? {
        Some(change) => writeln!(out, "Redid the change: {}.", change_text(book, &change)?)?,
        None => writeln!(out, "There is nothing to redo.")?,
    }
    Ok(())
}

/// `history list|keep|clean-up`.
pub(crate) fn history(
    book: &mut Book,
    args: &[String],
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return Err(CliError::usage(
            "history needs a command: list, keep or clean-up",
        ));
    };
    match command.as_str() {
        "list" => {
            Arguments::parse(rest, &[], &[])?.exactly([])?;
            let mut rows = vec![vec![
                "Made at (UTC)".to_owned(),
                "Change".to_owned(),
                "State".to_owned(),
            ]];
            for change in book.history()? {
                rows.push(vec![
                    change.made_at.to_string(),
                    change_text(book, &change)?,
                    if change.undone { "undone" } else { "done" }.to_owned(),
                ]);
            }
            write!(out, "{}", table(&rows, &[]))?;
            writeln!(out, "{}", retention_text(book.history_retention()?))?;
        }
        "keep" => {
            let args = Arguments::parse(rest, &[], &[])?;
            let [days] = args.exactly(["days or forever"])?;
            let keep = if days.eq_ignore_ascii_case("forever") {
                HistoryRetention::Forever
            } else {
                let days = days.parse().map_err(|_| {
                    CliError::message(format!(
                        "`{days}` isn't a number of days from 0 to {}, or forever",
                        u32::MAX
                    ))
                })?;
                HistoryRetention::Days(days)
            };
            book.set_history_retention(keep)?;
            writeln!(out, "{}", retention_text(keep))?;
        }
        "clean-up" => {
            Arguments::parse(rest, &[], &[])?.exactly([])?;
            book.clean_up_history()?;
            writeln!(
                out,
                "Deleted the whole history: nothing done so far can be undone."
            )?;
        }
        other => {
            return Err(CliError::usage(format!(
                "`history {other}` isn't a command"
            )));
        }
    }
    Ok(())
}

fn retention_text(keep: HistoryRetention) -> String {
    match keep {
        HistoryRetention::Days(0) => "The book keeps no history: a change can be undone only \
                                      until the book is closed, so not by a later command."
            .to_owned(),
        HistoryRetention::Days(1) => "The book keeps its history for 1 day.".to_owned(),
        HistoryRetention::Days(days) => format!("The book keeps its history for {days} days."),
        HistoryRetention::Forever => "The book keeps its history forever.".to_owned(),
    }
}

/// What the user did in `change`, the file it read and which transactions
/// it touched, so the user can tell e.g. two imports apart (#51, D-109):
/// `imported a statement from october.xml (2 transactions added in Checking
/// account, dated 2026-10-05 to 2026-10-14)`.
pub(crate) fn change_text(book: &Book, change: &Change) -> Result<String, CliError> {
    let action = match &change.file_name {
        Some(file) => format!("{} from {}", action_text(change.action), plain(file)),
        None => action_text(change.action).to_owned(),
    };
    let summary = book.change_summary(change.id)?;
    let mut counts = Vec::new();
    for (count, done) in [
        (summary.added, "added"),
        (summary.changed, "changed"),
        (summary.removed, "removed"),
    ] {
        if count > 0 {
            // Only the first count names the transactions.
            counts.push(if counts.is_empty() {
                format!("{} {done}", counted(count, "transaction", "transactions"))
            } else {
                format!("{count} {done}")
            });
        }
    }
    if counts.is_empty() {
        return Ok(action);
    }
    let mut details = counts.join(", ");
    // The user's accounts, not the categories: they say where it happened.
    let accounts: Vec<String> = summary
        .accounts
        .iter()
        .filter_map(|&id| book.account(id).ok())
        .filter(|account| account.kind.is_account())
        .map(|account| plain(&account.name))
        .collect();
    if let Some((last, rest)) = accounts.split_last() {
        details.push_str(" in ");
        if !rest.is_empty() {
            details.push_str(&rest.join(", "));
            details.push_str(" and ");
        }
        details.push_str(last);
    }
    match summary.dates {
        Some((first, last)) if first == last => details.push_str(&format!(", dated {first}")),
        Some((first, last)) => details.push_str(&format!(", dated {first} to {last}")),
        None => {}
    }
    Ok(format!("{action} ({details})"))
}

fn action_text(action: Action) -> &'static str {
    match action {
        Action::SetRoundingMode => "changed the rounding",
        Action::UseCurrency => "started using a currency",
        Action::AddAccount => "added an account",
        Action::RenameAccount => "renamed an account",
        Action::CloseAccount => "closed an account",
        Action::ReopenAccount => "reopened an account",
        Action::AddCategory => "added a category",
        Action::RenameCategory => "renamed a category",
        Action::MoveCategory => "moved a category",
        Action::AddRate => "added an exchange rate",
        Action::RemoveRate => "removed an exchange rate",
        Action::AddTransaction => "added a transaction",
        Action::EditTransaction => "changed a transaction",
        Action::RemoveTransaction => "removed a transaction",
        Action::JoinTransfer => "joined two transactions into a Transfer",
        Action::AddRule => "added a Rule",
        Action::EditRule => "changed a Rule",
        Action::MoveRule => "moved a Rule",
        Action::RemoveRule => "removed a Rule",
        Action::RunRules => "ran the Rules",
        Action::ImportStatement => "imported a statement",
        Action::CorrectStartingBalance => "corrected a starting balance",
        Action::AddInstitution => "added a bank",
        Action::AddPayee => "added a payee",
        Action::AddTag => "added a tag",
    }
}
