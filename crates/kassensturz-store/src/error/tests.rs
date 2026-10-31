// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The error codes of core and store against `docs/ERROR_CODES.md`. Core's
//! errors are checked here too, since core's tests can't see the store's;
//! `kassensturz-import` checks its own codes with the same list reader
//! (`list.rs`).

use std::collections::HashSet;

use kassensturz_core::{CurrencyCodeError, DecimalError, MoneyError};

use super::list::{check, documented, unused};
use super::*;
use crate::lock::HeldLock;
use crate::test_folder::TempFolder;

/// Where the codes checked here come from. `import.` codes are checked in
/// `kassensturz-import`, which the store doesn't depend on.
const ORIGINS: &[&str] = &[
    "money",
    "decimal",
    "currency",
    "transaction",
    "transfer",
    "rule",
    "store",
];

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
        TransactionError::OriginalInPartCurrency(split),
        TransactionError::OriginalSignMismatch(split),
        TransactionError::OriginalWithForeignRate(split),
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
        Corruption::HalfStatedBalance(account),
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
        Corruption::BadPartStatus(text("pending")),
        Corruption::BadPayee(kassensturz_core::PayeeId::generate()),
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
        StoreError::AlreadyExists {
            file: "Household.ksz".to_owned(),
        },
        StoreError::NotFound,
        StoreError::NotEmpty,
        StoreError::NotABook,
        StoreError::UnsupportedLayout { found: 99 },
        StoreError::OlderLayout { found: 16 },
        StoreError::DevelopmentLayout { found: 15 },
        locked(),
        StoreError::BackupFailed(Box::new(StoreError::Io(ErrorKind::StorageFull))),
        StoreError::JournalModeRefused { mode: text("wal") },
        StoreError::UnlistedCurrency(USD),
        StoreError::WithdrawnCurrency(USD),
        StoreError::UnknownCurrency(USD),
        StoreError::UnknownAccount(account),
        StoreError::EmptyAccountName,
        StoreError::IncompleteBankLink(account),
        StoreError::BankAccountAlreadyLinked { existing: account },
        StoreError::NotLinked(account),
        StoreError::UnknownView(kassensturz_core::ViewId::generate()),
        StoreError::ViewNameTaken {
            name: text("Spending"),
        },
        StoreError::InvalidViewDocument { reason: "bad-name" },
        StoreError::ViewDocumentTooBig { max_bytes: 1 << 20 },
        StoreError::UnknownGroup(kassensturz_core::GroupId::generate()),
        StoreError::GroupNameTaken {
            name: text("Fixed costs"),
        },
        StoreError::GroupNameTooLong { max_chars: 200 },
        StoreError::GroupWrongKind {
            name: text("Salary"),
        },
        StoreError::GroupLoop {
            name: text("Household"),
        },
        StoreError::GroupTooDeep { max_levels: 4 },
        StoreError::TooManyGroups { max: 200 },
        StoreError::TooManyGroupMembers { max: 500 },
        StoreError::UnknownBudget(kassensturz_core::BudgetId::generate()),
        StoreError::BudgetNotSpending(account),
        StoreError::BudgetOverlap { category: account },
        StoreError::BudgetAmount { reason: "zero" },
        StoreError::BudgetPeriod {
            reason: "not-period-start",
        },
        StoreError::TooManyBudgets {
            limit: "budgets",
            max: 1_000,
        },
        StoreError::DuplicateAccountName { existing: account },
        StoreError::AccountTypeMismatch {
            account_type: AccountType::CreditCard,
            kind: AccountKind::Asset,
        },
        StoreError::NotAnAccount(account),
        StoreError::StatedBalanceCurrencyMismatch {
            account,
            account_currency: EUR,
            currency: CurrencyCode::new("CHF").expect("valid code"),
        },
        StoreError::AccountClosed(account),
        StoreError::AccountNotEmpty {
            account,
            balance: money,
        },
        StoreError::CategoryInUse {
            category: account,
            used_by: "transactions",
        },
        StoreError::NotACategory(account),
        StoreError::ParentNotACategory(account),
        StoreError::ParentNotAnAccount(account),
        StoreError::UnknownInstitution(kassensturz_core::InstitutionId::generate()),
        StoreError::UnknownTag(kassensturz_core::TagId::generate()),
        StoreError::EmptyName,
        StoreError::DuplicatePayeeName {
            existing: "Bakery".into(),
        },
        StoreError::DuplicateTagName {
            existing: "Kids".into(),
        },
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
        StoreError::OriginalChanged(split),
        StoreError::UnknownRule(rule),
        StoreError::RuleIdInUse(rule),
        StoreError::EmptyRuleName,
        StoreError::DuplicateRuleName { existing: rule },
        StoreError::RuleInsideItself(rule),
        StoreError::RuleNeedsCategory(account),
        StoreError::TransferSuggested(transaction),
        StoreError::UnknownRate(rate),
        StoreError::RateWithinOneCurrency(EUR),
        StoreError::ChangeoverRate,
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
    let unused = unused(&rows, &seen, ORIGINS);
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
    for (error, found) in [
        (StoreError::UnsupportedLayout { found: 99 }, 99),
        (StoreError::OlderLayout { found: 16 }, 16),
        (StoreError::DevelopmentLayout { found: 15 }, 15),
    ] {
        let report = error.report();
        assert_eq!(report.param("found"), Some(&ErrorParam::Number(found)));
        assert_eq!(
            report.param("current"),
            Some(&ErrorParam::Number(layout::VERSION.into()))
        );
    }
}
