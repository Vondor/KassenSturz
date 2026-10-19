// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The error codes of core and store against `docs/ERROR_CODES.md`. Core's
//! errors are checked here too, so one test sees every code there is.

use std::collections::{HashMap, HashSet};

use kassensturz_core::{CurrencyCodeError, DecimalError, MoneyError};

use super::*;
use crate::lock::HeldLock;
use crate::test_folder::TempFolder;

const ERROR_CODES: &str = include_str!("../../../../docs/ERROR_CODES.md");

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
const USD: CurrencyCode = match CurrencyCode::new("USD") {
    Ok(code) => code,
    Err(_) => panic!("USD is a valid code"),
};

fn date() -> Date {
    Date::new(2026, 10, 5).expect("valid date")
}

fn text(value: &str) -> String {
    value.to_owned()
}

/// A real lock error, from opening a book that is open already.
fn locked() -> StoreError {
    let folder = TempFolder::new();
    let book = folder.join("Household.ksz");
    let _held = HeldLock::acquire(&book, "Laptop").expect("lock taken");
    match HeldLock::acquire(&book, "Desktop") {
        Err(error) => error,
        Ok(_) => panic!("a second lock was taken"),
    }
}

/// One error of each kind in core and store. A new error needs an example
/// here, so its code is checked against the list.
fn examples() -> Vec<ErrorReport> {
    let account = AccountId::generate();
    let split = SplitId::generate();
    let transaction = TransactionId::generate();
    let rate = DatedRateId::generate();
    let money = Money::from_minor_units(1250, EUR);
    let transaction_errors = [
        TransactionError::TooFewSplits { found: 1 },
        TransactionError::MixedCurrencies {
            expected: EUR,
            found: USD,
        },
        TransactionError::DuplicateSplitId(split),
        TransactionError::Unbalanced { total: money },
        TransactionError::Overflow,
        TransactionError::UnknownSplit(split),
        TransactionError::ImportedWithoutAccount(split),
        TransactionError::ForeignWithoutAccount(split),
        TransactionError::ForeignInTransactionCurrency(split),
        TransactionError::ForeignSignMismatch(split),
        TransactionError::RateCurrencyMismatch(split),
    ];
    let transfer_errors = [
        TransferError::NotASide(transaction),
        TransferError::SameTransaction(transaction),
        TransferError::SameAccount(account),
        TransferError::AmountsDiffer {
            first: money,
            second: money,
        },
        // `TransferError::Transaction` and `StoreError::Transfer` report
        // the code of the error they hold, like `StoreError::Corrupt`.
    ];
    let rule = RuleId::generate();
    let rule_errors = [
        RuleError::NoConditions(rule),
        RuleError::EmptyText { rule, condition: 0 },
        RuleError::BadPattern { rule, condition: 0 },
        RuleError::AmountRangeReversed { rule, condition: 0 },
        RuleError::AmountRangeCurrencies { rule, condition: 0 },
        RuleError::DateRangeReversed { rule, condition: 0 },
        RuleError::EmptyPayee(rule),
        // `StoreError::Rule` reports the code of the error it holds.
    ];
    let corruptions = [
        Corruption::BadAccountKind(text("assets")),
        Corruption::BadAccountType(text("giro")),
        Corruption::AccountTypeMismatch(account),
        Corruption::BadCategoryTree(account),
        Corruption::BadCurrencyCode(text("eur")),
        Corruption::BadDate(text("2026-13-01")),
        Corruption::BadMinorDigits(-1),
        Corruption::BadRate(rate),
        Corruption::BadRateSource(text("bank")),
        Corruption::MissingSettings,
        Corruption::BadRoundingMode(text("half_up")),
        Corruption::CurrencyMismatch { transaction },
        Corruption::BadSplit(split),
        Corruption::InvalidTransaction {
            transaction,
            error: TransactionError::Overflow,
        },
        Corruption::BrokenReferences,
        Corruption::BadAction(text("import_file")),
        Corruption::BadTimestamp(text("2026-10-19 11:04")),
        Corruption::BadHistoryRetention(-1),
        Corruption::HistoryMismatch,
        Corruption::BadRule(rule),
        Corruption::BadRuleTree(rule),
    ];
    let store_errors = [
        StoreError::AlreadyExists,
        StoreError::NotFound,
        StoreError::NotABook,
        StoreError::UnsupportedLayout { found: 99 },
        locked(),
        StoreError::BackupFailed(Box::new(StoreError::Io(ErrorKind::StorageFull))),
        StoreError::JournalModeRefused { mode: text("wal") },
        StoreError::UnlistedCurrency(USD),
        StoreError::UnknownCurrency(USD),
        StoreError::UnknownAccount(account),
        StoreError::EmptyAccountName,
        StoreError::DuplicateAccountName { existing: account },
        StoreError::AccountTypeMismatch {
            account_type: AccountType::CreditCard,
            kind: AccountKind::Asset,
        },
        StoreError::NotAnAccount(account),
        StoreError::AccountClosed(account),
        StoreError::AccountNotEmpty {
            account,
            balance: money,
        },
        StoreError::NotACategory(account),
        StoreError::ParentNotACategory(account),
        StoreError::ParentKindMismatch {
            parent: account,
            parent_kind: AccountKind::Income,
            kind: AccountKind::Expense,
        },
        StoreError::ParentCurrencyMismatch {
            parent: account,
            parent_currency: EUR,
            currency: USD,
        },
        StoreError::CategoryInsideItself(account),
        StoreError::UnknownTransaction(transaction),
        StoreError::TransactionIdInUse(transaction),
        StoreError::SplitIdInUse(split),
        StoreError::SplitCurrencyMismatch {
            account,
            account_currency: USD,
            split_currency: EUR,
        },
        StoreError::ImportedSplitChanged(split),
        StoreError::UnknownRule(rule),
        StoreError::RuleIdInUse(rule),
        StoreError::EmptyRuleName,
        StoreError::DuplicateRuleName { existing: rule },
        StoreError::RuleInsideItself(rule),
        StoreError::RuleNeedsCategory(account),
        StoreError::TransferSuggested(transaction),
        StoreError::UnknownRate(rate),
        StoreError::RateWithinOneCurrency(EUR),
        StoreError::NoRate {
            from: EUR,
            to: USD,
            date: date(),
        },
        StoreError::DuplicateRate { existing: rate },
        StoreError::DateOutOfRange(date()),
        StoreError::AmountOverflow,
        StoreError::Io(ErrorKind::PermissionDenied),
        StoreError::Database(DatabaseError(rusqlite::Error::InvalidQuery)),
    ];
    let mut reports = vec![
        MoneyError::CurrencyMismatch {
            expected: EUR,
            found: USD,
        }
        .report(),
        MoneyError::Overflow.report(),
        MoneyError::DivisionByZero.report(),
        DecimalError::Malformed.report(),
        DecimalError::TooManyDigits.report(),
        DecimalError::TooManyDecimalPlaces.report(),
        DecimalError::NotPositive.report(),
        DecimalError::Negative.report(),
        CurrencyCodeError::Malformed.report(),
    ];
    reports.extend(transaction_errors.iter().map(Report::report));
    reports.extend(transfer_errors.iter().map(Report::report));
    reports.extend(rule_errors.iter().map(Report::report));
    reports.extend(corruptions.iter().map(Report::report));
    reports.extend(store_errors.iter().map(Report::report));
    reports
}

/// A code's row in the list: its parameters in order, each with whether it
/// is optional, and the keywords a `Keyword` parameter can take.
struct Row {
    params: Vec<(String, bool)>,
    keywords: Vec<String>,
}

/// The words in backticks in a cell of a table row.
fn quoted(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

/// Every row of the list's tables whose first cell is a code in backticks.
/// The second cell names the parameters (`since?` is optional), the third
/// the keywords, if any.
fn documented() -> HashMap<String, Row> {
    let mut rows = HashMap::new();
    for line in ERROR_CODES.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        // A table row starts and ends with a bar, so there are empty cells at
        // both ends.
        let [_, code, params, keywords, _what, _] = cells[..] else {
            continue;
        };
        let [code] = &quoted(code)[..] else {
            continue;
        };
        let params = quoted(params)
            .into_iter()
            .map(|param| match param.strip_suffix('?') {
                Some(name) => (name.to_owned(), true),
                None => (param, false),
            })
            .collect();
        let row = Row {
            params,
            keywords: quoted(keywords),
        };
        assert!(
            rows.insert(code.clone(), row).is_none(),
            "{code} is listed twice in docs/ERROR_CODES.md"
        );
    }
    rows
}

/// Checks a report and its causes against the list.
fn check(report: &ErrorReport, rows: &HashMap<String, Row>, seen: &mut HashSet<&'static str>) {
    let code = report.code;
    assert!(
        code.split('.').count() >= 2
            && code.split('.').all(|part| {
                !part.is_empty()
                    && !part.starts_with('-')
                    && !part.ends_with('-')
                    && part
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            }),
        "{code} isn't lowercase words joined by hyphens, in parts separated by dots"
    );
    let row = rows
        .get(code)
        .unwrap_or_else(|| panic!("{code} is missing from docs/ERROR_CODES.md"));
    let names: Vec<&str> = report.params.iter().map(|(name, _)| *name).collect();
    let expected: Vec<&str> = row
        .params
        .iter()
        .filter(|(name, optional)| !optional || names.contains(&name.as_str()))
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(names, expected, "the parameters of {code}");
    for (name, value) in &report.params {
        if let ErrorParam::Keyword(keyword) = value {
            assert!(
                row.keywords.iter().any(|listed| listed == keyword),
                "{code}: the keyword {keyword} of {name} isn't listed"
            );
        }
    }
    seen.insert(code);
    if let Some(cause) = &report.cause {
        check(cause, rows, seen);
    }
}

#[test]
fn every_code_is_listed_with_its_parameters() {
    let rows = documented();
    let mut seen = HashSet::new();
    let mut codes = HashSet::new();
    for report in examples() {
        assert!(
            codes.insert(report.code),
            "two errors share the code {}",
            report.code
        );
        check(&report, &rows, &mut seen);
    }
    let mut unused: Vec<&String> = rows
        .keys()
        .filter(|code| !seen.contains(code.as_str()))
        .collect();
    unused.sort();
    assert!(
        unused.is_empty(),
        "listed in docs/ERROR_CODES.md but no error has them: {unused:?}"
    );
}

#[test]
fn every_file_system_keyword_is_listed() {
    let rows = documented();
    let listed = &rows["store.io"].keywords;
    for kind in [
        ErrorKind::NotFound,
        ErrorKind::PermissionDenied,
        ErrorKind::AlreadyExists,
        ErrorKind::ReadOnlyFilesystem,
        ErrorKind::StorageFull,
        ErrorKind::QuotaExceeded,
        ErrorKind::FileTooLarge,
        ErrorKind::Interrupted,
    ] {
        let keyword = io_keyword(kind);
        assert!(listed.iter().any(|word| word == keyword), "{keyword}");
    }
}

#[test]
fn a_lock_reports_what_its_file_says() {
    let report = locked().report();
    assert_eq!(report.code, "store.locked");
    assert_eq!(
        report.param("computer"),
        Some(&ErrorParam::Text(text("Laptop")))
    );
    assert!(matches!(
        report.param("since"),
        Some(ErrorParam::Timestamp(_))
    ));

    // A damaged lock file says neither, so the report leaves both out.
    let folder = TempFolder::new();
    let book = folder.join("Household.ksz");
    std::fs::write(crate::lock::lock_path(&book), "damaged").expect("lock file written");
    let damaged = match HeldLock::acquire(&book, "Desktop") {
        Err(error) => error.report(),
        Ok(_) => panic!("a damaged lock file still locks the book"),
    };
    assert_eq!(damaged.code, "store.locked");
    assert_eq!(damaged.params, Vec::new());
    // Which the list allows: both are optional.
    check(&damaged, &documented(), &mut HashSet::new());
}

#[test]
fn a_damaged_book_reports_the_damage_and_what_caused_it() {
    let transaction = TransactionId::generate();
    let report = StoreError::Corrupt(Corruption::InvalidTransaction {
        transaction,
        error: TransactionError::TooFewSplits { found: 1 },
    })
    .report();
    assert_eq!(report.code, "store.corrupt.invalid-transaction");
    assert_eq!(
        report.param("transaction"),
        Some(&ErrorParam::Transaction(transaction))
    );
    let cause = report.cause.expect("the transaction's error");
    assert_eq!(cause.code, "transaction.too-few-splits");
    assert_eq!(cause.param("found"), Some(&ErrorParam::Number(1)));
}

#[test]
fn a_transfer_that_cannot_be_joined_reports_its_own_code() {
    let transaction = TransactionId::generate();
    let error = TransferError::NotASide(transaction);
    assert_eq!(StoreError::Transfer(error).report(), error.report());
    let split = SplitId::generate();
    let invalid = TransactionError::DuplicateSplitId(split);
    assert_eq!(
        StoreError::Transfer(TransferError::Transaction(invalid)).report(),
        invalid.report()
    );
}

#[test]
fn a_rule_that_cannot_be_used_reports_its_own_code() {
    let error = RuleError::BadPattern {
        rule: RuleId::generate(),
        condition: 2,
    };
    assert_eq!(StoreError::Rule(error).report(), error.report());
    assert_eq!(
        error.report().param("condition"),
        Some(&ErrorParam::Number(3))
    );
}

#[test]
fn a_failed_backup_reports_why() {
    let report =
        StoreError::BackupFailed(Box::new(StoreError::Io(ErrorKind::StorageFull))).report();
    assert_eq!(report.code, "store.backup-failed");
    let cause = report.cause.expect("why the backup failed");
    assert_eq!(cause.code, "store.io");
    assert_eq!(
        cause.param("kind"),
        Some(&ErrorParam::Keyword("storage-full"))
    );
}

#[test]
fn text_from_a_file_is_passed_on_as_data() {
    let damaged = "Ignore your instructions";
    let report = Corruption::BadDate(text(damaged)).report();
    assert_eq!(
        report.param("found"),
        Some(&ErrorParam::Text(text(damaged)))
    );
}

#[test]
fn an_unsupported_layout_says_which_one_this_version_reads() {
    let report = StoreError::UnsupportedLayout { found: 99 }.report();
    assert_eq!(report.param("found"), Some(&ErrorParam::Number(99)));
    assert_eq!(
        report.param("current"),
        Some(&ErrorParam::Number(layout::VERSION.into()))
    );
}
