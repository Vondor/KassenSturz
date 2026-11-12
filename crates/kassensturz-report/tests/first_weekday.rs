// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The first day of the week is set once for the whole program, as the app
//! does from the user's locale at start (D-740). This file is its own test
//! program, so setting it here changes no other test.

use kassensturz_core::Date;
use kassensturz_report::{Formula, Period, Weekday, first_weekday, set_first_weekday};

fn day(year: i16, month: i8, day: i8) -> Date {
    Date::new(year, month, day).expect("a real day")
}

#[test]
fn the_week_words_follow_the_day_the_app_sets() {
    // Saturday 31 October 2026, the last day of an October figure.
    let figure = Period::new(day(2026, 10, 31), day(2026, 10, 31)).expect("period");
    let span = || {
        Formula::parse("=Rent@\"this week\"")
            .and_then(|formula| formula.span(figure))
            .expect("span")
    };
    assert_eq!(first_weekday(), Weekday::Monday);
    assert_eq!(
        span(),
        Period::new(day(2026, 10, 26), day(2026, 11, 1)).expect("period")
    );
    set_first_weekday(Weekday::Sunday);
    assert_eq!(first_weekday(), Weekday::Sunday);
    assert_eq!(
        span(),
        Period::new(day(2026, 10, 25), day(2026, 10, 31)).expect("period")
    );
}
