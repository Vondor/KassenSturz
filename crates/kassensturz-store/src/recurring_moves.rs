// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Monthly moves between the user's own accounts that the user asked to be
//! joined by themselves (D-749, `docs/RECURRING_MOVES.md`): remembering and
//! removing them, joining the pairs that fit them after an import, the
//! sides that wait for their other half, and joining several suggested
//! Transfers at once ("Join all").
//!
//! A join here is the same join as [`Book::join_transfer`]: both account
//! parts stay exactly as their banks sent them, so no balance and no
//! register changes (D-691); only the two other parts go.

use std::collections::{HashMap, HashSet};

use kassensturz_core::{
    AccountId, Date, MAX_TAGS_ON_TRANSACTION, Money, MoveCandidate, OtherParty, RecurringMove,
    RecurringMoveError, RecurringMoveId, Transaction, TransactionId, TransferSide, compact_iban,
    fits_recurring_move, join_transfer, match_recurring_moves,
};
use rusqlite::{OptionalExtension, params};

use crate::error::StoreError;
use crate::{Action, Book, currency_from_text, date_from_text, date_to_text};

/// The columns [`read_move`] expects, in its order.
const COLUMNS: &str = "id, from_account_id, to_account_id, amount, currency, day_of_month,
                       from_party_iban, from_party_name, created_on";

/// A row of `recurring_moves` as SQLite returns it.
struct MoveRow {
    id: [u8; 16],
    from_account: [u8; 16],
    to_account: [u8; 16],
    amount: i64,
    currency: String,
    day_of_month: u8,
    from_party_iban: Option<String>,
    from_party_name: Option<String>,
    created_on: String,
}

fn read_move(row: &rusqlite::Row<'_>) -> rusqlite::Result<MoveRow> {
    Ok(MoveRow {
        id: row.get(0)?,
        from_account: row.get(1)?,
        to_account: row.get(2)?,
        amount: row.get(3)?,
        currency: row.get(4)?,
        day_of_month: row.get(5)?,
        from_party_iban: row.get(6)?,
        from_party_name: row.get(7)?,
        created_on: row.get(8)?,
    })
}

fn move_from(row: MoveRow) -> Result<RecurringMove, StoreError> {
    Ok(RecurringMove {
        id: RecurringMoveId::from_bytes(row.id),
        from_account: AccountId::from_bytes(row.from_account),
        to_account: AccountId::from_bytes(row.to_account),
        amount: Money::from_minor_units(row.amount, currency_from_text(&row.currency)?),
        day_of_month: row.day_of_month,
        from_party_iban: row.from_party_iban,
        from_party_name: row.from_party_name,
        created_on: date_from_text(&row.created_on)?,
    })
}

/// A monthly move to remember ([`Book::add_recurring_move`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewRecurringMove {
    /// The account the money leaves.
    pub from_account: AccountId,
    /// The account the money arrives at.
    pub to_account: AccountId,
    /// The exact amount that leaves, more than zero.
    pub amount: Money,
    /// The day of the month, 1 to 31.
    pub day_of_month: u8,
    /// The other party the paying side named, as delivered.
    pub from_party_iban: Option<String>,
    /// The other party's name on the paying side, as delivered.
    pub from_party_name: Option<String>,
    /// The day the user remembers it.
    pub created_on: Date,
}

/// A pair joined because it fits a remembered move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecurringJoin {
    /// The move it fits.
    pub recurring_move: RecurringMoveId,
    /// The paying side as it was before the join. The joined Transfer keeps
    /// its transaction's ID.
    pub outgoing: TransferSide,
    /// The receiving side as it was; its transaction is gone, its account
    /// part is in the joined Transfer.
    pub incoming: TransferSide,
}

/// Why sides that fit a remembered move were not joined by themselves.
#[derive(Debug)]
pub enum NotJoined {
    /// Two sides fit equally well, or one side fits two moves: the user
    /// picks.
    Unclear,
    /// The join would lose something the user wrote, or carry more tags
    /// than a transaction keeps ([`StoreError::Transfer`],
    /// [`StoreError::TooManyTags`]).
    Refused(StoreError),
}

/// Sides that fit a remembered move and need the user.
#[derive(Debug)]
pub struct NeedsYou {
    /// The move.
    pub recurring_move: RecurringMoveId,
    /// The sides, sorted by date.
    pub transactions: Vec<TransactionId>,
    /// Why they were not joined.
    pub reason: NotJoined,
}

/// What [`Book::join_recurring_moves`] did, or would do.
#[derive(Debug, Default)]
pub struct RecurringJoins {
    /// The pairs joined, sorted by the paying side's date.
    pub joined: Vec<RecurringJoin>,
    /// The sides left for the user.
    pub needs_you: Vec<NeedsYou>,
}

/// A suggested Transfer that "Join all" left alone, and why.
#[derive(Debug)]
pub struct RefusedJoin {
    /// The side the money leaves.
    pub outgoing: TransactionId,
    /// The side it arrives at.
    pub incoming: TransactionId,
    /// Why: [`StoreError::TransferNotSuggested`], a
    /// [`StoreError::Transfer`] such as a note a join would lose, or
    /// [`StoreError::TooManyTags`].
    pub reason: StoreError,
}

/// What [`Book::join_transfers`] did.
#[derive(Debug, Default)]
pub struct JoinedTransfers {
    /// The joined Transfers, in the order of the pairs given: the paying
    /// side's ID, which each keeps.
    pub joined: Vec<TransactionId>,
    /// The pairs left alone.
    pub refused: Vec<RefusedJoin>,
}

/// Text of nothing but spaces counts as none.
fn given(text: Option<&str>) -> Option<String> {
    text.filter(|text| !text.trim().is_empty())
        .map(str::to_owned)
}

impl Book {
    /// The remembered monthly moves, in the order they were remembered.
    pub fn recurring_moves(&self) -> Result<Vec<RecurringMove>, StoreError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM recurring_moves ORDER BY id"
        ))?;
        let rows = statement
            .query_map([], read_move)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(move_from).collect()
    }

    /// The remembered move `id` ([`StoreError::UnknownRecurringMove`]).
    pub fn recurring_move(&self, id: RecurringMoveId) -> Result<RecurringMove, StoreError> {
        let row = self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM recurring_moves WHERE id = ?1"),
                [id.to_bytes()],
                read_move,
            )
            .optional()?
            .ok_or(StoreError::UnknownRecurringMove(id))?;
        move_from(row)
    }

    /// The move to remember for the two sides `a` and `b`, which the user
    /// is joining (D-749): from the account the money leaves to the one it
    /// arrives at, the exact amount, the day of the month the paying side
    /// was booked on, and the other party its bank named on the paying
    /// account part, as delivered. Nothing is changed.
    ///
    /// The two must be sides that could be joined ([`StoreError::Transfer`]),
    /// in one currency, as [`Book::join_transfer`] needs them.
    pub fn recurring_move_for(
        &self,
        a: TransactionId,
        b: TransactionId,
        today: Date,
    ) -> Result<NewRecurringMove, StoreError> {
        let first = self.transaction(a)?;
        let second = self.transaction(b)?;
        let accounts = self.account_ids()?;
        // Refused as a join would be, so a move is only remembered for a
        // pair that can be joined.
        let joined = join_transfer(&first, &second, |id| accounts.contains(&id))
            .map_err(StoreError::Transfer)?;
        let side = |transaction: &Transaction| {
            TransferSide::of(transaction, |id| accounts.contains(&id)).map_err(StoreError::Transfer)
        };
        let (paying, outgoing, incoming) = if joined.id() == a {
            (&first, side(&first)?, side(&second)?)
        } else {
            (&second, side(&second)?, side(&first)?)
        };
        let party = party_of(paying, &outgoing);
        let amount = incoming.amount;
        Ok(NewRecurringMove {
            from_account: outgoing.account,
            to_account: incoming.account,
            amount,
            day_of_month: u8::try_from(outgoing.booking_date.day()).unwrap_or(1),
            from_party_iban: party.iban,
            from_party_name: party.name,
            created_on: today,
        })
    }

    /// Remembers `new` as a monthly move to join by itself from now on
    /// (D-749); the user action [`Action::AddRecurringMove`]. Returns its
    /// ID.
    ///
    /// Both accounts must be the user's own, never categories
    /// ([`StoreError::NotAnAccount`]), different, in the amount's currency,
    /// the amount more than zero and the day 1 to 31
    /// ([`StoreError::RecurringMove`]). A move with the same accounts,
    /// amount and other party is remembered once
    /// ([`StoreError::RecurringMoveExists`]). The other party is kept as
    /// delivered; text of nothing but spaces counts as none.
    pub fn add_recurring_move(
        &mut self,
        new: &NewRecurringMove,
    ) -> Result<RecurringMoveId, StoreError> {
        let moved = RecurringMove {
            id: RecurringMoveId::generate(),
            from_account: new.from_account,
            to_account: new.to_account,
            amount: new.amount,
            day_of_month: new.day_of_month,
            from_party_iban: given(new.from_party_iban.as_deref()),
            from_party_name: given(new.from_party_name.as_deref()),
            created_on: new.created_on,
        };
        moved.check().map_err(StoreError::RecurringMove)?;
        for id in [moved.from_account, moved.to_account] {
            let account = self.account(id)?;
            if !account.kind.is_account() {
                return Err(StoreError::NotAnAccount(id));
            }
            if account.currency != moved.amount.currency() {
                return Err(StoreError::RecurringMove(RecurringMoveError::OtherCurrency));
            }
        }
        let existing: Option<[u8; 16]> = self
            .conn
            .query_row(
                "SELECT id FROM recurring_moves
                 WHERE from_account_id = ?1 AND to_account_id = ?2
                   AND amount = ?3 AND currency = ?4
                   AND coalesce(from_party_iban, '') = coalesce(?5, '')
                   AND coalesce(from_party_name, '') = coalesce(?6, '')",
                params![
                    moved.from_account.to_bytes(),
                    moved.to_account.to_bytes(),
                    moved.amount.minor_units(),
                    moved.amount.currency().as_str(),
                    moved.from_party_iban,
                    moved.from_party_name,
                ],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            return Err(StoreError::RecurringMoveExists {
                existing: RecurringMoveId::from_bytes(existing),
            });
        }
        let created_on = date_to_text(moved.created_on)?;
        self.as_one_action(Action::AddRecurringMove, |book| {
            book.conn.execute(
                "INSERT INTO recurring_moves (id, from_account_id, to_account_id, amount,
                     currency, day_of_month, from_party_iban, from_party_name, created_on)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    moved.id.to_bytes(),
                    moved.from_account.to_bytes(),
                    moved.to_account.to_bytes(),
                    moved.amount.minor_units(),
                    moved.amount.currency().as_str(),
                    moved.day_of_month,
                    moved.from_party_iban,
                    moved.from_party_name,
                    created_on,
                ],
            )?;
            Ok::<(), StoreError>(())
        })?;
        Ok(moved.id)
    }

    /// Stops joining the move `id` by itself (D-749); the user action
    /// [`Action::RemoveRecurringMove`], which undo takes back. Moves already
    /// joined stay joined; from now on the user is asked about it again.
    pub fn remove_recurring_move(&mut self, id: RecurringMoveId) -> Result<(), StoreError> {
        self.recurring_move(id)?;
        self.as_one_action(Action::RemoveRecurringMove, |book| {
            book.conn
                .execute("DELETE FROM recurring_moves WHERE id = ?1", [id.to_bytes()])?;
            Ok(())
        })
    }

    /// The sides that may be joined by themselves, each with what fits a
    /// remembered move needs: the open ones and those a Rule sorted, never
    /// those the user sorted by hand (D-031, D-691).
    fn move_candidates(&self) -> Result<Vec<(Transaction, MoveCandidate)>, StoreError> {
        Ok(self
            .joinable_sides()?
            .into_iter()
            .map(|(transaction, side)| {
                let party = party_of(&transaction, &side);
                (transaction, MoveCandidate { side, party })
            })
            .collect())
    }

    /// Each of the user's accounts' own IBANs, without spaces and in
    /// capitals: the number the user gave it and its bank link's IBAN.
    fn own_ibans(&self) -> Result<HashMap<AccountId, Vec<String>>, StoreError> {
        let mut ibans: HashMap<AccountId, Vec<String>> = HashMap::new();
        for account in self.accounts(true)? {
            if let Some(number) = account.number.as_deref().filter(|n| !n.trim().is_empty()) {
                ibans
                    .entry(account.id)
                    .or_default()
                    .push(compact_iban(number));
            }
        }
        for link in self.bank_links()? {
            if let Some(iban) = link.iban {
                ibans
                    .entry(link.account)
                    .or_default()
                    .push(compact_iban(&iban));
            }
        }
        Ok(ibans)
    }

    /// The transactions that are a side of a remembered move whose other
    /// half may still come (D-749, 3.5): a Rule leaves them alone
    /// ([`StoreError::FitsMonthlyMove`]), so the move isn't booked twice.
    pub(crate) fn waiting_for_moves(&self) -> Result<HashSet<TransactionId>, StoreError> {
        let moves = self.recurring_moves()?;
        if moves.is_empty() {
            return Ok(HashSet::new());
        }
        let ibans = self.own_ibans()?;
        let none = Vec::new();
        Ok(self
            .move_candidates()?
            .into_iter()
            .filter(|(_, candidate)| {
                moves.iter().any(|moved| {
                    fits_recurring_move(
                        moved,
                        candidate,
                        ibans.get(&moved.to_account).unwrap_or(&none),
                    )
                })
            })
            .map(|(transaction, _)| transaction.id())
            .collect())
    }

    /// Joins the pairs that fit remembered moves (D-749), as one user
    /// action of its own, [`Action::JoinRecurringMoves`]: one undo takes
    /// back the joins and leaves the transactions as they were before,
    /// categories and Rule marks included. Nothing is recorded when nothing
    /// fits.
    ///
    /// With `involving`, only pairs with at least one side among those
    /// transactions are joined, e.g. the ones an import added, so a pair the
    /// user left unjoined before is not joined behind their back; sides of
    /// pairs that need the user are reported the same way.
    ///
    /// Which pairs fit is [`match_recurring_moves`]'s decision. A pair whose
    /// join would lose something the user wrote, or carry too many tags, is
    /// not joined but reported in [`RecurringJoins::needs_you`], as are
    /// sides that can't be told apart.
    pub fn join_recurring_moves(
        &mut self,
        involving: Option<&HashSet<TransactionId>>,
    ) -> Result<RecurringJoins, StoreError> {
        let moves = self.recurring_moves()?;
        if moves.is_empty() {
            return Ok(RecurringJoins::default());
        }
        let ibans = self.own_ibans()?;
        let candidates = self.move_candidates()?;
        let transactions: HashMap<TransactionId, &Transaction> = candidates
            .iter()
            .map(|(transaction, _)| (transaction.id(), transaction))
            .collect();
        let sides: Vec<MoveCandidate> = candidates
            .iter()
            .map(|(_, candidate)| candidate.clone())
            .collect();
        let found = match_recurring_moves(&moves, &sides, &ibans);
        let wanted = |ids: &[TransactionId]| {
            involving.is_none_or(|involving| ids.iter().any(|id| involving.contains(id)))
        };
        let accounts = self.account_ids()?;
        let mut result = RecurringJoins::default();
        for unclear in found.unclear {
            if wanted(&unclear.transactions) {
                result.needs_you.push(NeedsYou {
                    recurring_move: unclear.recurring_move,
                    transactions: unclear.transactions,
                    reason: NotJoined::Unclear,
                });
            }
        }
        for found in found.matches {
            let (outgoing, incoming) = (found.pair.outgoing, found.pair.incoming);
            if !wanted(&[outgoing.transaction, incoming.transaction]) {
                continue;
            }
            let refusal = match join_transfer(
                transactions[&outgoing.transaction],
                transactions[&incoming.transaction],
                |id| accounts.contains(&id),
            ) {
                Err(error) => Some(StoreError::Transfer(error)),
                Ok(joined) if joined.tags().len() > MAX_TAGS_ON_TRANSACTION => {
                    Some(StoreError::TooManyTags {
                        limit: "transaction",
                        max: MAX_TAGS_ON_TRANSACTION,
                    })
                }
                Ok(_) => None,
            };
            match refusal {
                Some(error) => result.needs_you.push(NeedsYou {
                    recurring_move: found.recurring_move,
                    transactions: vec![outgoing.transaction, incoming.transaction],
                    reason: NotJoined::Refused(error),
                }),
                None => result.joined.push(RecurringJoin {
                    recurring_move: found.recurring_move,
                    outgoing,
                    incoming,
                }),
            }
        }
        if result.joined.is_empty() {
            return Ok(result);
        }
        let pairs: Vec<(TransactionId, TransactionId)> = result
            .joined
            .iter()
            .map(|join| (join.outgoing.transaction, join.incoming.transaction))
            .collect();
        self.as_one_action(Action::JoinRecurringMoves, |book| {
            for (outgoing, incoming) in pairs {
                book.join_transfer(outgoing, incoming)?;
            }
            Ok::<(), StoreError>(())
        })?;
        Ok(result)
    }

    /// Joins the suggested Transfers `pairs` at once ("Join all", D-749,
    /// 3.6), as one user action, [`Action::JoinTransfers`], which one undo
    /// takes back. Each pair joins as [`Book::join_transfer`] joins it, so
    /// no balance changes.
    ///
    /// Only pairs the book suggests right now ([`Book::transfer_suggestions`],
    /// in either order) are joined, so a side the user sorted by hand never
    /// is; any other pair is left alone
    /// ([`StoreError::TransferNotSuggested`]), and so is one whose join would
    /// lose something the user wrote or carry too many tags. Those are
    /// listed in [`JoinedTransfers::refused`]; the rest join. Nothing is
    /// recorded when nothing joins.
    pub fn join_transfers(
        &mut self,
        pairs: &[(TransactionId, TransactionId)],
    ) -> Result<JoinedTransfers, StoreError> {
        let suggested: HashSet<(TransactionId, TransactionId)> = self
            .transfer_suggestions()?
            .into_iter()
            .map(|found| (found.outgoing.transaction, found.incoming.transaction))
            .collect();
        let accounts = self.account_ids()?;
        let mut result = JoinedTransfers::default();
        let mut to_join = Vec::new();
        let mut used = HashSet::new();
        for &(a, b) in pairs {
            let (outgoing, incoming) = if suggested.contains(&(a, b)) {
                (a, b)
            } else if suggested.contains(&(b, a)) {
                (b, a)
            } else {
                result.refused.push(RefusedJoin {
                    outgoing: a,
                    incoming: b,
                    reason: StoreError::TransferNotSuggested {
                        outgoing: a,
                        incoming: b,
                    },
                });
                continue;
            };
            // The same pair named twice joins once.
            if !used.insert(outgoing) {
                continue;
            }
            let joined = join_transfer(
                &self.transaction(outgoing)?,
                &self.transaction(incoming)?,
                |id| accounts.contains(&id),
            );
            let refusal = match joined {
                Err(error) => Some(StoreError::Transfer(error)),
                Ok(joined) if joined.tags().len() > MAX_TAGS_ON_TRANSACTION => {
                    Some(StoreError::TooManyTags {
                        limit: "transaction",
                        max: MAX_TAGS_ON_TRANSACTION,
                    })
                }
                Ok(_) => None,
            };
            match refusal {
                Some(reason) => result.refused.push(RefusedJoin {
                    outgoing,
                    incoming,
                    reason,
                }),
                None => to_join.push((outgoing, incoming)),
            }
        }
        if to_join.is_empty() {
            return Ok(result);
        }
        result.joined = self.as_one_action(Action::JoinTransfers, |book| {
            to_join
                .iter()
                .map(|&(outgoing, incoming)| book.join_transfer(outgoing, incoming))
                .collect::<Result<Vec<_>, StoreError>>()
        })?;
        Ok(result)
    }
}

/// The other party the bank named on the account part of `side`.
fn party_of(transaction: &Transaction, side: &TransferSide) -> OtherParty {
    transaction
        .splits()
        .iter()
        .find(|split| split.account == Some(side.account))
        .and_then(|split| split.imported.as_ref())
        .map_or_else(OtherParty::default, |imported| {
            OtherParty::new(
                imported.other_party_iban.as_deref(),
                imported.other_party_name.as_deref(),
            )
        })
}

#[cfg(test)]
mod tests;
