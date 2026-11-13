// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Planned payments as a QML object (US7.1, D-769, `docs/SCHEDULES.md`
//! part 2), on the book the app has open (`session.rs`). The work is in
//! `planned_payments.rs`; this file turns it into the plain values QML
//! reads. Every amount and date is text (D-065).

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QMap, QMapPair_QString_QVariant, QString, QVariant};

use kassensturz_report::status;

use crate::errors::ErrorText;
use crate::number_format::app_locale;
use crate::planned_payments::{
    self, COMING_UP_DAYS, EntryChoice, MonthRow, MonthText, PaymentText, PlanForm, PlanText,
    PlansError, TABLE_MONTHS,
};
use crate::register::Shown;
use crate::register_model::{field, flag};
use crate::session;
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The planned payments of the open book. `view` holds an object
        /// with `plans` (each with `id`, `name`, `account`, `accountId`,
        /// `amount`, `exactAmount`, `outgoing`, `unit`, `every`,
        /// `firstDue`, `ends`, `nextDue`, `paused`, `accountClosed`,
        /// `category` and `party`), `toConfirm` (the payments whose day
        /// passed without an entry, "Did this happen?") and `comingUp` (the
        /// open ones of the next 30 days), both lists of payments with
        /// `planId`, `name`, `account`, `amount`, `exactAmount`,
        /// `outgoing`, `due`, `dueOn` and `state`; `comingUpStatus` and
        /// `comingUpCount` (the *Coming up* tile's state, D-734); and
        /// `forecast`, *Left this month* (D-770): `accounts` (names),
        /// `leftOut`, `hasPlans`, `balanceNow`, `outDue`, `inDue`, `left`,
        /// `expectedEnd` (each amount also as `exact…`; `inDue` and
        /// `expectedEnd` empty when no income is still to come),
        /// `belowZero`, `overdue`, `status`, `statusCount` and `months`
        /// (each with `month`, `start`, `plannedIn`, `plannedOut` and
        /// `end`). Or an error object.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type PlannedPayments = super::PlannedPaymentsRust;
    }

    extern "RustQt" {
        /// The book was changed here: every screen reads it again.
        #[qsignal]
        fn changed(self: Pin<&mut PlannedPayments>);
    }

    extern "RustQt" {
        /// Reads the open book again.
        #[qinvokable]
        fn refresh(self: Pin<&mut PlannedPayments>);

        /// The form for the plan `id`, or for a new plan on the account
        /// `account_id` when `id` is empty: an object with `name`,
        /// `accountId`, `outgoing`, `amount`, `categoryId`, `unit`,
        /// `every`, `firstDue`, `ends`, `tolerance`, `party` and
        /// `fromEntry`, all text but `outgoing`; or an error object.
        #[qinvokable]
        fn form(self: &PlannedPayments, id: &QString, account_id: &QString) -> QVariant;

        /// The form "Plan this again…" opens for the entry
        /// `transaction_id` in the account `account_id`, named
        /// `fallback_name` when the entry names nobody; an empty object
        /// when no plan could ever match that entry (a Transfer between
        /// the user's own accounts); or an error object.
        #[qinvokable]
        #[cxx_name = "formFromEntry"]
        fn form_from_entry(
            self: &PlannedPayments,
            transaction_id: &QString,
            account_id: &QString,
            fallback_name: &QString,
        ) -> QVariant;

        /// The next three days the form would plan, as a list of dates;
        /// empty while the form can't be read.
        #[qinvokable]
        #[cxx_name = "nextDates"]
        fn next_dates(self: &PlannedPayments, form: &QVariant) -> QVariant;

        /// Saves the form as the plan `id`, or as a new plan when `id` is
        /// empty. Returns an object with the plan's `id`, or an error
        /// object.
        #[qinvokable]
        fn save(self: Pin<&mut PlannedPayments>, id: &QString, form: &QVariant) -> QVariant;

        /// Pauses the plan `id`, or lets it go on. Returns an empty object
        /// or an error object.
        #[qinvokable]
        #[cxx_name = "setPaused"]
        fn set_paused(self: Pin<&mut PlannedPayments>, id: &QString, paused: bool) -> QVariant;

        /// Removes the plan `id`; its entries stay. Returns an empty object
        /// or an error object.
        #[qinvokable]
        fn remove(self: Pin<&mut PlannedPayments>, id: &QString) -> QVariant;

        /// "Skip this time" for the payment of `plan_id` with `due_on`.
        #[qinvokable]
        fn skip(self: Pin<&mut PlannedPayments>, plan_id: &QString, due_on: &QString) -> QVariant;

        /// The form "Book it" opens: an object with `amount` and `date`,
        /// or an error object.
        #[qinvokable]
        #[cxx_name = "bookForm"]
        fn book_form(self: &PlannedPayments, plan_id: &QString) -> QVariant;

        /// "Book it": a new entry for the payment of `plan_id` with
        /// `due_on`, on `date` with `amount`, as typed. Returns an object
        /// with the entry's `id`, or an error object.
        #[qinvokable]
        #[cxx_name = "bookIt"]
        fn book_it(
            self: Pin<&mut PlannedPayments>,
            plan_id: &QString,
            due_on: &QString,
            date: &QString,
            amount: &QString,
        ) -> QVariant;

        /// The entries "Match an entry…" offers for the payment of
        /// `plan_id` with `due_on`, closest first: a list of `id`, `date`,
        /// `payee`, `amount` and `exactAmount`; or an error object.
        #[qinvokable]
        #[cxx_name = "entryChoices"]
        fn entry_choices(self: &PlannedPayments, plan_id: &QString, due_on: &QString) -> QVariant;

        /// Matches the entry `transaction_id` to the payment of `plan_id`
        /// with `due_on`. Returns an empty object or an error object.
        #[qinvokable]
        #[cxx_name = "matchEntry"]
        fn match_entry(
            self: Pin<&mut PlannedPayments>,
            plan_id: &QString,
            due_on: &QString,
            transaction_id: &QString,
        ) -> QVariant;
    }
}

/// The Rust side of [`qobject::PlannedPayments`].
#[derive(Default)]
pub struct PlannedPaymentsRust {
    view: QVariant,
}

/// Runs `read` on the open book, as a value for QML.
fn reading<T>(
    read: impl FnOnce(&kassensturz_store::Book) -> Result<T, PlansError>,
    shown: impl FnOnce(T) -> QVariant,
) -> QVariant {
    let locale = app_locale();
    let found = session::with_book(|book| {
        read(book).map_err(|error| planned_payments::error_text(book, &error, &locale))
    });
    match found {
        Some(Ok(value)) => shown(value),
        Some(Err(error)) => error_object(&error),
        None => error_object(&ErrorText::plain("input.no-such-entry")),
    }
}

impl qobject::PlannedPayments {
    fn refresh(mut self: Pin<&mut Self>) {
        let locale = app_locale();
        let view = reading(
            |book| {
                Ok((
                    planned_payments::list(book, &locale)?,
                    planned_payments::did_this_happen(book, &locale)?,
                    planned_payments::coming_up(book, COMING_UP_DAYS, &locale)?,
                    planned_payments::this_month(book, TABLE_MONTHS, &locale)?,
                ))
            },
            |(plans, to_confirm, coming_up, month)| {
                view_object(&plans, &to_confirm, &coming_up, &month)
            },
        );
        self.as_mut().rust_mut().view = view;
        self.as_mut().view_changed();
    }

    fn form(&self, id: &QString, account_id: &QString) -> QVariant {
        let (id, account_id) = (id.to_string(), account_id.to_string());
        let locale = app_locale();
        if id.is_empty() {
            return form_object(&planned_payments::new_form(&account_id, &locale));
        }
        reading(
            |book| planned_payments::edit_form(book, &id, &locale),
            |form| form_object(&form),
        )
    }

    fn form_from_entry(
        &self,
        transaction_id: &QString,
        account_id: &QString,
        fallback_name: &QString,
    ) -> QVariant {
        let (transaction, account, fallback) = (
            transaction_id.to_string(),
            account_id.to_string(),
            fallback_name.to_string(),
        );
        let locale = app_locale();
        reading(
            |book| {
                planned_payments::form_from_entry(book, &transaction, &account, &fallback, &locale)
            },
            |form| form.as_ref().map_or_else(|| map([]), form_object),
        )
    }

    fn next_dates(&self, form: &QVariant) -> QVariant {
        let form = read_form(form);
        let locale = app_locale();
        session::with_book(|book| planned_payments::next_dates(book, &form, &locale))
            .map_or_else(|| list([]), |dates| list(dates.iter().map(|day| text(day))))
    }

    fn save(self: Pin<&mut Self>, id: &QString, form: &QVariant) -> QVariant {
        let (id, form) = (id.to_string(), read_form(form));
        self.changing(|book, locale| {
            let saved = planned_payments::save(book, &id, &form, locale)?;
            Ok(map([("id", text(&saved))]))
        })
    }

    fn set_paused(self: Pin<&mut Self>, id: &QString, paused: bool) -> QVariant {
        let id = id.to_string();
        self.changing(|book, _| {
            planned_payments::set_paused(book, &id, paused)?;
            Ok(map([]))
        })
    }

    fn remove(self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        self.changing(|book, _| {
            planned_payments::remove(book, &id)?;
            Ok(map([]))
        })
    }

    fn skip(self: Pin<&mut Self>, plan_id: &QString, due_on: &QString) -> QVariant {
        let (plan, due_on) = (plan_id.to_string(), due_on.to_string());
        self.changing(|book, _| {
            planned_payments::skip(book, &plan, &due_on)?;
            Ok(map([]))
        })
    }

    fn book_form(&self, plan_id: &QString) -> QVariant {
        let plan = plan_id.to_string();
        let locale = app_locale();
        reading(
            |book| planned_payments::book_form(book, &plan, &locale),
            |(amount, date)| map([("amount", text(&amount)), ("date", text(&date))]),
        )
    }

    fn book_it(
        self: Pin<&mut Self>,
        plan_id: &QString,
        due_on: &QString,
        date: &QString,
        amount: &QString,
    ) -> QVariant {
        let (plan, due_on, date, amount) = (
            plan_id.to_string(),
            due_on.to_string(),
            date.to_string(),
            amount.to_string(),
        );
        self.changing(|book, locale| {
            let booked = planned_payments::book_it(book, &plan, &due_on, &date, &amount, locale)?;
            Ok(map([("id", text(&booked))]))
        })
    }

    fn entry_choices(&self, plan_id: &QString, due_on: &QString) -> QVariant {
        let (plan, due_on) = (plan_id.to_string(), due_on.to_string());
        let locale = app_locale();
        reading(
            |book| planned_payments::entry_choices(book, &plan, &due_on, &locale),
            |choices| list(choices.iter().map(choice_object)),
        )
    }

    fn match_entry(
        self: Pin<&mut Self>,
        plan_id: &QString,
        due_on: &QString,
        transaction_id: &QString,
    ) -> QVariant {
        let (plan, due_on, transaction) = (
            plan_id.to_string(),
            due_on.to_string(),
            transaction_id.to_string(),
        );
        self.changing(|book, _| {
            planned_payments::match_entry(book, &plan, &due_on, &transaction)?;
            Ok(map([]))
        })
    }

    /// Changes the open book with `change`; when it worked, reads it again
    /// and tells the screens.
    fn changing(
        mut self: Pin<&mut Self>,
        change: impl FnOnce(
            &mut kassensturz_store::Book,
            &crate::number_format::UserLocale<crate::locale::QtLocale>,
        ) -> Result<QVariant, PlansError>,
    ) -> QVariant {
        let locale = app_locale();
        let result = session::with_book_mut(|book| {
            change(book, &locale)
                .map_err(|error| planned_payments::error_text(book, &error, &locale))
        })
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")));
        match result {
            Ok(value) => {
                self.as_mut().refresh();
                self.as_mut().changed();
                value
            }
            Err(error) => error_object(&error),
        }
    }
}

/// The form as QML sends it back.
fn read_form(form: &QVariant) -> PlanForm {
    let Some(form) = form.value::<QMap<QMapPair_QString_QVariant>>() else {
        return PlanForm::default();
    };
    PlanForm {
        name: field(&form, "name"),
        account_id: field(&form, "accountId"),
        outgoing: flag(&form, "outgoing"),
        amount: field(&form, "amount"),
        category_id: field(&form, "categoryId"),
        unit: field(&form, "unit"),
        every: field(&form, "every"),
        first_due: field(&form, "firstDue"),
        ends: field(&form, "ends"),
        tolerance: field(&form, "tolerance"),
        party: field(&form, "party"),
        from_entry: field(&form, "fromEntry"),
    }
}

fn view_object(
    plans: &[PlanText],
    to_confirm: &[PaymentText],
    coming_up: &[PaymentText],
    month: &MonthText,
) -> QVariant {
    let soon = status::coming_up(Some(plans.len()), coming_up.len());
    map([
        ("plans", list(plans.iter().map(plan_object))),
        ("toConfirm", list(to_confirm.iter().map(payment_object))),
        ("comingUp", list(coming_up.iter().map(payment_object))),
        ("comingUpStatus", text(soon.status.id())),
        ("comingUpCount", count(soon.count)),
        ("forecast", month_object(month)),
    ])
}

/// A count for QML, which reads numbers as `int`.
fn count(value: usize) -> QVariant {
    QVariant::from(&i32::try_from(value).unwrap_or(i32::MAX))
}

/// *Left this month*, as the tile and the window show it.
fn month_object(month: &MonthText) -> QVariant {
    let empty = Shown::default();
    let in_due = month.in_due.as_ref().unwrap_or(&empty);
    let expected_end = month.expected_end.as_ref().unwrap_or(&empty);
    map([
        (
            "accounts",
            list(month.accounts.iter().map(|name| text(name))),
        ),
        ("leftOut", count(month.left_out)),
        ("hasPlans", QVariant::from(&month.has_plans)),
        ("balanceNow", text(&month.balance_now.text)),
        ("exactBalanceNow", text(&month.balance_now.exact)),
        ("outDue", text(&month.out_due.text)),
        ("exactOutDue", text(&month.out_due.exact)),
        ("inDue", text(&in_due.text)),
        ("exactInDue", text(&in_due.exact)),
        ("left", text(&month.left.text)),
        ("exactLeft", text(&month.left.exact)),
        ("belowZero", QVariant::from(&month.below_zero)),
        ("expectedEnd", text(&expected_end.text)),
        ("exactExpectedEnd", text(&expected_end.exact)),
        ("overdue", count(month.overdue)),
        ("status", text(month.status.status.id())),
        ("statusCount", count(month.status.count)),
        ("months", list(month.months.iter().map(month_row_object))),
    ])
}

/// One month of the month by month table.
fn month_row_object(row: &MonthRow) -> QVariant {
    map([
        ("month", text(&row.month)),
        ("start", text(&row.start.text)),
        ("exactStart", text(&row.start.exact)),
        ("plannedIn", text(&row.planned_in.text)),
        ("exactPlannedIn", text(&row.planned_in.exact)),
        ("plannedOut", text(&row.planned_out.text)),
        ("exactPlannedOut", text(&row.planned_out.exact)),
        ("end", text(&row.end.text)),
        ("exactEnd", text(&row.end.exact)),
    ])
}

/// One plan, as `PlannedPaymentsDialog.qml` lists it.
fn plan_object(plan: &PlanText) -> QVariant {
    map([
        ("id", text(&plan.id)),
        ("name", text(&plan.name)),
        ("account", text(&plan.account)),
        ("accountId", text(&plan.account_id)),
        ("amount", text(&plan.amount.text)),
        ("exactAmount", text(&plan.amount.exact)),
        ("outgoing", QVariant::from(&plan.outgoing)),
        ("unit", text(plan.unit)),
        ("every", QVariant::from(&plan.every)),
        ("firstDue", text(&plan.first_due)),
        ("ends", text(&plan.ends)),
        ("nextDue", text(&plan.next_due)),
        ("paused", QVariant::from(&plan.paused)),
        ("accountClosed", QVariant::from(&plan.account_closed)),
        ("category", text(&plan.category)),
        ("party", text(&plan.party)),
    ])
}

/// One payment of a plan, for "Did this happen?" and "Coming up".
fn payment_object(payment: &PaymentText) -> QVariant {
    map([
        ("planId", text(&payment.plan_id)),
        ("name", text(&payment.name)),
        ("account", text(&payment.account)),
        ("amount", text(&payment.amount.text)),
        ("exactAmount", text(&payment.amount.exact)),
        ("outgoing", QVariant::from(&payment.outgoing)),
        ("due", text(&payment.due)),
        ("dueOn", text(&payment.due_on)),
        ("state", text(payment.state)),
    ])
}

/// The plan form, as `PlanFormDialog.qml` fills its fields.
fn form_object(form: &PlanForm) -> QVariant {
    map([
        ("name", text(&form.name)),
        ("accountId", text(&form.account_id)),
        ("outgoing", QVariant::from(&form.outgoing)),
        ("amount", text(&form.amount)),
        ("categoryId", text(&form.category_id)),
        ("unit", text(&form.unit)),
        ("every", text(&form.every)),
        ("firstDue", text(&form.first_due)),
        ("ends", text(&form.ends)),
        ("tolerance", text(&form.tolerance)),
        ("party", text(&form.party)),
        ("fromEntry", text(&form.from_entry)),
    ])
}

/// An entry "Match an entry…" offers.
fn choice_object(choice: &EntryChoice) -> QVariant {
    map([
        ("id", text(&choice.id)),
        ("date", text(&choice.date)),
        ("payee", text(&choice.payee)),
        ("amount", text(&choice.amount.text)),
        ("exactAmount", text(&choice.amount.exact)),
    ])
}

#[cfg(test)]
mod tests {
    use super::{choice_object, form_object, payment_object, plan_object, read_form, view_object};
    use crate::planned_payments::{
        EntryChoice, MonthRow, MonthText, PaymentText, PlanForm, PlanText,
    };
    use crate::register::Shown;
    use crate::variant::read_back::{elements, flag_of, keys, list_of, number_of, text_of};
    use kassensturz_report::status::{self, ForecastFigures};

    fn exactly(text: &str) -> Shown {
        Shown {
            text: text.into(),
            exact: text.into(),
        }
    }

    fn payment() -> PaymentText {
        PaymentText {
            plan_id: "p1".into(),
            name: "Rent".into(),
            account: "Checking account".into(),
            amount: Shown {
                text: "750 €".into(),
                exact: "750,00 €".into(),
            },
            outgoing: true,
            due: "2026-11-02".into(),
            due_on: "2026-11-01".into(),
            state: "upcoming",
        }
    }

    /// `PlannedPaymentsDialog.qml` and the tiles read these by name
    /// (AGENTS.md §9.4).
    #[test]
    fn the_planned_payments_screens_read_back_every_field() {
        let plan = plan_object(&PlanText {
            id: "p1".into(),
            name: "Rent".into(),
            account: "Checking account".into(),
            account_id: "a1".into(),
            amount: exactly("750,00 €"),
            outgoing: true,
            unit: "month",
            every: 1,
            first_due: "2026-09-01".into(),
            ends: "2027-08-01".into(),
            next_due: "2026-11-02".into(),
            paused: false,
            account_closed: true,
            category: "Rent".into(),
            party: "DE89370400440532013000".into(),
        });
        assert_eq!(keys(&plan).len(), 16);
        assert_eq!(text_of(&plan, "id"), "p1");
        assert_eq!(text_of(&plan, "name"), "Rent");
        assert_eq!(text_of(&plan, "account"), "Checking account");
        assert_eq!(text_of(&plan, "accountId"), "a1");
        assert_eq!(text_of(&plan, "amount"), "750,00 €");
        assert_eq!(text_of(&plan, "exactAmount"), "750,00 €");
        assert!(flag_of(&plan, "outgoing"));
        assert_eq!(text_of(&plan, "unit"), "month");
        assert_eq!(number_of(&plan, "every"), 1);
        assert_eq!(text_of(&plan, "firstDue"), "2026-09-01");
        assert_eq!(text_of(&plan, "ends"), "2027-08-01");
        assert_eq!(text_of(&plan, "nextDue"), "2026-11-02");
        assert!(!flag_of(&plan, "paused"));
        assert!(flag_of(&plan, "accountClosed"));
        assert_eq!(text_of(&plan, "category"), "Rent");
        assert_eq!(text_of(&plan, "party"), "DE89370400440532013000");

        let paid = payment_object(&payment());
        assert_eq!(keys(&paid).len(), 9);
        assert_eq!(text_of(&paid, "planId"), "p1");
        assert_eq!(text_of(&paid, "name"), "Rent");
        assert_eq!(text_of(&paid, "account"), "Checking account");
        assert_eq!(text_of(&paid, "amount"), "750 €");
        assert_eq!(text_of(&paid, "exactAmount"), "750,00 €");
        assert!(flag_of(&paid, "outgoing"));
        assert_eq!(text_of(&paid, "due"), "2026-11-02");
        assert_eq!(text_of(&paid, "dueOn"), "2026-11-01");
        assert_eq!(text_of(&paid, "state"), "upcoming");

        let choice = choice_object(&EntryChoice {
            id: "t1".into(),
            date: "2026-11-03".into(),
            payee: "Landlord".into(),
            amount: exactly("-750,00 €"),
        });
        assert_eq!(keys(&choice).len(), 5);
        assert_eq!(text_of(&choice, "id"), "t1");
        assert_eq!(text_of(&choice, "date"), "2026-11-03");
        assert_eq!(text_of(&choice, "payee"), "Landlord");
        assert_eq!(text_of(&choice, "amount"), "-750,00 €");
        assert_eq!(text_of(&choice, "exactAmount"), "-750,00 €");

        let view = view_object(&[], &[payment()], &[payment(), payment()], &month());
        assert_eq!(keys(&view).len(), 6);
        assert_eq!(list_of(&view, "plans").len(), 0);
        assert_eq!(list_of(&view, "toConfirm").len(), 1);
        assert_eq!(list_of(&view, "comingUp").len(), 2);
        // No plan: the Coming up tile can't say anything yet (D-734).
        assert_eq!(text_of(&view, "comingUpStatus"), "unknown");
        assert_eq!(number_of(&view, "comingUpCount"), 0);
        assert!(elements(&view).is_none(), "an object, not a list");
    }

    fn month() -> MonthText {
        MonthText {
            accounts: vec!["Checking account".into(), "Cash".into()],
            left_out: 1,
            has_plans: true,
            balance_now: exactly("2340,00 €"),
            out_due: exactly("29,90 €"),
            in_due: Some(exactly("2200,00 €")),
            left: Shown {
                text: "2310 €".into(),
                exact: "2310,10 €".into(),
            },
            below_zero: false,
            expected_end: Some(exactly("4510,10 €")),
            overdue: 2,
            months: vec![MonthRow {
                month: "November 2026".into(),
                start: exactly("4510,10 €"),
                planned_in: exactly("2200,00 €"),
                planned_out: exactly("779,90 €"),
                end: exactly("5930,20 €"),
            }],
            status: status::forecast(Some(ForecastFigures {
                accounts: true,
                plans: true,
                overdue: 2,
            })),
        }
    }

    /// The *Left this month* tile and the month by month table read these
    /// by name (AGENTS.md §9.4).
    #[test]
    fn the_left_this_month_tile_reads_back_every_field() {
        let plan = PlanText {
            id: "p1".into(),
            name: "Gym".into(),
            account: "Checking account".into(),
            account_id: "a1".into(),
            amount: exactly("29,90 €"),
            outgoing: true,
            unit: "month",
            every: 1,
            first_due: "2026-10-25".into(),
            ends: String::new(),
            next_due: "2026-10-26".into(),
            paused: false,
            account_closed: false,
            category: String::new(),
            party: String::new(),
        };
        let view = view_object(&[plan], &[], &[payment(), payment(), payment()], &month());
        assert_eq!(text_of(&view, "comingUpStatus"), "fine");
        assert_eq!(number_of(&view, "comingUpCount"), 3);
        let forecast = super::month_object(&month());
        assert_eq!(keys(&forecast).len(), 18);
        let accounts: Vec<String> = list_of(&forecast, "accounts")
            .iter()
            .map(|name| {
                name.value::<cxx_qt_lib::QString>()
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(accounts, ["Checking account", "Cash"]);
        assert_eq!(number_of(&forecast, "leftOut"), 1);
        assert!(flag_of(&forecast, "hasPlans"));
        assert_eq!(text_of(&forecast, "balanceNow"), "2340,00 €");
        assert_eq!(text_of(&forecast, "exactBalanceNow"), "2340,00 €");
        assert_eq!(text_of(&forecast, "outDue"), "29,90 €");
        assert_eq!(text_of(&forecast, "exactOutDue"), "29,90 €");
        assert_eq!(text_of(&forecast, "inDue"), "2200,00 €");
        assert_eq!(text_of(&forecast, "exactInDue"), "2200,00 €");
        assert_eq!(text_of(&forecast, "left"), "2310 €");
        assert_eq!(text_of(&forecast, "exactLeft"), "2310,10 €");
        assert!(!flag_of(&forecast, "belowZero"));
        assert_eq!(text_of(&forecast, "expectedEnd"), "4510,10 €");
        assert_eq!(text_of(&forecast, "exactExpectedEnd"), "4510,10 €");
        assert_eq!(number_of(&forecast, "overdue"), 2);
        assert_eq!(text_of(&forecast, "status"), "to-do");
        assert_eq!(number_of(&forecast, "statusCount"), 2);
        let months = list_of(&forecast, "months");
        assert_eq!(months.len(), 1);
        let november = &months[0];
        assert_eq!(keys(november).len(), 9);
        assert_eq!(text_of(november, "month"), "November 2026");
        assert_eq!(text_of(november, "start"), "4510,10 €");
        assert_eq!(text_of(november, "exactStart"), "4510,10 €");
        assert_eq!(text_of(november, "plannedIn"), "2200,00 €");
        assert_eq!(text_of(november, "exactPlannedIn"), "2200,00 €");
        assert_eq!(text_of(november, "plannedOut"), "779,90 €");
        assert_eq!(text_of(november, "exactPlannedOut"), "779,90 €");
        assert_eq!(text_of(november, "end"), "5930,20 €");
        assert_eq!(text_of(november, "exactEnd"), "5930,20 €");

        // Without income still to come, both of its lines are empty.
        let no_income = super::month_object(&MonthText {
            in_due: None,
            expected_end: None,
            ..month()
        });
        assert_eq!(text_of(&no_income, "inDue"), "");
        assert_eq!(text_of(&no_income, "expectedEnd"), "");
    }

    /// The form goes to QML and comes back unchanged, so a field QML
    /// doesn't touch is never lost on the way.
    #[test]
    fn the_plan_form_comes_back_from_qml_as_it_went() {
        let form = PlanForm {
            name: "Rent".into(),
            account_id: "a1".into(),
            outgoing: true,
            amount: "750,00".into(),
            category_id: "c1".into(),
            unit: "month".into(),
            every: "1".into(),
            first_due: "2026-09-01".into(),
            ends: "2027-08-01".into(),
            tolerance: "5,00".into(),
            party: "Example Landlord".into(),
            from_entry: "t1".into(),
        };
        let object = form_object(&form);
        assert_eq!(keys(&object).len(), 12);
        assert_eq!(read_form(&object), form);
        assert_eq!(
            read_form(&cxx_qt_lib::QVariant::default()),
            PlanForm::default()
        );
    }
}
