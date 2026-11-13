// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Noticing that the day, and with it perhaps the month, has changed while
//! the app stayed open (#831).
//!
//! "Today" is read from the clock each time a screen asks for it, but a
//! screen only asks when it is made or when the book changes. An app left
//! open over midnight therefore kept showing the figures of the old day.
//! The app asks [`DayWatch`] now and then; when the date has moved, every
//! screen that depends on "today" reads again.

use jiff::civil::Date;

/// What a new reading of the clock says compared with the one before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayChange {
    /// The same day as before, or the first reading.
    None,
    /// A later or earlier day in the same month.
    Day,
    /// Another month (or year).
    Month,
}

/// The date at the last reading of the clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct DayWatch {
    seen: Option<Date>,
}

impl DayWatch {
    /// A watch that has not read the clock yet.
    #[must_use]
    pub const fn new() -> Self {
        Self { seen: None }
    }

    /// Notes today's date as the clock gives it. The first reading is not a
    /// change: the screens were just made with it. A clock set back counts
    /// as a change too, since the figures shown no longer fit it.
    pub fn note(&mut self, today: Date) -> DayChange {
        let before = self.seen.replace(today);
        match before {
            Some(before) if before == today => DayChange::None,
            Some(before) if (before.year(), before.month()) == (today.year(), today.month()) => {
                DayChange::Day
            }
            Some(_) => DayChange::Month,
            None => DayChange::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clock_moved_over_a_month_end_is_noticed_once() {
        let mut watch = DayWatch::new();
        assert_eq!(watch.note(Date::constant(2026, 11, 30)), DayChange::None);
        assert_eq!(watch.note(Date::constant(2026, 11, 30)), DayChange::None);
        assert_eq!(watch.note(Date::constant(2026, 12, 1)), DayChange::Month);
        assert_eq!(watch.note(Date::constant(2026, 12, 1)), DayChange::None);
    }

    #[test]
    fn a_new_day_in_the_same_month_is_a_day_change() {
        let mut watch = DayWatch::new();
        watch.note(Date::constant(2026, 11, 12));
        assert_eq!(watch.note(Date::constant(2026, 11, 13)), DayChange::Day);
    }

    #[test]
    fn the_same_month_of_another_year_is_a_month_change() {
        let mut watch = DayWatch::new();
        watch.note(Date::constant(2026, 12, 31));
        assert_eq!(watch.note(Date::constant(2027, 1, 1)), DayChange::Month);
        watch.note(Date::constant(2027, 1, 20));
        assert_eq!(watch.note(Date::constant(2028, 1, 20)), DayChange::Month);
    }

    #[test]
    fn a_clock_set_back_counts_as_a_change() {
        let mut watch = DayWatch::new();
        watch.note(Date::constant(2026, 12, 2));
        assert_eq!(watch.note(Date::constant(2026, 11, 2)), DayChange::Month);
    }
}
