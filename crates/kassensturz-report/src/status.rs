// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shared status of a tile that shrinks to an icon (US5.4, D-734,
//! `docs/STATUS_TILES.md`): five states, the order in which they win when
//! several apply, and one pure function per status tile from the figures the
//! tile already has. The app draws the state (shape, mark, colour, words);
//! this module only decides it, so the decision is tested in one place.

/// The state a status tile shows (D-734).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TileStatus {
    /// `fine`: nothing to do.
    Fine,
    /// `unknown`: the state can't be told, or nothing is set up yet. Never
    /// shown as fine.
    Unknown,
    /// `to-do`: something waits for the user.
    ToDo,
    /// `failed`: the last attempt did not work.
    Failed,
    /// `running`: work is under way right now.
    Running,
}

impl TileStatus {
    /// The fixed word the app reads.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Fine => "fine",
            Self::Unknown => "unknown",
            Self::ToDo => "to-do",
            Self::Failed => "failed",
            Self::Running => "running",
        }
    }

    /// The state that wins when both apply: `running` > `failed` > `to-do`
    /// > `unknown` > `fine` (the order of the variants).
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        self.max(other)
    }
}

/// A tile's state and the count its words name (how many warnings apply,
/// how many transactions wait, how many accounts failed). The count is 0
/// where the words name none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    /// The state.
    pub status: TileStatus,
    /// The count for the words.
    pub count: usize,
}

impl Status {
    const fn new(status: TileStatus, count: usize) -> Self {
        Self { status, count }
    }
}

/// The figures the Warnings tile (`notice.alarms`, D-721) has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AlarmsFigures {
    /// How many rules the tile has.
    pub rules: usize,
    /// How many are triggered, or name a budget or account that is gone.
    pub to_do: usize,
    /// How many could not be compared (D-722 point 5).
    pub cant_check: usize,
}

/// The Warnings tile's status. `None` when its rules could not be read.
pub const fn alarms(figures: Option<AlarmsFigures>) -> Status {
    match figures {
        None => Status::new(TileStatus::Failed, 0),
        Some(f) if f.to_do > 0 => Status::new(TileStatus::ToDo, f.to_do),
        Some(f) if f.rules == 0 => Status::new(TileStatus::Unknown, 0),
        Some(f) if f.cant_check > 0 => Status::new(TileStatus::Unknown, f.cant_check),
        Some(f) => Status::new(TileStatus::Fine, f.rules),
    }
}

/// The status of a tile that counts things waiting for the user: the
/// transactions without a category (`notice.unsorted`, D-637) or the bank
/// accounts not updated for a long time (`notice.fetch_gap`, D-328).
pub const fn waiting(count: usize) -> Status {
    if count > 0 {
        Status::new(TileStatus::ToDo, count)
    } else {
        Status::new(TileStatus::Fine, 0)
    }
}

/// How the last *Update all accounts* of this session ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateRun {
    /// It failed as a whole (the bank object returned an error).
    Failed,
    /// It ran: how many accounts failed and how many notices wait.
    Done {
        /// Accounts whose update failed.
        failed: usize,
        /// Notices to check.
        to_check: usize,
    },
}

/// The figures the *Update all accounts* tile (`bank.update_all`) has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpdateFigures {
    /// Whether bank connections can be used here at all.
    pub usable: bool,
    /// How many accounts are linked to a bank.
    pub linked: usize,
    /// Whether no linked account was ever updated.
    pub never: bool,
    /// Whether an update runs right now.
    pub running: bool,
    /// How the last update of this session ended, if one ran.
    pub last: Option<UpdateRun>,
}

/// The *Update all accounts* tile's status.
pub const fn update_all(f: UpdateFigures) -> Status {
    if f.running {
        return Status::new(TileStatus::Running, 0);
    }
    if !f.usable || f.linked == 0 {
        return Status::new(TileStatus::Unknown, 0);
    }
    match f.last {
        Some(UpdateRun::Failed) => Status::new(TileStatus::Failed, 0),
        Some(UpdateRun::Done { failed, .. }) if failed > 0 => {
            Status::new(TileStatus::Failed, failed)
        }
        Some(UpdateRun::Done { to_check, .. }) if to_check > 0 => {
            Status::new(TileStatus::ToDo, to_check)
        }
        Some(UpdateRun::Done { .. }) => Status::new(TileStatus::Fine, 0),
        None if f.never => Status::new(TileStatus::Unknown, 0),
        None => Status::new(TileStatus::Fine, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_and_the_order_are_fixed() {
        use TileStatus::*;
        let all = [Fine, Unknown, ToDo, Failed, Running];
        let words: Vec<_> = all.iter().map(|s| s.id()).collect();
        assert_eq!(words, ["fine", "unknown", "to-do", "failed", "running"]);
        for (i, a) in all.iter().enumerate() {
            for b in &all[i..] {
                assert_eq!(a.and(*b), *b);
                assert_eq!(b.and(*a), *b);
            }
        }
    }

    #[test]
    fn warnings() {
        let f = |rules, to_do, cant_check| {
            alarms(Some(AlarmsFigures {
                rules,
                to_do,
                cant_check,
            }))
        };
        assert_eq!(alarms(None), Status::new(TileStatus::Failed, 0));
        assert_eq!(f(0, 0, 0), Status::new(TileStatus::Unknown, 0));
        assert_eq!(f(3, 0, 0), Status::new(TileStatus::Fine, 3));
        assert_eq!(f(3, 2, 0), Status::new(TileStatus::ToDo, 2));
        // Something to do wins over a rule that can't be checked; that
        // rule alone is never fine.
        assert_eq!(f(3, 1, 1), Status::new(TileStatus::ToDo, 1));
        assert_eq!(f(3, 0, 1), Status::new(TileStatus::Unknown, 1));
    }

    #[test]
    fn counts_that_wait() {
        assert_eq!(waiting(0), Status::new(TileStatus::Fine, 0));
        assert_eq!(waiting(5), Status::new(TileStatus::ToDo, 5));
    }

    #[test]
    fn update_all_accounts() {
        let base = UpdateFigures {
            usable: true,
            linked: 2,
            never: false,
            running: false,
            last: None,
        };
        let s = |f| update_all(f).status;
        assert_eq!(s(base), TileStatus::Fine);
        assert_eq!(
            s(UpdateFigures {
                never: true,
                ..base
            }),
            TileStatus::Unknown
        );
        assert_eq!(s(UpdateFigures { linked: 0, ..base }), TileStatus::Unknown);
        assert_eq!(
            s(UpdateFigures {
                usable: false,
                ..base
            }),
            TileStatus::Unknown
        );
        let done = |failed, to_check| UpdateFigures {
            last: Some(UpdateRun::Done { failed, to_check }),
            ..base
        };
        assert_eq!(update_all(done(0, 0)), Status::new(TileStatus::Fine, 0));
        assert_eq!(update_all(done(0, 3)), Status::new(TileStatus::ToDo, 3));
        assert_eq!(update_all(done(1, 3)), Status::new(TileStatus::Failed, 1));
        let failed = UpdateFigures {
            last: Some(UpdateRun::Failed),
            ..base
        };
        assert_eq!(s(failed), TileStatus::Failed);
        // Running wins over everything, also over a failed last run.
        assert_eq!(
            s(UpdateFigures {
                running: true,
                ..failed
            }),
            TileStatus::Running
        );
    }
}
