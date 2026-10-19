// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Checks what an importer returns, so the import pipeline can rely on it
//! whichever importer read the file, a plugin's included later (D-091).

use kassensturz_core::Money;

use crate::error::ImportError;
use crate::importer::Imported;
use crate::limits::Limits;
use crate::statement::Statement;

/// Checks that `imported` keeps `limits` and that every amount of a
/// statement is in the statement's currency.
pub(crate) fn check(imported: &Imported, limits: &Limits) -> Result<(), ImportError> {
    match imported {
        Imported::Statements(statements) => {
            let entries = statements
                .iter()
                .map(|statement| statement.entries.len())
                .fold(0_usize, usize::saturating_add);
            limits.check_entries(entries)?;
            statements
                .iter()
                .try_for_each(|statement| check_statement(statement, limits))
        }
    }
}

fn check_statement(statement: &Statement, limits: &Limits) -> Result<(), ImportError> {
    let expected = statement.currency.code();
    let in_currency = |amount: Money| {
        if amount.currency() == expected {
            Ok(())
        } else {
            Err(ImportError::CurrencyMismatch {
                expected,
                found: amount.currency(),
            })
        }
    };
    let account = &statement.account;
    for text in [&account.iban, &account.number].into_iter().flatten() {
        limits.check_text(text, None)?;
    }
    for balance in [statement.opening_balance, statement.closing_balance]
        .into_iter()
        .flatten()
    {
        in_currency(balance.amount)?;
    }
    for entry in &statement.entries {
        in_currency(entry.amount)?;
        for text in entry.texts() {
            limits.check_text(text, None)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
