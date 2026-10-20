// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why statements can't be imported into an account.

use kassensturz_core::{ErrorReport, Report};
use kassensturz_import::ImportError;
use kassensturz_store::StoreError;

/// Why statements can't be imported into an account. Nothing was imported:
/// the book is as it was.
///
/// Its report is that of the error inside, an `import.` or a `store.` code
/// (`docs/ERROR_CODES.md`), so the pipeline adds no codes of its own.
#[derive(Debug)]
pub enum PipelineError {
    /// The statements don't fit the account, e.g. because they are in
    /// another currency ([`ImportError::AccountCurrencyDiffers`]).
    Import(ImportError),
    /// The book refused, e.g. because the account is closed or is a
    /// category, or couldn't be read or written.
    Store(StoreError),
}

impl From<ImportError> for PipelineError {
    fn from(error: ImportError) -> Self {
        Self::Import(error)
    }
}

impl From<StoreError> for PipelineError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl Report for PipelineError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::Import(error) => error.report(),
            Self::Store(error) => error.report(),
        }
    }
}
