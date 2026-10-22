// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Rules in a book: keeping the user's tree of Rules, and running it on the
//! book's transactions (D-014, D-031, D-088).
//!
//! Rules only touch what nobody decided by hand, and book a transaction
//! once (Q-044). A run books a transaction that has two parts, one for one
//! of the user's accounts and one without a category yet; `split_rules`
//! marks the part a Rule booked. What a Rule booked keeps its booking when
//! the Rules change later, unless the user changes that Rule and asks for
//! what it booked to follow ([`Book::update_rule_and_rebook`]). When the
//! user changes such a transaction, its parts are written again without the
//! mark, so no Rule changes it again; and a transaction broken into more
//! Parts is never theirs (D-031).

use std::collections::{HashMap, HashSet};

use kassensturz_core::{
    Account, AccountId, Actions, AmountTest, BookTo, Condition, CurrencyCode, ForeignAmount,
    MatchMode, Money, Rule, RuleFacts, RuleId, RuleNode, RuleSet, Split, SplitId, TextField,
    TextOperator, Transaction, TransactionId, TransferError,
};
use rusqlite::{OptionalExtension, params};

use crate::error::{Corruption, StoreError};
use crate::transactions::{insert_split_rate, split_rate};
use crate::{Action, Book, currency_from_text, date_from_text, date_to_text};

/// What running the Rules, or rebooking what a changed Rule booked, does or
/// would do to a book's transactions.
#[derive(Debug, Default)]
pub struct RuleRun {
    /// The transactions whose category, Transfer account or payee the Rules
    /// change, oldest first.
    pub changed: Vec<RuleChange>,
    /// The transactions a Rule matched but couldn't change, oldest first,
    /// e.g. because an exchange rate is missing. They stay as they are.
    pub skipped: Vec<RuleSkip>,
}

/// One transaction the Rules match ([`Book::rule_matches`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleMatch {
    /// The transaction.
    pub transaction: TransactionId,
    /// The user's account its matching part is in.
    pub account: AccountId,
    /// The Rules that matched, from the top-level Rule down to the sub-Rule
    /// that decided. Never empty.
    pub rules: Vec<RuleId>,
}

/// One transaction the Rules change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleChange {
    /// The transaction.
    pub transaction: TransactionId,
    /// The Rule that decided: the deepest sub-Rule that matched.
    pub rule: RuleId,
}

/// One transaction a Rule matched but couldn't change.
#[derive(Debug)]
pub struct RuleSkip {
    /// The transaction.
    pub transaction: TransactionId,
    /// The Rule that decided.
    pub rule: RuleId,
    /// Why it stays as it is: [`StoreError::TransferSuggested`] if it looks
    /// like one side of a Transfer, [`StoreError::NoRate`] if the category
    /// or account is in another currency and the book has no rate for the
    /// day, [`StoreError::AccountClosed`] if an account it would book to, or
    /// take the money from, is closed, or a [`StoreError::Transfer`] if a
    /// Transfer would go into the transaction's own account.
    pub reason: StoreError,
}

/// Which part of a transaction a Rule booked, and which Rule. A transaction
/// with a mark is one a Rule booked and the user didn't change since, so
/// changing that Rule can book it again when the user asks for it
/// ([`Book::update_rule_and_rebook`]); one the user changed has none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleMark {
    /// The part the Rule booked, or left without a category while it set
    /// the payee.
    pub split: SplitId,
    /// The Rule that decided, or `None` if it was removed since.
    pub rule: Option<RuleId>,
}

/// A change the Rules make to one transaction.
struct Planned {
    transaction: Transaction,
    part: SplitId,
    rule: RuleId,
}

/// Which transactions the Rules look at.
#[derive(Clone, Copy)]
enum Scope<'a> {
    /// The ones without a category yet, which a run books (Q-044).
    Uncategorized,
    /// The ones among these without a category yet, e.g. what an import
    /// just added (D-030).
    Only(&'a HashSet<TransactionId>),
    /// The ones these Rules booked and the user didn't change since, which
    /// changing one of the Rules books again when the user asks for it.
    BookedBy(&'a HashSet<RuleId>),
}

impl Book {
    /// Adds a Rule at the end of the top-level list, or of `parent`'s
    /// sub-Rules (D-014). The Rule is checked ([`Rule::check`]); its name
    /// can't be empty or the same as a sibling's, ignoring capitals; it
    /// books to a category or, for a Transfer, to an account; and every
    /// account, currency and date its conditions name must be in the book.
    pub fn add_rule(&mut self, rule: &Rule, parent: Option<RuleId>) -> Result<(), StoreError> {
        if let Some(parent) = parent {
            self.rule(parent)?;
        }
        let exists: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM rules WHERE id = ?1",
                [rule.id.to_bytes()],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Err(StoreError::RuleIdInUse(rule.id));
        }
        self.check_rule(rule, parent)?;
        let mut order = self.sibling_ids(parent)?;
        order.push(rule.id);
        self.as_one_action(Action::AddRule, |book| {
            book.conn.execute(
                "INSERT INTO rules
                     (id, parent_id, position, name, match_mode,
                      category_id, transfer_account_id, payee)
                 VALUES (?1, ?2, 0, '', 'all', NULL, NULL, NULL)",
                params![rule.id.to_bytes(), parent.map(RuleId::to_bytes)],
            )?;
            book.write_rule(rule)?;
            book.write_order(&order)
        })
    }

    /// Changes a Rule: its name, whether all or any conditions count, its
    /// conditions and its actions, checked as in [`Book::add_rule`]. Its
    /// place and its sub-Rules stay. What it booked earlier keeps its
    /// booking (Q-044); [`Book::update_rule_and_rebook`] books it again.
    pub fn update_rule(&mut self, rule: &Rule) -> Result<(), StoreError> {
        let parent = self.rule_parent(rule.id)?;
        self.check_rule(rule, parent)?;
        self.as_one_action(Action::EditRule, |book| book.write_rule(rule))
    }

    /// Changes a Rule as [`Book::update_rule`] does and, in the same user
    /// action, books again what it and its sub-Rules booked earlier, for
    /// when the user wants their earlier bookings to follow the change
    /// (Q-044).
    ///
    /// Those transactions get what the Rules give them now, as in a run
    /// ([`Book::run_rules`]): the first matching top-level Rule decides,
    /// which may now be another one. A transaction the user changed since
    /// has no mark and isn't touched (D-031). When no Rule matches it
    /// anymore, what the Rule did earlier stays (Q-046). One that can't be
    /// booked again, e.g. because an exchange rate is missing, stays as it is
    /// and is reported as skipped.
    pub fn update_rule_and_rebook(&mut self, rule: &Rule) -> Result<RuleRun, StoreError> {
        let (run, planned) = self.plan_rebook(rule)?;
        self.as_one_action(Action::EditRule, |book| {
            book.write_rule(rule)?;
            for change in &planned {
                book.write_planned(change)?;
            }
            Ok(run)
        })
    }

    /// What [`Book::update_rule_and_rebook`] would do to the transactions the
    /// Rule booked earlier, without changing anything, the Rule included.
    pub fn preview_rebook(&self, rule: &Rule) -> Result<RuleRun, StoreError> {
        self.plan_rebook(rule).map(|(run, _)| run)
    }

    /// Removes a Rule with all its sub-Rules. Transactions they changed keep
    /// what they did, but no longer say which Rule it was.
    pub fn remove_rule(&mut self, id: RuleId) -> Result<(), StoreError> {
        let parent = self.rule_parent(id)?;
        let order: Vec<RuleId> = self
            .sibling_ids(parent)?
            .into_iter()
            .filter(|&sibling| sibling != id)
            .collect();
        self.as_one_action(Action::RemoveRule, |book| {
            // Its sub-Rules and conditions go with it, and the marks of the
            // parts it booked lose its ID.
            book.conn
                .execute("DELETE FROM rules WHERE id = ?1", [id.to_bytes()])?;
            book.write_order(&order)
        })
    }

    /// Moves a Rule, with its sub-Rules, to `position` (from 0) among the
    /// top-level Rules or among `parent`'s sub-Rules; a position past the
    /// end puts it last. The order decides which Rule wins (D-031). A Rule
    /// can't move inside itself or one of its sub-Rules, nor next to a Rule
    /// with the same name.
    pub fn move_rule(
        &mut self,
        id: RuleId,
        parent: Option<RuleId>,
        position: usize,
    ) -> Result<(), StoreError> {
        let rule = self.rule(id)?;
        let old_parent = self.rule_parent(id)?;
        // Walking up from the new place must not meet the Rule itself. A
        // book with a loop that doesn't include it would never end
        // otherwise, so the walk takes at most as many steps as there are
        // Rules.
        let count = self.rule_count()?;
        let mut above = parent;
        let mut steps = 0;
        while let Some(next) = above {
            if next == id {
                return Err(StoreError::RuleInsideItself(id));
            }
            steps += 1;
            if steps > count {
                return Err(StoreError::Corrupt(Corruption::BadRuleTree(next)));
            }
            above = self.rule_parent(next)?;
        }
        self.check_rule(&rule, parent)?;
        let old_order: Vec<RuleId> = self
            .sibling_ids(old_parent)?
            .into_iter()
            .filter(|&sibling| sibling != id)
            .collect();
        let mut new_order = if parent == old_parent {
            old_order.clone()
        } else {
            self.sibling_ids(parent)?
        };
        new_order.insert(position.min(new_order.len()), id);
        self.as_one_action(Action::MoveRule, |book| {
            book.conn.execute(
                "UPDATE rules SET parent_id = ?2 WHERE id = ?1",
                params![id.to_bytes(), parent.map(RuleId::to_bytes)],
            )?;
            if parent != old_parent {
                book.write_order(&old_order)?;
            }
            book.write_order(&new_order)
        })
    }

    /// The Rule with this ID, without its sub-Rules.
    pub fn rule(&self, id: RuleId) -> Result<Rule, StoreError> {
        let row = self
            .conn
            .query_row(
                &format!("SELECT {RULE_COLUMNS} FROM rules WHERE id = ?1"),
                [id.to_bytes()],
                RuleRow::read,
            )
            .optional()?
            .ok_or(StoreError::UnknownRule(id))?;
        let mut conditions = self.conditions(Some(id))?;
        row.into_rule(conditions.remove(&id).unwrap_or_default())
    }

    /// Every Rule as a tree, each list in the user's order.
    pub fn rules(&self) -> Result<Vec<RuleNode>, StoreError> {
        let mut statement = self
            .conn
            .prepare(&format!("SELECT {RULE_COLUMNS} FROM rules"))?;
        let rows = statement
            .query_map([], RuleRow::read)?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let mut conditions = self.conditions(None)?;
        let mut children: HashMap<Option<RuleId>, Vec<(i64, Rule)>> = HashMap::new();
        let count = rows.len();
        let ids: HashSet<RuleId> = rows.iter().map(|row| row.id).collect();
        for row in rows {
            if let Some(parent) = row.parent
                && !ids.contains(&parent)
            {
                return Err(StoreError::Corrupt(Corruption::BadRuleTree(row.id)));
            }
            let (parent, position) = (row.parent, row.position);
            let own = conditions.remove(&row.id).unwrap_or_default();
            let rule = row.into_rule(own)?;
            children.entry(parent).or_default().push((position, rule));
        }
        let mut sorted: HashMap<Option<RuleId>, Vec<Rule>> = children
            .into_iter()
            .map(|(parent, mut list)| {
                list.sort_by_key(|(position, rule)| (*position, rule.id));
                (parent, list.into_iter().map(|(_, rule)| rule).collect())
            })
            .collect();
        let mut built = 0;
        let tree = build_tree(&mut sorted, None, &mut built);
        // Rules that no top-level Rule leads to sit in a loop.
        if built != count {
            let lost = sorted
                .values()
                .flatten()
                .map(|rule| rule.id)
                .min()
                .expect("a Rule was not reached");
            return Err(StoreError::Corrupt(Corruption::BadRuleTree(lost)));
        }
        Ok(tree)
    }

    /// Which part of `transaction` a Rule booked, if the Rules may change it
    /// again ([`RuleMark`]).
    pub fn rule_mark(&self, transaction: TransactionId) -> Result<Option<RuleMark>, StoreError> {
        self.transaction(transaction)?;
        let row: Option<([u8; 16], Option<[u8; 16]>)> = self
            .conn
            .query_row(
                "SELECT m.split_id, m.rule_id
                 FROM split_rules AS m JOIN splits AS s ON s.id = m.split_id
                 WHERE s.transaction_id = ?1",
                [transaction.to_bytes()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(row.map(|(split, rule)| RuleMark {
            split: SplitId::from_bytes(split),
            rule: rule.map(RuleId::from_bytes),
        }))
    }

    /// Every part a Rule booked that the Rules may change again, with that
    /// Rule ([`RuleMark`]), sorted by part, e.g. for the complete export.
    pub fn rule_marks(&self) -> Result<Vec<RuleMark>, StoreError> {
        let mut marks: Vec<RuleMark> = self
            .marks()?
            .into_iter()
            .map(|(split, rule)| RuleMark { split, rule })
            .collect();
        marks.sort_by_key(|mark| mark.split);
        Ok(marks)
    }

    /// Which transactions the Rules match as the book is now, whether or not
    /// a Rule booked them, oldest first: every transaction with one part in
    /// one of the user's accounts and the other in a category or without a
    /// category yet, checked as a run checks it ([`Book::run_rules`]), with
    /// what the user's account shows (D-108). Changes nothing.
    pub fn rule_matches(&self) -> Result<Vec<RuleMatch>, StoreError> {
        let tree = self.rules()?;
        if tree.is_empty() {
            return Ok(Vec::new());
        }
        let rules = RuleSet::new(&tree).map_err(StoreError::Rule)?;
        let accounts: HashMap<AccountId, Account> = self
            .all_accounts()?
            .into_iter()
            .map(|account| (account.id, account))
            .collect();
        let in_an_account = |part: &Split| {
            part.account
                .and_then(|id| accounts.get(&id))
                .is_some_and(|account| account.kind.is_account())
        };
        let mut ids = self
            .conn
            .prepare(
                "SELECT transaction_id FROM splits GROUP BY transaction_id HAVING count(*) = 2",
            )?
            .query_map([], |row| Ok(TransactionId::from_bytes(row.get(0)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.sort_unstable();
        let mut transactions = ids
            .into_iter()
            .map(|id| self.transaction(id))
            .collect::<Result<Vec<_>, _>>()?;
        transactions.sort_by_key(|transaction| (transaction.booking_date(), transaction.id()));

        let mut found = Vec::new();
        for transaction in transactions {
            let [first, second] = transaction.splits() else {
                continue;
            };
            let own = match (in_an_account(first), in_an_account(second)) {
                (true, false) => first,
                (false, true) => second,
                // A move between two accounts, or no account at all.
                _ => continue,
            };
            let Some(account) = own.account else {
                continue;
            };
            if own.foreign.is_some() {
                continue;
            }
            let facts = RuleFacts {
                account,
                amount: own.amount,
                booking_date: transaction.date_of(own),
                payee: transaction.payee_of(own),
                imported: own.imported.as_ref(),
            };
            if let Some(outcome) = rules.evaluate(&facts) {
                found.push(RuleMatch {
                    transaction: transaction.id(),
                    account,
                    rules: outcome.rules,
                });
            }
        }
        Ok(found)
    }

    /// What running the Rules would do, without changing anything: see
    /// [`Book::run_rules`].
    pub fn preview_rules(&self) -> Result<RuleRun, StoreError> {
        self.plan(&self.rules()?, Scope::Uncategorized)
            .map(|(run, _)| run)
    }

    /// Runs the Rules on every transaction without a category yet, as one
    /// user action, which undo takes back as a whole (D-031, D-088).
    ///
    /// A transaction is theirs when it has two parts: one for one of the
    /// user's accounts, which the Rules look at, and one without a category
    /// yet, which they book. A Rule books a transaction once: what a Rule
    /// booked earlier keeps its booking, even after the Rules changed,
    /// unless the user changes that Rule and asks for what it booked to
    /// follow ([`Book::update_rule_and_rebook`], Q-044). A transaction the
    /// user categorized, changed or broke into Parts stays as it is, and so
    /// does one that looks like one side of a Transfer, since Transfers come
    /// first (D-030). For each, the first matching top-level Rule decides,
    /// refined by its matching sub-Rules ([`RuleSet::evaluate`]): it books
    /// the other part to a category or, as a Transfer, to an account, and
    /// sets the payee. A category or account in another currency gets the
    /// amount converted with the book's rate for the booking date (D-043).
    /// When no Rule matches, or one matches without actions, the
    /// transaction stays as it is. A Rule that only set the payee left the
    /// category open, so a later run can still book that transaction.
    pub fn run_rules(&mut self) -> Result<RuleRun, StoreError> {
        let (run, planned) = self.plan(&self.rules()?, Scope::Uncategorized)?;
        if planned.is_empty() {
            return Ok(run);
        }
        self.as_one_action(Action::RunRules, |book| {
            for change in &planned {
                book.write_planned(change)?;
            }
            Ok(run)
        })
    }

    /// Runs the Rules on `transactions` only, as [`Book::run_rules`] runs
    /// them on every transaction without a category yet, and as one user
    /// action too. The import runs them on the transactions it just added
    /// this way, so the rest of the book stays as it is (D-030, D-088).
    /// Among `transactions`, the Rules book only what has no category yet.
    pub fn run_rules_on(&mut self, transactions: &[TransactionId]) -> Result<RuleRun, StoreError> {
        let only: HashSet<TransactionId> = transactions.iter().copied().collect();
        let (run, planned) = self.plan(&self.rules()?, Scope::Only(&only))?;
        if planned.is_empty() {
            return Ok(run);
        }
        self.as_one_action(Action::RunRules, |book| {
            for change in &planned {
                book.write_planned(change)?;
            }
            Ok(run)
        })
    }

    /// What changing `rule` does to what it and its sub-Rules booked
    /// earlier, with the Rules as they are going to be.
    fn plan_rebook(&self, rule: &Rule) -> Result<(RuleRun, Vec<Planned>), StoreError> {
        let parent = self.rule_parent(rule.id)?;
        self.check_rule(rule, parent)?;
        let mut tree = self.rules()?;
        let booked_by = replace_rule(&mut tree, rule).expect("a Rule of the book is in its tree");
        self.plan(&tree, Scope::BookedBy(&booked_by))
    }

    /// What the Rules in `tree` do to the transactions `scope` names: what
    /// to report, and what to write.
    fn plan(
        &self,
        tree: &[RuleNode],
        scope: Scope<'_>,
    ) -> Result<(RuleRun, Vec<Planned>), StoreError> {
        let rules = RuleSet::new(tree).map_err(StoreError::Rule)?;
        let accounts: HashMap<AccountId, Account> = self
            .all_accounts()?
            .into_iter()
            .map(|account| (account.id, account))
            .collect();
        let is_account = |id: AccountId| accounts.get(&id).is_some_and(|a| a.kind.is_account());
        let suggested: HashSet<TransactionId> = self
            .transfer_suggestions()?
            .into_iter()
            .flat_map(|found| [found.outgoing.transaction, found.incoming.transaction])
            .collect();
        let marks = self.marks()?;
        let mut transactions = self
            .candidates(scope)?
            .into_iter()
            .map(|id| self.transaction(id))
            .collect::<Result<Vec<_>, _>>()?;
        transactions.sort_by_key(|transaction| (transaction.booking_date(), transaction.id()));

        let mut run = RuleRun::default();
        let mut planned = Vec::new();
        for transaction in transactions {
            let [first, second] = transaction.splits() else {
                continue;
            };
            let marked = |part: &Split| marks.get(&part.id).copied();
            let (own, part, mark) = match (marked(first), marked(second)) {
                (Some(mark), _) => (second, first, Some(mark)),
                (_, Some(mark)) => (first, second, Some(mark)),
                (None, None) => match (first.account, second.account) {
                    (Some(_), None) => (first, second, None),
                    (None, Some(_)) => (second, first, None),
                    _ => continue,
                },
            };
            // A Rule books a transaction once; what it booked changes only
            // when the user asks for that with a change of the Rule (Q-044).
            let wanted = match scope {
                Scope::Uncategorized | Scope::Only(_) => part.account.is_none(),
                Scope::BookedBy(booked_by) => {
                    mark.flatten().is_some_and(|rule| booked_by.contains(&rule))
                }
            };
            if !wanted {
                continue;
            }
            let Some(account) = own.account.filter(|&id| is_account(id)) else {
                continue;
            };
            if own.foreign.is_some() {
                continue;
            }
            // What the user sees in the account's register (D-108).
            let facts = RuleFacts {
                account,
                amount: own.amount,
                booking_date: transaction.date_of(own),
                payee: transaction.payee_of(own),
                imported: own.imported.as_ref(),
            };
            let Some(outcome) = rules.evaluate(&facts) else {
                continue;
            };
            let rule = outcome.rule();
            let id = transaction.id();
            let skip = |reason| RuleSkip {
                transaction: id,
                rule,
                reason,
            };
            if outcome.actions.is_empty() {
                continue;
            }
            if suggested.contains(&id) {
                run.skipped.push(skip(StoreError::TransferSuggested(id)));
                continue;
            }
            let rebooked =
                match self.rebook(&transaction, part, account, &outcome.actions, &accounts) {
                    Ok(rebooked) => rebooked,
                    Err(
                        reason @ (StoreError::NoRate { .. }
                        | StoreError::AmountOverflow
                        | StoreError::AccountClosed(_)
                        | StoreError::OriginalChanged(_)
                        | StoreError::Transfer(_)),
                    ) => {
                        run.skipped.push(skip(reason));
                        continue;
                    }
                    Err(error) => return Err(error),
                };
            let changed = rebooked != transaction;
            if changed {
                run.changed.push(RuleChange {
                    transaction: id,
                    rule,
                });
            }
            // A mark naming another Rule now is updated too, so the book
            // says which Rule decided.
            if changed || mark.is_some_and(|mark| mark != Some(rule)) {
                planned.push(Planned {
                    transaction: rebooked,
                    part: part.id,
                    rule,
                });
            }
        }
        Ok((run, planned))
    }

    /// `transaction` with `actions` applied to its `part`, the one the Rules
    /// book; `account` is the user's account the money moves in or out of.
    fn rebook(
        &self,
        transaction: &Transaction,
        part: &Split,
        account: AccountId,
        actions: &Actions,
        accounts: &HashMap<AccountId, Account>,
    ) -> Result<Transaction, StoreError> {
        let mut rebooked = part.clone();
        if let Some(book_to) = actions.book_to {
            let (target, fits) = match book_to {
                BookTo::Category(id) => (id, accounts.get(&id).map(|a| a.kind.is_category())),
                BookTo::Transfer(id) => (id, accounts.get(&id).map(|a| a.kind.is_account())),
            };
            match (book_to, fits) {
                (_, None) => return Err(StoreError::UnknownAccount(target)),
                (BookTo::Category(_), Some(false)) => {
                    return Err(StoreError::RuleNeedsCategory(target));
                }
                (BookTo::Transfer(_), Some(false)) => return Err(StoreError::NotAnAccount(target)),
                _ => {}
            }
            if matches!(book_to, BookTo::Transfer(_)) && target == account {
                return Err(StoreError::Transfer(TransferError::SameAccount(account)));
            }
            // A part already booked there keeps its amounts, so a newer
            // exchange rate never changes what is booked.
            if part.account != Some(target) {
                let closed = |id: AccountId| accounts.get(&id).is_some_and(|a| a.closed);
                if let Some(old) = part.account.filter(|&old| closed(old)) {
                    return Err(StoreError::AccountClosed(old));
                }
                if closed(target) {
                    return Err(StoreError::AccountClosed(target));
                }
                let currency = accounts[&target].currency;
                rebooked.account = Some(target);
                if part.original.is_some() {
                    // A part converted from a withdrawn currency keeps its
                    // amounts and its one rate, the fixed one (D-104), so it
                    // moves only to an account in the same currency.
                    if currency != part.account_amount().currency() {
                        return Err(StoreError::OriginalChanged(part.id));
                    }
                } else {
                    rebooked.foreign = if currency == transaction.currency() {
                        None
                    } else {
                        let (amount, rate) =
                            self.convert(part.amount, currency, transaction.booking_date())?;
                        Some(ForeignAmount {
                            amount,
                            rate: Some(rate),
                        })
                    };
                }
            }
        }
        let payee = actions
            .payee
            .clone()
            .or_else(|| transaction.payee().map(str::to_owned));
        let splits = transaction
            .splits()
            .iter()
            .map(|split| {
                let mut split = if split.id == part.id {
                    rebooked.clone()
                } else {
                    split.clone()
                };
                // The payee a Rule sets is the one the account shows, so a
                // part's own payee goes (D-108).
                if actions.payee.is_some() {
                    split.own_payee = None;
                }
                split
            })
            .collect();
        Transaction::new(transaction.id(), transaction.booking_date(), payee, splits).map_err(
            |error| {
                StoreError::Corrupt(Corruption::InvalidTransaction {
                    transaction: transaction.id(),
                    error,
                })
            },
        )
    }

    /// Writes one transaction the Rules changed, and marks its part.
    fn write_planned(&self, planned: &Planned) -> Result<(), StoreError> {
        let transaction = &planned.transaction;
        let part = transaction
            .splits()
            .iter()
            .find(|split| split.id == planned.part)
            .expect("the part the Rules booked");
        let id = part.id.to_bytes();
        self.conn.execute(
            "UPDATE transactions SET payee = ?2 WHERE id = ?1",
            params![transaction.id().to_bytes(), transaction.payee()],
        )?;
        // A Rule that set the payee took the parts' own payees away.
        for split in transaction.splits() {
            if split.own_payee.is_none() {
                self.conn.execute(
                    "UPDATE splits SET own_payee = NULL WHERE id = ?1 AND own_payee IS NOT NULL",
                    [split.id.to_bytes()],
                )?;
            }
        }
        self.conn.execute(
            "UPDATE splits SET account_id = ?2, foreign_amount = ?3, foreign_currency = ?4
             WHERE id = ?1",
            params![
                id,
                part.account.map(AccountId::to_bytes),
                part.foreign.map(|foreign| foreign.amount.minor_units()),
                part.foreign
                    .map(|foreign| foreign.amount.currency().as_str().to_owned()),
            ],
        )?;
        // Written again even if unchanged: the history records only real
        // changes.
        self.conn
            .execute("DELETE FROM split_rates WHERE split_id = ?1", [id])?;
        if let Some(rate) = split_rate(part) {
            insert_split_rate(&self.conn, part.id, &rate)?;
        }
        self.conn.execute(
            "INSERT INTO split_rules (split_id, rule_id) VALUES (?1, ?2)
             ON CONFLICT (split_id) DO UPDATE SET rule_id = excluded.rule_id",
            params![id, planned.rule.to_bytes()],
        )?;
        Ok(())
    }

    /// The parts Rules booked, with the Rule that decided.
    fn marks(&self) -> Result<HashMap<SplitId, Option<RuleId>>, StoreError> {
        let mut statement = self
            .conn
            .prepare("SELECT split_id, rule_id FROM split_rules")?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    SplitId::from_bytes(row.get(0)?),
                    row.get::<_, Option<[u8; 16]>>(1)?.map(RuleId::from_bytes),
                ))
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// The transactions `scope` may name, each once: those with two parts,
    /// one of them without a category, all or only the ones it names, or
    /// those with a part one of the Rules booked. [`Book::plan`] checks each
    /// one further.
    fn candidates(&self, scope: Scope<'_>) -> Result<Vec<TransactionId>, StoreError> {
        let mut ids = match scope {
            Scope::Uncategorized | Scope::Only(_) => {
                let mut statement = self.conn.prepare(
                    "SELECT transaction_id FROM splits GROUP BY transaction_id
                     HAVING count(*) = 2 AND count(account_id) = 1",
                )?;
                statement
                    .query_map([], |row| Ok(TransactionId::from_bytes(row.get(0)?)))?
                    .collect::<Result<Vec<_>, _>>()?
            }
            Scope::BookedBy(booked_by) => {
                let mut statement = self.conn.prepare(
                    "SELECT s.transaction_id, m.rule_id
                     FROM split_rules AS m JOIN splits AS s ON s.id = m.split_id
                     WHERE m.rule_id IS NOT NULL",
                )?;
                statement
                    .query_map([], |row| {
                        Ok((
                            TransactionId::from_bytes(row.get(0)?),
                            RuleId::from_bytes(row.get(1)?),
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .filter(|(_, rule)| booked_by.contains(rule))
                    .map(|(id, _)| id)
                    .collect()
            }
        };
        if let Scope::Only(only) = scope {
            ids.retain(|id| only.contains(id));
        }
        ids.sort_unstable();
        ids.dedup();
        Ok(ids)
    }

    /// The checks `add_rule`, `update_rule` and `move_rule` share, on the
    /// Rule as it is going to be, inside `parent`.
    fn check_rule(&self, rule: &Rule, parent: Option<RuleId>) -> Result<(), StoreError> {
        rule.check().map_err(StoreError::Rule)?;
        let wanted = rule.name.trim().to_lowercase();
        if wanted.is_empty() {
            return Err(StoreError::EmptyRuleName);
        }
        for sibling in self.sibling_ids(parent)? {
            if sibling != rule.id && self.rule(sibling)?.name.trim().to_lowercase() == wanted {
                return Err(StoreError::DuplicateRuleName { existing: sibling });
            }
        }
        match rule.actions.book_to {
            Some(BookTo::Category(id)) if !self.account(id)?.kind.is_category() => {
                return Err(StoreError::RuleNeedsCategory(id));
            }
            Some(BookTo::Transfer(id)) if !self.account(id)?.kind.is_account() => {
                return Err(StoreError::NotAnAccount(id));
            }
            _ => {}
        }
        for condition in &rule.conditions {
            match condition {
                Condition::Amount(AmountTest::Equals(amount)) => {
                    self.currency_minor_digits(amount.currency())?;
                }
                Condition::Amount(AmountTest::Between { min, .. }) => {
                    self.currency_minor_digits(min.currency())?;
                }
                Condition::Currency(currency) => {
                    self.currency_minor_digits(*currency)?;
                }
                Condition::BookingDate { first, last } => {
                    date_to_text(*first)?;
                    date_to_text(*last)?;
                }
                Condition::Account(id) => {
                    if !self.account(*id)?.kind.is_account() {
                        return Err(StoreError::NotAnAccount(*id));
                    }
                }
                Condition::Amount(AmountTest::MoneyIn | AmountTest::MoneyOut)
                | Condition::Text { .. } => {}
            }
        }
        Ok(())
    }

    /// Writes `rule`'s name, conditions and actions into its row, which
    /// exists. Conditions keep the IDs of the ones at the same position, so
    /// the history records only those that changed.
    fn write_rule(&self, rule: &Rule) -> Result<(), StoreError> {
        let id = rule.id.to_bytes();
        let (category, transfer) = match rule.actions.book_to {
            Some(BookTo::Category(id)) => (Some(id), None),
            Some(BookTo::Transfer(id)) => (None, Some(id)),
            None => (None, None),
        };
        self.conn.execute(
            "UPDATE rules SET name = ?2, match_mode = ?3, category_id = ?4,
                              transfer_account_id = ?5, payee = ?6
             WHERE id = ?1",
            params![
                id,
                rule.name,
                match rule.match_mode {
                    MatchMode::All => "all",
                    MatchMode::Any => "any",
                },
                category.map(AccountId::to_bytes),
                transfer.map(AccountId::to_bytes),
                rule.actions.payee,
            ],
        )?;
        let mut statement = self
            .conn
            .prepare("SELECT id FROM rule_conditions WHERE rule_id = ?1 ORDER BY position")?;
        let mut kept = statement
            .query_map([id], |row| row.get::<_, [u8; 16]>(0))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter();
        drop(statement);
        self.conn
            .execute("DELETE FROM rule_conditions WHERE rule_id = ?1", [id])?;
        let mut insert = self.conn.prepare(&format!(
            "INSERT INTO rule_conditions (id, rule_id, position, {CONDITION_VALUES})
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"
        ))?;
        for (position, condition) in (0_i64..).zip(&rule.conditions) {
            let row = ConditionRow::of(condition)?;
            let condition_id = kept
                .next()
                .unwrap_or_else(|| uuid::Uuid::now_v7().into_bytes());
            insert.execute(params![
                condition_id,
                id,
                position,
                row.field,
                row.operator,
                row.text_value,
                row.case_sensitive,
                row.amount_min,
                row.amount_max,
                row.currency,
                row.date_first,
                row.date_last,
                row.account,
            ])?;
        }
        Ok(())
    }

    /// Gives the Rules in `order` the positions 0, 1, 2 and so on.
    fn write_order(&self, order: &[RuleId]) -> Result<(), StoreError> {
        for (position, id) in (0_i64..).zip(order) {
            self.conn.execute(
                "UPDATE rules SET position = ?2 WHERE id = ?1",
                params![id.to_bytes(), position],
            )?;
        }
        Ok(())
    }

    /// The Rule `id` sits inside, or `None` at the top level.
    fn rule_parent(&self, id: RuleId) -> Result<Option<RuleId>, StoreError> {
        let parent: Option<Option<[u8; 16]>> = self
            .conn
            .query_row(
                "SELECT parent_id FROM rules WHERE id = ?1",
                [id.to_bytes()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(parent
            .ok_or(StoreError::UnknownRule(id))?
            .map(RuleId::from_bytes))
    }

    /// The Rules inside `parent`, or at the top level, in their order.
    fn sibling_ids(&self, parent: Option<RuleId>) -> Result<Vec<RuleId>, StoreError> {
        let mut statement = self
            .conn
            .prepare("SELECT id FROM rules WHERE parent_id IS ?1 ORDER BY position, id")?;
        let ids = statement
            .query_map([parent.map(RuleId::to_bytes)], |row| {
                Ok(RuleId::from_bytes(row.get(0)?))
            })?
            .collect::<Result<_, _>>()?;
        Ok(ids)
    }

    fn rule_count(&self) -> Result<usize, StoreError> {
        let count: i64 = self
            .conn
            .query_row("SELECT count(*) FROM rules", [], |row| row.get(0))?;
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    }

    /// The conditions of the Rule `only`, or of every Rule, by Rule, each
    /// list in its order.
    fn conditions(
        &self,
        only: Option<RuleId>,
    ) -> Result<HashMap<RuleId, Vec<Condition>>, StoreError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT rule_id, {CONDITION_VALUES} FROM rule_conditions
             WHERE ?1 IS NULL OR rule_id = ?1
             ORDER BY rule_id, position"
        ))?;
        let rows = statement
            .query_map([only.map(RuleId::to_bytes)], |row| {
                Ok((RuleId::from_bytes(row.get(0)?), ConditionRow::read(row, 1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut conditions: HashMap<RuleId, Vec<Condition>> = HashMap::new();
        for (rule, row) in rows {
            conditions
                .entry(rule)
                .or_default()
                .push(row.into_condition(rule)?);
        }
        Ok(conditions)
    }
}

/// Builds the list of Rules inside `parent` from `children`, taking the
/// Rules out of it, and counts them in `built`.
fn build_tree(
    children: &mut HashMap<Option<RuleId>, Vec<Rule>>,
    parent: Option<RuleId>,
    built: &mut usize,
) -> Vec<RuleNode> {
    let rules = children.remove(&parent).unwrap_or_default();
    rules
        .into_iter()
        .map(|rule| {
            *built += 1;
            let sub_rules = build_tree(children, Some(rule.id), built);
            RuleNode { rule, sub_rules }
        })
        .collect()
}

/// Puts `rule` in `nodes` in place of the Rule with its ID, and returns that
/// ID with the IDs of all its sub-Rules, or `None` if it isn't there.
fn replace_rule(nodes: &mut [RuleNode], rule: &Rule) -> Option<HashSet<RuleId>> {
    for node in nodes {
        if node.rule.id == rule.id {
            node.rule = rule.clone();
            let mut ids = HashSet::new();
            collect_ids(node, &mut ids);
            return Some(ids);
        }
        if let Some(ids) = replace_rule(&mut node.sub_rules, rule) {
            return Some(ids);
        }
    }
    None
}

/// Adds the ID of `node`'s Rule and of all its sub-Rules to `ids`.
fn collect_ids(node: &RuleNode, ids: &mut HashSet<RuleId>) {
    ids.insert(node.rule.id);
    for sub in &node.sub_rules {
        collect_ids(sub, ids);
    }
}

/// The columns [`RuleRow::read`] expects, in its order.
const RULE_COLUMNS: &str =
    "id, parent_id, position, name, match_mode, category_id, transfer_account_id, payee";

/// A row of `rules` as SQLite returns it, before KassenSturz's checks.
struct RuleRow {
    id: RuleId,
    parent: Option<RuleId>,
    position: i64,
    name: String,
    match_mode: String,
    category: Option<[u8; 16]>,
    transfer: Option<[u8; 16]>,
    payee: Option<String>,
}

impl RuleRow {
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: RuleId::from_bytes(row.get(0)?),
            parent: row.get::<_, Option<[u8; 16]>>(1)?.map(RuleId::from_bytes),
            position: row.get(2)?,
            name: row.get(3)?,
            match_mode: row.get(4)?,
            category: row.get(5)?,
            transfer: row.get(6)?,
            payee: row.get(7)?,
        })
    }

    fn into_rule(self, conditions: Vec<Condition>) -> Result<Rule, StoreError> {
        let bad = || StoreError::Corrupt(Corruption::BadRule(self.id));
        let match_mode = match self.match_mode.as_str() {
            "all" => MatchMode::All,
            "any" => MatchMode::Any,
            _ => return Err(bad()),
        };
        let book_to = match (self.category, self.transfer) {
            (Some(id), None) => Some(BookTo::Category(AccountId::from_bytes(id))),
            (None, Some(id)) => Some(BookTo::Transfer(AccountId::from_bytes(id))),
            (None, None) => None,
            (Some(_), Some(_)) => return Err(bad()),
        };
        Ok(Rule {
            id: self.id,
            name: self.name,
            match_mode,
            conditions,
            actions: Actions {
                book_to,
                payee: self.payee,
            },
        })
    }
}

/// The columns of `rule_conditions` that hold a condition, in the order
/// [`ConditionRow::read`] expects.
const CONDITION_VALUES: &str = "field, operator, text_value, case_sensitive, amount_min, \
                                amount_max, currency, date_first, date_last, account_id";

/// A condition as its row in `rule_conditions` holds it. Only the columns
/// its field and operator need are set (`docs/BOOK_FORMAT.md`).
#[derive(Default)]
struct ConditionRow {
    field: String,
    operator: String,
    text_value: Option<String>,
    case_sensitive: Option<bool>,
    amount_min: Option<i64>,
    amount_max: Option<i64>,
    currency: Option<String>,
    date_first: Option<String>,
    date_last: Option<String>,
    account: Option<[u8; 16]>,
}

const TEXT_FIELDS: [(TextField, &str); 4] = [
    (TextField::OtherPartyName, "other_party_name"),
    (TextField::OtherPartyIban, "other_party_iban"),
    (TextField::RemittanceInformation, "remittance_information"),
    (TextField::Payee, "payee"),
];

const TEXT_OPERATORS: [(TextOperator, &str); 5] = [
    (TextOperator::Contains, "contains"),
    (TextOperator::Equals, "equals"),
    (TextOperator::StartsWith, "starts_with"),
    (TextOperator::EndsWith, "ends_with"),
    (TextOperator::Matches, "matches"),
];

impl ConditionRow {
    fn of(condition: &Condition) -> Result<Self, StoreError> {
        let words = |field: &str, operator: &str| Self {
            field: field.into(),
            operator: operator.into(),
            ..Self::default()
        };
        let amount = |operator, min: Money, max: Money| Self {
            amount_min: Some(min.minor_units()),
            amount_max: Some(max.minor_units()),
            currency: Some(min.currency().as_str().into()),
            ..words("amount", operator)
        };
        Ok(match condition {
            Condition::Text {
                field,
                operator,
                value,
                case_sensitive,
            } => {
                let field = TEXT_FIELDS
                    .iter()
                    .find(|(listed, _)| listed == field)
                    .map(|(_, text)| *text)
                    .expect("every text field is listed");
                let operator = TEXT_OPERATORS
                    .iter()
                    .find(|(listed, _)| listed == operator)
                    .map(|(_, text)| *text)
                    .expect("every text operator is listed");
                Self {
                    text_value: Some(value.clone()),
                    case_sensitive: Some(*case_sensitive),
                    ..words(field, operator)
                }
            }
            Condition::Amount(AmountTest::Equals(value)) => amount("equals", *value, *value),
            Condition::Amount(AmountTest::Between { min, max }) => amount("between", *min, *max),
            Condition::Amount(AmountTest::MoneyIn) => words("amount", "money_in"),
            Condition::Amount(AmountTest::MoneyOut) => words("amount", "money_out"),
            Condition::BookingDate { first, last } => Self {
                date_first: Some(date_to_text(*first)?),
                date_last: Some(date_to_text(*last)?),
                ..words("booking_date", "between")
            },
            Condition::Currency(currency) => Self {
                currency: Some(currency.as_str().into()),
                ..words("currency", "is")
            },
            Condition::Account(account) => Self {
                account: Some(account.to_bytes()),
                ..words("account", "is")
            },
        })
    }

    /// Reads the columns of [`CONDITION_VALUES`], starting at `first`.
    fn read(row: &rusqlite::Row<'_>, first: usize) -> rusqlite::Result<Self> {
        Ok(Self {
            field: row.get(first)?,
            operator: row.get(first + 1)?,
            text_value: row.get(first + 2)?,
            case_sensitive: row.get(first + 3)?,
            amount_min: row.get(first + 4)?,
            amount_max: row.get(first + 5)?,
            currency: row.get(first + 6)?,
            date_first: row.get(first + 7)?,
            date_last: row.get(first + 8)?,
            account: row.get(first + 9)?,
        })
    }

    /// The condition, for the Rule `rule`. Only the columns its field and
    /// operator use may be set.
    fn into_condition(self, rule: RuleId) -> Result<Condition, StoreError> {
        let bad = || StoreError::Corrupt(Corruption::BadRule(rule));
        let currency = |text: &Option<String>| -> Result<CurrencyCode, StoreError> {
            currency_from_text(text.as_deref().ok_or_else(bad)?)
        };
        let set = [
            self.text_value.is_some(),
            self.case_sensitive.is_some(),
            self.amount_min.is_some(),
            self.amount_max.is_some(),
            self.currency.is_some(),
            self.date_first.is_some(),
            self.date_last.is_some(),
            self.account.is_some(),
        ];
        let only = |wanted: &[usize]| {
            set.iter()
                .enumerate()
                .all(|(index, is_set)| *is_set == wanted.contains(&index))
        };
        let text_field = TEXT_FIELDS
            .iter()
            .find(|(_, text)| *text == self.field)
            .map(|(field, _)| *field);
        let condition = match (text_field, self.field.as_str(), self.operator.as_str()) {
            (Some(field), _, operator) if only(&[0, 1]) => {
                let operator = TEXT_OPERATORS
                    .iter()
                    .find(|(_, text)| *text == operator)
                    .map(|(operator, _)| *operator)
                    .ok_or_else(bad)?;
                Condition::Text {
                    field,
                    operator,
                    value: self.text_value.ok_or_else(bad)?,
                    case_sensitive: self.case_sensitive.ok_or_else(bad)?,
                }
            }
            (None, "amount", operator @ ("equals" | "between")) if only(&[2, 3, 4]) => {
                let currency = currency(&self.currency)?;
                let min = Money::from_minor_units(self.amount_min.ok_or_else(bad)?, currency);
                let max = Money::from_minor_units(self.amount_max.ok_or_else(bad)?, currency);
                match operator {
                    "equals" if min == max => Condition::Amount(AmountTest::Equals(min)),
                    "between" => Condition::Amount(AmountTest::Between { min, max }),
                    _ => return Err(bad()),
                }
            }
            (None, "amount", "money_in") if only(&[]) => Condition::Amount(AmountTest::MoneyIn),
            (None, "amount", "money_out") if only(&[]) => Condition::Amount(AmountTest::MoneyOut),
            (None, "booking_date", "between") if only(&[5, 6]) => {
                let date = |text: Option<String>| date_from_text(&text.ok_or_else(bad)?);
                Condition::BookingDate {
                    first: date(self.date_first)?,
                    last: date(self.date_last)?,
                }
            }
            (None, "currency", "is") if only(&[4]) => {
                Condition::Currency(currency(&self.currency)?)
            }
            (None, "account", "is") if only(&[7]) => {
                Condition::Account(AccountId::from_bytes(self.account.ok_or_else(bad)?))
            }
            _ => return Err(bad()),
        };
        Ok(condition)
    }
}

#[cfg(test)]
mod tests;
