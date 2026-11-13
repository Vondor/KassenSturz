// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of remembered monthly moves and "Join all" (D-749), on books in
//! memory. All data is made up (AGENTS.md §3.1 rule 2); the IBANs are the
//! examples in SWIFT's IBAN Registry (D-055).

use kassensturz_core::{
    AccountKind, Actions, AmountTest, BookTo, Condition, ImportedDetails, MatchMode, PartDetail,
    Rule, RuleId, Split, TransferError,
};

use super::*;
use crate::history::RECORDED;
use crate::tests::{EUR, USD, account, date, eur};

/// SWIFT's example IBANs: the savings, holiday and child's accounts.
const SAVINGS_IBAN: &str = "DE89370400440532013000";
const HOLIDAY_IBAN: &str = "GB29NWBK60161331926819";
const CHILD_IBAN: &str = "NL91ABNA0417164300";
/// Another one, for a shop.
const SHOP_IBAN: &str = "FR1420041010050500013M02606";

/// Everything the book holds, every row of every recorded table as SQL
/// text, so two moments can be compared exactly.
fn contents(book: &Book) -> Vec<String> {
    let mut rows = Vec::new();
    for table in &RECORDED {
        let values = table
            .columns
            .iter()
            .map(|(name, _)| format!("quote({name})"))
            .collect::<Vec<_>>()
            .join(" || ', ' || ");
        let mut statement = book
            .conn
            .prepare(&format!(
                "SELECT '{name}: ' || {values} FROM main.{name} ORDER BY 1",
                name = table.name
            ))
            .expect("rows read");
        rows.extend(
            statement
                .query_map([], |row| row.get::<_, String>(0))
                .expect("rows read")
                .collect::<Result<Vec<_>, _>>()
                .expect("rows read"),
        );
    }
    rows
}

struct Household {
    book: Book,
    checking: AccountId,
    savings: AccountId,
    holiday: AccountId,
    child: AccountId,
    salary: AccountId,
}

fn household() -> Household {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
    let savings = account(&mut book, "Savings", AccountKind::Asset, EUR);
    let holiday = account(&mut book, "Holiday fund", AccountKind::Asset, EUR);
    let child = account(&mut book, "Child's account", AccountKind::Asset, EUR);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR);
    Household {
        book,
        checking,
        savings,
        holiday,
        child,
        salary,
    }
}

/// One bank's entry, as the import adds it: the account's part with what
/// the bank delivered, and a part without a category (D-083).
fn entry(
    book: &mut Book,
    account: AccountId,
    minor_units: i64,
    on: Date,
    party: (Option<&str>, Option<&str>),
) -> Transaction {
    let delivered = ImportedDetails {
        bank_id: Some(format!("{on}-{minor_units}-{}", TransactionId::generate())),
        booking_date: Some(on),
        value_date: Some(on),
        other_party_name: party.1.map(str::to_owned),
        other_party_iban: party.0.map(str::to_owned),
        remittance_information: Some("Standing order".into()),
    };
    let booked = Split {
        imported: Some(delivered),
        ..Split::new(account, eur(minor_units))
    };
    let transaction = Transaction::new(
        TransactionId::generate(),
        on,
        party.1.map(str::to_owned),
        vec![booked, Split::uncategorized(eur(-minor_units))],
    )
    .expect("balanced");
    book.add_transaction(&transaction).expect("added");
    transaction
}

fn balance(book: &Book, account: AccountId) -> i64 {
    book.account_parts(account)
        .expect("parts")
        .iter()
        .map(|part| part.amount.minor_units())
        .sum()
}

fn remembered(
    book: &mut Book,
    from: AccountId,
    to: AccountId,
    iban: Option<&str>,
) -> RecurringMoveId {
    book.add_recurring_move(&NewRecurringMove {
        from_account: from,
        to_account: to,
        amount: eur(10_000),
        day_of_month: 1,
        from_party_iban: iban.map(str::to_owned),
        from_party_name: None,
        created_on: date(2026, 9, 1),
    })
    .expect("remembered")
}

/// A Rule that books every incoming entry as salary, the kind that made a
/// standing order count as income before (#784).
fn salary_rule(book: &mut Book, salary: AccountId) {
    book.add_rule(
        &Rule {
            id: RuleId::generate(),
            name: "Money in".into(),
            match_mode: MatchMode::All,
            conditions: vec![Condition::Amount(AmountTest::MoneyIn)],
            actions: Actions {
                book_to: Some(BookTo::Category(salary)),
                payee: None,
            },
        },
        None,
    )
    .expect("Rule added");
}

#[test]
fn a_move_is_remembered_from_the_pair_the_user_joins() {
    let Household {
        mut book,
        checking,
        savings,
        ..
    } = household();
    let out = entry(
        &mut book,
        checking,
        -10_000,
        date(2026, 10, 1),
        (Some("DE89 3704 0044 0532 0130 00"), Some("Ada Example")),
    );
    let into = entry(&mut book, savings, 10_000, date(2026, 10, 2), (None, None));
    // Either order: the paying side decides.
    for (a, b) in [(out.id(), into.id()), (into.id(), out.id())] {
        let new = book
            .recurring_move_for(a, b, date(2026, 10, 5))
            .expect("a move");
        assert_eq!(
            new,
            NewRecurringMove {
                from_account: checking,
                to_account: savings,
                amount: eur(10_000),
                day_of_month: 1,
                from_party_iban: Some("DE89 3704 0044 0532 0130 00".into()),
                from_party_name: Some("Ada Example".into()),
                created_on: date(2026, 10, 5),
            }
        );
    }
    let before = contents(&book);
    let new = book
        .recurring_move_for(out.id(), into.id(), date(2026, 10, 5))
        .expect("a move");
    // Working out the move changes nothing.
    assert_eq!(contents(&book), before);
    let id = book.add_recurring_move(&new).expect("remembered");
    let moves = book.recurring_moves().expect("moves");
    assert_eq!(moves.len(), 1);
    assert_eq!(moves[0].id, id);
    assert_eq!(moves[0].amount, eur(10_000));
    assert_eq!(
        moves[0].from_party_iban.as_deref(),
        Some("DE89 3704 0044 0532 0130 00")
    );
    assert_eq!(book.recurring_move(id).expect("move"), moves[0]);
    // Remembering it changed no transaction.
    assert_eq!(book.transaction(out.id()).expect("read"), out);
    assert_eq!(book.transaction(into.id()).expect("read"), into);
    // The same move again is refused.
    assert!(matches!(
        book.add_recurring_move(&new),
        Err(StoreError::RecurringMoveExists { existing }) if existing == id
    ));
}

#[test]
fn a_pair_that_cannot_be_joined_gives_no_move() {
    let Household {
        mut book,
        checking,
        savings,
        ..
    } = household();
    let out = entry(
        &mut book,
        checking,
        -10_000,
        date(2026, 10, 1),
        (None, None),
    );
    let other = entry(&mut book, savings, 9_000, date(2026, 10, 1), (None, None));
    assert!(matches!(
        book.recurring_move_for(out.id(), other.id(), date(2026, 10, 5)),
        Err(StoreError::Transfer(TransferError::AmountsDiffer { .. }))
    ));
}

#[test]
fn only_a_move_between_two_own_accounts_in_one_currency_is_remembered() {
    let Household {
        mut book,
        checking,
        savings,
        salary,
        ..
    } = household();
    book.use_currency(USD).expect("currency used");
    let dollars = account(&mut book, "Dollar account", AccountKind::Asset, USD);
    let good = NewRecurringMove {
        from_account: checking,
        to_account: savings,
        amount: eur(10_000),
        day_of_month: 1,
        from_party_iban: Some("  ".into()),
        from_party_name: None,
        created_on: date(2026, 10, 5),
    };
    let wrong = [
        (
            NewRecurringMove {
                to_account: checking,
                ..good.clone()
            },
            "same-account",
        ),
        (
            NewRecurringMove {
                amount: eur(0),
                ..good.clone()
            },
            "amount",
        ),
        (
            NewRecurringMove {
                day_of_month: 32,
                ..good.clone()
            },
            "day",
        ),
        (
            NewRecurringMove {
                to_account: dollars,
                ..good.clone()
            },
            "currency",
        ),
    ];
    for (new, reason) in wrong {
        match book.add_recurring_move(&new) {
            Err(StoreError::RecurringMove(error)) => assert_eq!(error.keyword(), reason),
            other => panic!("{reason}: {other:?}"),
        }
    }
    assert!(matches!(
        book.add_recurring_move(&NewRecurringMove {
            to_account: salary,
            ..good.clone()
        }),
        Err(StoreError::NotAnAccount(id)) if id == salary
    ));
    assert!(book.recurring_moves().expect("moves").is_empty());
    // Spaces alone count as no party.
    let id = book.add_recurring_move(&good).expect("remembered");
    assert_eq!(book.recurring_move(id).expect("move").from_party_iban, None);
}

#[test]
fn remembering_and_removing_a_move_are_undone_and_redone() {
    let Household {
        mut book,
        checking,
        savings,
        ..
    } = household();
    let empty = contents(&book);
    let id = remembered(&mut book, checking, savings, None);
    let with_move = contents(&book);
    book.remove_recurring_move(id).expect("removed");
    assert!(book.recurring_moves().expect("moves").is_empty());
    assert!(matches!(
        book.remove_recurring_move(id),
        Err(StoreError::UnknownRecurringMove(unknown)) if unknown == id
    ));
    assert_eq!(
        book.undo().expect("undone").map(|change| change.action),
        Some(Action::RemoveRecurringMove)
    );
    assert_eq!(contents(&book), with_move);
    assert_eq!(
        book.undo().expect("undone").map(|change| change.action),
        Some(Action::AddRecurringMove)
    );
    assert_eq!(contents(&book), empty);
    book.redo().expect("redone");
    assert_eq!(contents(&book), with_move);
}

/// The three standing orders of #784 on the 1st, each from the checking
/// account, the same amount, to three accounts, as both banks' entries.
struct ThreeOrders {
    household: Household,
    moves: [RecurringMoveId; 3],
    outs: [Transaction; 3],
    intos: [Transaction; 3],
}

fn three_orders() -> ThreeOrders {
    let mut household = household();
    let Household {
        book,
        checking,
        savings,
        holiday,
        child,
        ..
    } = &mut household;
    let targets = [
        (*savings, SAVINGS_IBAN),
        (*holiday, HOLIDAY_IBAN),
        (*child, CHILD_IBAN),
    ];
    let moves = targets.map(|(to, iban)| remembered(book, *checking, to, Some(iban)));
    let on = date(2026, 10, 1);
    let outs = targets.map(|(_, iban)| {
        entry(
            book,
            *checking,
            -10_000,
            on,
            (Some(iban), Some("Ada Example")),
        )
    });
    let intos = targets.map(|(to, _)| {
        entry(
            book,
            to,
            10_000,
            date(2026, 10, 2),
            (None, Some("Ada Example")),
        )
    });
    ThreeOrders {
        household,
        moves,
        outs,
        intos,
    }
}

#[test]
fn the_pairs_that_fit_remembered_moves_join_in_one_action_that_undo_takes_back() {
    let ThreeOrders {
        mut household,
        moves,
        outs,
        intos,
    } = three_orders();
    let book = &mut household.book;
    let accounts = [
        household.checking,
        household.savings,
        household.holiday,
        household.child,
    ];
    let balances: Vec<i64> = accounts.iter().map(|&id| balance(book, id)).collect();
    let before = contents(book);

    let joins = book.join_recurring_moves(None).expect("joined");
    assert!(joins.needs_you.is_empty(), "{:?}", joins.needs_you);
    assert_eq!(joins.joined.len(), 3);
    for index in 0..3 {
        let join = joins
            .joined
            .iter()
            .find(|join| join.recurring_move == moves[index])
            .expect("joined");
        assert_eq!(join.outgoing.transaction, outs[index].id());
        assert_eq!(join.incoming.transaction, intos[index].id());
        // One Transfer, both account parts as delivered.
        let joined = book.transaction(outs[index].id()).expect("joined");
        assert!(joined.splits().iter().all(|split| split.account.is_some()));
        assert_eq!(joined.splits().len(), 2);
        assert!(book.transaction(intos[index].id()).is_err());
    }
    // No balance changed (principle 1).
    let after: Vec<i64> = accounts.iter().map(|&id| balance(book, id)).collect();
    assert_eq!(after, balances);
    // Nothing is suggested any more.
    assert!(book.transfer_suggestions().expect("suggestions").is_empty());

    // One undo takes back all three joins, table for table.
    assert_eq!(
        book.undo().expect("undone").map(|change| change.action),
        Some(Action::JoinRecurringMoves)
    );
    assert_eq!(contents(book), before);
    book.redo().expect("redone");
    assert_eq!(book.transfer_suggestions().expect("suggestions").len(), 0);
    // A second run finds nothing more and records nothing.
    let changes = book.history().expect("history").len();
    let again = book.join_recurring_moves(None).expect("ran");
    assert!(again.joined.is_empty());
    assert_eq!(book.history().expect("history").len(), changes);
}

#[test]
fn only_pairs_with_a_new_side_join_when_the_caller_names_them() {
    let ThreeOrders {
        mut household,
        outs,
        intos,
        ..
    } = three_orders();
    let book = &mut household.book;
    let new: HashSet<TransactionId> = [intos[1].id()].into_iter().collect();
    let joins = book.join_recurring_moves(Some(&new)).expect("joined");
    assert_eq!(joins.joined.len(), 1);
    assert_eq!(joins.joined[0].outgoing.transaction, outs[1].id());
    // The other two pairs wait for the user as before.
    assert_eq!(book.transfer_suggestions().expect("suggestions").len(), 2);
}

#[test]
fn without_remembered_moves_nothing_joins_and_nothing_waits() {
    let Household {
        mut book,
        checking,
        savings,
        salary,
        ..
    } = household();
    salary_rule(&mut book, salary);
    let out = entry(
        &mut book,
        checking,
        -10_000,
        date(2026, 10, 1),
        (None, None),
    );
    let into = entry(&mut book, savings, 10_000, date(2026, 10, 1), (None, None));
    let joins = book.join_recurring_moves(None).expect("ran");
    assert!(joins.joined.is_empty());
    assert!(book.waiting_for_moves().expect("read").is_empty());
    assert_eq!(book.transaction(out.id()).expect("read"), out);
    // The suggestion still holds the Rule back, as before (D-088).
    let run = book.run_rules().expect("Rules ran");
    assert!(run.changed.is_empty());
    assert!(matches!(run.skipped[0].reason, StoreError::TransferSuggested(id) if id == into.id()));
}

#[test]
fn a_rule_leaves_a_side_that_waits_for_its_other_half_alone() {
    let Household {
        mut book,
        checking,
        savings,
        salary,
        ..
    } = household();
    salary_rule(&mut book, salary);
    let id = remembered(&mut book, checking, savings, None);
    // Only the savings account's statement is in: its entry waits.
    let into = entry(
        &mut book,
        savings,
        10_000,
        date(2026, 10, 2),
        (None, Some("Ada Example")),
    );
    // An incoming entry that fits no move is sorted as before.
    let pay = entry(
        &mut book,
        checking,
        250_000,
        date(2026, 10, 1),
        (None, Some("Example Employer")),
    );
    assert_eq!(
        book.waiting_for_moves().expect("read"),
        [into.id()].into_iter().collect()
    );
    // A register can say which move an entry waits for (US6.3).
    let waiting = book.waiting_moves().expect("read");
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[&into.id()].id, id);
    let run = book.run_rules().expect("Rules ran");
    assert_eq!(run.changed.len(), 1);
    assert_eq!(run.changed[0].transaction, pay.id());
    assert_eq!(run.skipped.len(), 1);
    assert!(
        matches!(run.skipped[0].reason, StoreError::FitsMonthlyMove(waiting) if waiting == into.id())
    );
    // It counts in no category, so neither as income nor as spending.
    assert_eq!(book.transaction(into.id()).expect("read"), into);

    // The checking account's statement comes: the pair joins.
    let out = entry(
        &mut book,
        checking,
        -10_000,
        date(2026, 10, 1),
        (None, None),
    );
    let joins = book.join_recurring_moves(None).expect("joined");
    assert_eq!(joins.joined.len(), 1);
    assert_eq!(joins.joined[0].outgoing.transaction, out.id());
    assert_eq!(joins.joined[0].incoming.transaction, into.id());

    // Once the move is removed, a waiting entry is an ordinary one again.
    book.undo().expect("join undone");
    book.remove_recurring_move(id).expect("removed");
    assert!(book.waiting_for_moves().expect("read").is_empty());
}

#[test]
fn a_side_sorted_by_hand_never_joins_by_itself() {
    let Household {
        mut book,
        checking,
        savings,
        salary,
        ..
    } = household();
    remembered(&mut book, checking, savings, None);
    let out = entry(
        &mut book,
        checking,
        -10_000,
        date(2026, 10, 1),
        (None, None),
    );
    let into = entry(&mut book, savings, 10_000, date(2026, 10, 1), (None, None));
    // The user books the incoming entry as salary by hand (D-031).
    let splits = into
        .splits()
        .iter()
        .map(|split| Split {
            account: split.account.or(Some(salary)),
            ..split.clone()
        })
        .collect();
    let sorted = Transaction::new(into.id(), into.booking_date(), None, splits)
        .expect("balanced")
        .with_details_of(&into);
    book.update_transaction(&sorted).expect("sorted");
    let joins = book.join_recurring_moves(None).expect("ran");
    assert!(joins.joined.is_empty());
    assert!(joins.needs_you.is_empty());
    assert_eq!(book.transaction(out.id()).expect("read"), out);
    assert_eq!(
        book.transaction(into.id()).expect("read").splits(),
        sorted.splits()
    );
}

#[test]
fn a_rule_sorted_side_joins_and_undo_brings_its_category_back() {
    let Household {
        mut book,
        checking,
        savings,
        salary,
        ..
    } = household();
    salary_rule(&mut book, salary);
    // Sorted by the Rule before the move was remembered.
    let into = entry(&mut book, savings, 10_000, date(2026, 10, 1), (None, None));
    book.run_rules().expect("Rules ran");
    let sorted = book.transaction(into.id()).expect("read");
    assert!(
        sorted
            .splits()
            .iter()
            .any(|split| split.account == Some(salary))
    );
    remembered(&mut book, checking, savings, None);
    let out = entry(
        &mut book,
        checking,
        -10_000,
        date(2026, 10, 1),
        (None, None),
    );
    let before = contents(&book);
    let joins = book.join_recurring_moves(None).expect("joined");
    assert_eq!(joins.joined.len(), 1);
    let joined = book.transaction(out.id()).expect("joined");
    assert!(
        joined
            .splits()
            .iter()
            .all(|split| split.account != Some(salary))
    );
    book.undo().expect("undone");
    assert_eq!(contents(&book), before);
    assert_eq!(book.transaction(into.id()).expect("back"), sorted);
}

#[test]
fn a_shop_payment_of_the_same_amount_is_not_taken_for_the_move() {
    let Household {
        mut book,
        checking,
        savings,
        ..
    } = household();
    remembered(&mut book, checking, savings, Some(SAVINGS_IBAN));
    let shop = entry(
        &mut book,
        checking,
        -10_000,
        date(2026, 10, 1),
        (Some(SHOP_IBAN), Some("Corner Shop")),
    );
    let out = entry(
        &mut book,
        checking,
        -10_000,
        date(2026, 10, 1),
        (Some(SAVINGS_IBAN), None),
    );
    let into = entry(&mut book, savings, 10_000, date(2026, 10, 1), (None, None));
    let joins = book.join_recurring_moves(None).expect("joined");
    assert_eq!(joins.joined.len(), 1);
    assert_eq!(joins.joined[0].outgoing.transaction, out.id());
    assert_eq!(joins.joined[0].incoming.transaction, into.id());
    assert_eq!(book.transaction(shop.id()).expect("read"), shop);
}

#[test]
fn a_move_without_a_party_takes_the_receiving_accounts_own_iban() {
    let Household {
        mut book, checking, ..
    } = household();
    // A second savings account, whose number the user gave.
    let savings = AccountId::generate();
    book.add_account(&kassensturz_core::Account {
        id: savings,
        name: "Savings with number".into(),
        kind: AccountKind::Asset,
        currency: EUR,
        account_type: None,
        closed: false,
        parent: None,
        number: Some("DE89 3704 0044 0532 0130 00".into()),
        notes: None,
        institution: None,
    })
    .expect("account added");
    remembered(&mut book, checking, savings, None);
    let out = entry(
        &mut book,
        checking,
        -10_000,
        date(2026, 10, 1),
        (Some(SAVINGS_IBAN), Some("Me")),
    );
    entry(&mut book, savings, 10_000, date(2026, 10, 1), (None, None));
    let joins = book.join_recurring_moves(None).expect("joined");
    assert_eq!(joins.joined.len(), 1);
    assert_eq!(joins.joined[0].outgoing.transaction, out.id());
}

#[test]
fn two_equal_entries_and_a_join_that_would_lose_a_note_need_the_user() {
    let Household {
        mut book,
        checking,
        savings,
        holiday,
        ..
    } = household();
    let to_savings = remembered(&mut book, checking, savings, None);
    let first = entry(
        &mut book,
        checking,
        -10_000,
        date(2026, 10, 1),
        (None, None),
    );
    let second = entry(
        &mut book,
        checking,
        -10_000,
        date(2026, 10, 1),
        (None, None),
    );
    entry(&mut book, savings, 10_000, date(2026, 10, 1), (None, None));
    // On the holiday fund's move, the other part of the receiving side
    // has a note the joined part would lose (D-691).
    let to_holiday = book
        .add_recurring_move(&NewRecurringMove {
            from_account: checking,
            to_account: holiday,
            amount: eur(7_000),
            day_of_month: 1,
            from_party_iban: None,
            from_party_name: None,
            created_on: date(2026, 9, 1),
        })
        .expect("remembered");
    let out = entry(&mut book, checking, -7_000, date(2026, 10, 1), (None, None));
    let into = entry(&mut book, holiday, 7_000, date(2026, 10, 1), (None, None));
    let splits = into
        .splits()
        .iter()
        .map(|split| Split {
            note: Some(
                if split.account.is_some() {
                    "Holiday"
                } else {
                    "For the trip"
                }
                .to_owned(),
            ),
            ..split.clone()
        })
        .collect();
    let noted = Transaction::new(into.id(), into.booking_date(), None, splits)
        .expect("balanced")
        .with_details_of(&into);
    book.update_transaction(&noted).expect("noted");

    let before = contents(&book);
    let joins = book.join_recurring_moves(None).expect("ran");
    assert!(joins.joined.is_empty());
    assert_eq!(joins.needs_you.len(), 2);
    let unclear = joins
        .needs_you
        .iter()
        .find(|needs| needs.recurring_move == to_savings)
        .expect("unclear");
    assert!(matches!(unclear.reason, NotJoined::Unclear));
    let mut sides = unclear.transactions.clone();
    sides.sort();
    let mut expected = vec![first.id(), second.id()];
    expected.sort();
    assert_eq!(sides, expected);
    let refused = joins
        .needs_you
        .iter()
        .find(|needs| needs.recurring_move == to_holiday)
        .expect("refused");
    assert_eq!(refused.transactions, vec![out.id(), into.id()]);
    assert!(matches!(
        refused.reason,
        NotJoined::Refused(StoreError::Transfer(TransferError::PartHoldsMore {
            detail: PartDetail::Note,
            ..
        }))
    ));
    // Nothing changed, nothing recorded.
    assert_eq!(contents(&book), before);
}

#[test]
fn join_all_joins_the_suggested_pairs_in_one_action() {
    let Household {
        mut book,
        checking,
        savings,
        holiday,
        ..
    } = household();
    let pairs: Vec<(Transaction, Transaction)> = (1..=3)
        .map(|day| {
            let out = entry(
                &mut book,
                checking,
                -1_000 * i64::from(day),
                date(2026, 10, day),
                (None, None),
            );
            let into = entry(
                &mut book,
                savings,
                1_000 * i64::from(day),
                date(2026, 10, day),
                (None, None),
            );
            (out, into)
        })
        .collect();
    let stray_out = entry(
        &mut book,
        checking,
        -5_000,
        date(2026, 10, 20),
        (None, None),
    );
    let stray_in = entry(&mut book, holiday, 5_000, date(2026, 11, 20), (None, None));
    let balances = [checking, savings, holiday].map(|id| balance(&book, id));
    let before = contents(&book);
    // The incoming side first for one of them: either order is taken.
    let mut asked: Vec<(TransactionId, TransactionId)> = pairs
        .iter()
        .map(|(out, into)| (out.id(), into.id()))
        .collect();
    asked[1] = (asked[1].1, asked[1].0);
    asked.push((stray_out.id(), stray_in.id()));
    asked.push((pairs[0].0.id(), pairs[0].1.id()));
    let result = book.join_transfers(&asked).expect("joined");
    assert_eq!(
        result.joined,
        pairs.iter().map(|(out, _)| out.id()).collect::<Vec<_>>()
    );
    assert_eq!(result.refused.len(), 1);
    assert!(matches!(
        result.refused[0].reason,
        StoreError::TransferNotSuggested { outgoing, incoming }
            if outgoing == stray_out.id() && incoming == stray_in.id()
    ));
    assert_eq!(
        [checking, savings, holiday].map(|id| balance(&book, id)),
        balances
    );
    assert_eq!(
        book.undo().expect("undone").map(|change| change.action),
        Some(Action::JoinTransfers)
    );
    assert_eq!(contents(&book), before);
    // Nothing to join records nothing.
    let changes = book.history().expect("history").len();
    let none = book
        .join_transfers(&[(stray_out.id(), stray_in.id())])
        .expect("ran");
    assert!(none.joined.is_empty());
    assert_eq!(book.history().expect("history").len(), changes);
}
