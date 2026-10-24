// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Rules screens as a QML object (M3.11), on the book the app has open
//! (`session.rs`). The work is in `rules.rs`.
//!
//! QML gets everything as text, flags and lists of them (D-065); Rules are
//! named by their ID as text. A form goes to Rust as an object of text and
//! flags, with its conditions as a list of such objects. Every change asks
//! the other screens to read the book again through the `changed` signal.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};
use kassensturz_core::CurrencyCode;
use kassensturz_store::Book;

use crate::errors::{ErrorText, amount_example};
use crate::format::Locale;
use crate::locale::QtLocale;
use crate::number_format::{UserLocale, app_locale};
use crate::overview_model::latest_row_object;
use crate::register_model::{field, flag};
use crate::rules::{
    self, Choice, Choices, ConditionForm, Rebooked, RuleEditError, RuleForm, RuleRow, Suggestion,
};
use crate::session;
use crate::settings::Settings;
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        /// A value of any Qt type.
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The Rules of the book the app has open, and the Rules KassenSturz
        /// suggests.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type RuleBook = super::RuleBookRust;
    }

    extern "RustQt" {
        /// Reads the open book again, e.g. after another screen changed it.
        #[qinvokable]
        fn refresh(self: Pin<&mut RuleBook>);

        /// What the editor offers: `categories` (`id`, `name`, `depth`,
        /// `income`), `accounts` (`id`, `name`) and the `currency` amounts
        /// count in by default.
        #[qinvokable]
        fn choices(self: &RuleBook) -> QVariant;

        /// Today's date as the user types dates, as an example.
        #[qinvokable]
        #[cxx_name = "dateExample"]
        fn date_example(self: &RuleBook) -> QString;

        /// An amount in the currency (an ISO 4217 code) written the way the
        /// user types one, as an example.
        #[qinvokable]
        #[cxx_name = "amountExample"]
        fn amount_example(self: &RuleBook, currency: &QString) -> QString;

        /// A form for a new Rule inside the Rule with the ID `parent`
        /// (empty for the top level).
        #[qinvokable]
        #[cxx_name = "newForm"]
        fn new_form(self: &RuleBook, parent: &QString) -> QVariant;

        /// The form of the Rule with this ID, or an error's `code`.
        #[qinvokable]
        #[cxx_name = "formOf"]
        fn form_of(self: &RuleBook, id: &QString) -> QVariant;

        /// The form of a Rule to start from for a transaction in an
        /// account, or an error's `code`.
        #[qinvokable]
        #[cxx_name = "formFromTransaction"]
        fn form_from_transaction(
            self: &RuleBook,
            account: &QString,
            transaction: &QString,
        ) -> QVariant;

        /// The form of a Rule to start from for a suggestion, or an
        /// error's `code`.
        #[qinvokable]
        #[cxx_name = "formFromSuggestion"]
        fn form_from_suggestion(self: &RuleBook, payee: &QString, category: &QString) -> QVariant;

        /// Adds the Rule the form describes. Returns an empty map, or the
        /// error's `code` and its parameters.
        #[qinvokable]
        #[cxx_name = "addRule"]
        fn add_rule(self: Pin<&mut RuleBook>, form: &QVariant) -> QVariant;

        /// Changes a Rule to what the form describes, and with `rebook`
        /// books again what it booked before. Returns the number of
        /// transactions `changed` and `skipped`, or the error.
        #[qinvokable]
        #[cxx_name = "changeRule"]
        fn change_rule(self: Pin<&mut RuleBook>, form: &QVariant, rebook: bool) -> QVariant;

        /// How many transactions changing the Rule would book differently
        /// (`changed`) and not be able to (`skipped`); nothing is changed.
        #[qinvokable]
        #[cxx_name = "previewRebook"]
        fn preview_rebook(self: &RuleBook, form: &QVariant) -> QVariant;

        /// The transactions the Rule with this ID matches now, newest first
        /// (`accountId`, `account`, `id`, `date`, `otherParty`, `category`,
        /// `amount`, `exactAmount`, `incoming`, `kind`); nothing if the Rule
        /// is gone.
        #[qinvokable]
        fn matches(self: &RuleBook, id: &QString) -> QVariant;

        /// The transactions the Rule the form describes would fit on its own,
        /// newest first, in the shape of `matches`; nothing is saved. Empty
        /// while the form isn't a Rule that can be saved yet.
        #[qinvokable]
        #[cxx_name = "previewMatches"]
        fn preview_matches(self: &RuleBook, form: &QVariant) -> QVariant;

        /// Removes the Rule with this ID and its sub-Rules.
        #[qinvokable]
        #[cxx_name = "removeRule"]
        fn remove_rule(self: Pin<&mut RuleBook>, id: &QString) -> QVariant;

        /// Moves the Rule one place up or down.
        #[qinvokable]
        #[cxx_name = "moveRule"]
        fn move_rule(self: Pin<&mut RuleBook>, id: &QString, up: bool) -> QVariant;

        /// Remembers that the user doesn't want a Rule for this suggestion.
        #[qinvokable]
        #[cxx_name = "dismissSuggestion"]
        fn dismiss_suggestion(self: Pin<&mut RuleBook>, payee: &QString, category: &QString);
    }

    extern "RustQt" {
        /// The book changed through this object: the other screens read it
        /// again.
        #[qsignal]
        fn changed(self: Pin<&mut RuleBook>);
    }

    impl cxx_qt::Initialize for RuleBook {}
}

/// The Rust side of [`qobject::RuleBook`].
#[derive(Default)]
pub struct RuleBookRust {
    view: QVariant,
}

impl cxx_qt::Initialize for qobject::RuleBook {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh();
    }
}

impl qobject::RuleBook {
    fn refresh(mut self: Pin<&mut Self>) {
        let read = session::with_book(|book| {
            let dismissed = rules::dismissed(&Settings::user());
            Ok::<_, kassensturz_store::StoreError>((
                rules::view(book)?,
                rules::suggestions(book, &dismissed)?,
            ))
        });
        let (rows, suggestions) = match read {
            Some(Ok(read)) => read,
            Some(Err(error)) => {
                eprintln!("kassensturz: couldn't read the Rules: {error:?}");
                return;
            }
            None => return,
        };
        self.as_mut().rust_mut().view = view_object(&rows, &suggestions);
        self.as_mut().view_changed();
    }

    fn choices(&self) -> QVariant {
        let result = session::with_book(|book| rules::choices(book, fallback_currency()));
        match result {
            Some(Ok(choices)) => choices_object(&choices),
            Some(Err(error)) => {
                eprintln!("kassensturz: couldn't read the choices: {error:?}");
                error_object(&ErrorText::plain("input.no-such-entry"))
            }
            None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }

    fn date_example(&self) -> QString {
        let locale = app_locale();
        QString::from(locale.short_date(locale.today()))
    }

    fn amount_example(&self, currency: &QString) -> QString {
        let locale = app_locale();
        let example = CurrencyCode::new(&currency.to_string())
            .ok()
            .and_then(|currency| {
                session::with_book(|book| amount_example(book, currency, &locale).ok()).flatten()
            });
        QString::from(example.unwrap_or_default())
    }

    fn new_form(&self, parent: &QString) -> QVariant {
        form_object(&rules::new_form(&parent.to_string()))
    }

    fn form_of(&self, id: &QString) -> QVariant {
        let id = id.to_string();
        read_book(|book, locale| rules::form_of(book, &id, locale))
            .map_or_else(|error| error_object(&error), |form| form_object(&form))
    }

    fn form_from_transaction(&self, account: &QString, transaction: &QString) -> QVariant {
        let (account, transaction) = (account.to_string(), transaction.to_string());
        read_book(|book, _| rules::form_from_transaction(book, &account, &transaction))
            .map_or_else(|error| error_object(&error), |form| form_object(&form))
    }

    fn form_from_suggestion(&self, payee: &QString, category: &QString) -> QVariant {
        let (payee, category) = (payee.to_string(), category.to_string());
        read_book(|book, _| rules::form_from_suggestion(book, &payee, &category))
            .map_or_else(|error| error_object(&error), |form| form_object(&form))
    }

    fn add_rule(self: Pin<&mut Self>, form: &QVariant) -> QVariant {
        let form = read_form(form);
        let result = change(|book, locale| rules::add_rule(book, &form, locale).map(drop));
        self.finish(result.map(|()| map([])))
    }

    fn change_rule(self: Pin<&mut Self>, form: &QVariant, rebook: bool) -> QVariant {
        let form = read_form(form);
        let result = change(|book, locale| rules::change_rule(book, &form, rebook, locale));
        self.finish(result.map(rebooked_object))
    }

    fn preview_rebook(&self, form: &QVariant) -> QVariant {
        let form = read_form(form);
        read_book(|book, locale| rules::preview_rebook(book, &form, locale))
            .map_or_else(|error| error_object(&error), rebooked_object)
    }

    fn matches(&self, id: &QString) -> QVariant {
        let id = id.to_string();
        read_book(|book, locale| rules::matches(book, &id, locale)).map_or_else(
            |_| list([]),
            |rows| list(rows.iter().map(latest_row_object)),
        )
    }

    fn preview_matches(&self, form: &QVariant) -> QVariant {
        let form = read_form(form);
        read_book(|book, locale| rules::preview_matches(book, &form, locale)).map_or_else(
            |_| list([]),
            |rows| list(rows.iter().map(latest_row_object)),
        )
    }

    fn remove_rule(self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        let result = change(|book, _| rules::remove_rule(book, &id));
        self.finish(result.map(|()| map([])))
    }

    fn move_rule(self: Pin<&mut Self>, id: &QString, up: bool) -> QVariant {
        let id = id.to_string();
        let result = change(|book, _| rules::move_rule(book, &id, up));
        self.finish(result.map(|()| map([])))
    }

    fn dismiss_suggestion(mut self: Pin<&mut Self>, payee: &QString, category: &QString) {
        let (payee, category) = (payee.to_string(), category.to_string());
        if !rules::dismiss(&Settings::user(), &payee, &category) {
            eprintln!("kassensturz: couldn't save the turned-down suggestion");
        }
        self.as_mut().refresh();
    }

    /// Shows the new state after a change, or returns why it failed.
    fn finish(mut self: Pin<&mut Self>, result: Result<QVariant, ErrorText>) -> QVariant {
        match result {
            Ok(done) => {
                self.as_mut().refresh();
                self.as_mut().changed();
                done
            }
            Err(error) => error_object(&error),
        }
    }
}

/// Reads the open book, or says why not.
fn read_book<T>(
    f: impl FnOnce(&Book, &UserLocale<QtLocale>) -> Result<T, RuleEditError>,
) -> Result<T, ErrorText> {
    let locale = app_locale();
    session::with_book(|book| {
        f(book, &locale).map_err(|error| rules::error_text(book, &error, &locale))
    })
    .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")))
}

/// Changes the open book, or says why not.
fn change<T>(
    f: impl FnOnce(&mut Book, &UserLocale<QtLocale>) -> Result<T, RuleEditError>,
) -> Result<T, ErrorText> {
    let locale = app_locale();
    session::with_book_mut(|book| {
        f(book, &locale).map_err(|error| rules::error_text(book, &error, &locale))
    })
    .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")))
}

/// The currency of amounts in a book without accounts: the regional
/// settings' if KassenSturz knows it, otherwise the euro.
fn fallback_currency() -> CurrencyCode {
    QtLocale::currency()
        .or_else(|| CurrencyCode::new("EUR").ok())
        .expect("EUR is a valid code")
}

fn view_object(rows: &[RuleRow], suggestions: &[Suggestion]) -> QVariant {
    map([
        ("rows", list(rows.iter().map(row_object))),
        (
            "suggestions",
            list(suggestions.iter().map(suggestion_object)),
        ),
    ])
}

fn int(value: usize) -> QVariant {
    QVariant::from(&i32::try_from(value).unwrap_or(i32::MAX))
}

fn row_object(row: &RuleRow) -> QVariant {
    map([
        ("id", text(&row.id)),
        ("name", text(&row.name)),
        ("depth", int(row.depth)),
        ("position", int(row.position)),
        ("siblings", int(row.siblings)),
        ("bookToKind", text(row.book_to_kind)),
        ("bookTo", text(&row.book_to)),
        ("payee", text(&row.payee)),
        ("conditions", int(row.conditions)),
    ])
}

fn suggestion_object(suggestion: &Suggestion) -> QVariant {
    map([
        ("payee", text(&suggestion.payee)),
        ("category", text(&suggestion.category)),
        ("categoryPath", text(&suggestion.category_path)),
        ("count", int(suggestion.count)),
    ])
}

fn rebooked_object(rebooked: Rebooked) -> QVariant {
    map([
        ("changed", int(rebooked.changed)),
        ("skipped", int(rebooked.skipped)),
    ])
}

fn choices_object(choices: &Choices) -> QVariant {
    let choice = |choice: &Choice| {
        map([
            ("id", text(&choice.id)),
            ("name", text(&choice.name)),
            ("depth", int(choice.depth)),
            ("income", QVariant::from(&choice.income)),
        ])
    };
    map([
        ("categories", list(choices.categories.iter().map(choice))),
        ("accounts", list(choices.accounts.iter().map(choice))),
        ("currency", text(&choices.currency)),
    ])
}

fn form_object(form: &RuleForm) -> QVariant {
    map([
        ("id", text(&form.id)),
        ("parent", text(&form.parent)),
        ("name", text(&form.name)),
        ("matchAll", QVariant::from(&form.match_all)),
        (
            "conditions",
            list(form.conditions.iter().map(condition_object)),
        ),
        ("bookToKind", text(&form.book_to_kind)),
        ("bookTo", text(&form.book_to)),
        ("payee", text(&form.payee)),
    ])
}

fn condition_object(condition: &ConditionForm) -> QVariant {
    map([
        ("kind", text(&condition.kind)),
        ("field", text(&condition.field)),
        ("operator", text(&condition.operator)),
        ("value", text(&condition.value)),
        ("caseSensitive", QVariant::from(&condition.case_sensitive)),
        ("direction", text(&condition.direction)),
        ("amountTest", text(&condition.amount_test)),
        ("amount", text(&condition.amount)),
        ("amountTo", text(&condition.amount_to)),
        ("currency", text(&condition.currency)),
        ("firstDate", text(&condition.first_date)),
        ("lastDate", text(&condition.last_date)),
        ("account", text(&condition.account)),
    ])
}

/// The editor's form as QML sends it.
fn read_form(form: &QVariant) -> RuleForm {
    let map = form
        .value::<QMap<QMapPair_QString_QVariant>>()
        .unwrap_or_default();
    let conditions = map
        .get(&QString::from("conditions"))
        .and_then(|value| value.value::<QList<QVariant>>())
        .unwrap_or_default();
    RuleForm {
        id: field(&map, "id"),
        parent: field(&map, "parent"),
        name: field(&map, "name"),
        match_all: flag(&map, "matchAll"),
        conditions: conditions
            .iter()
            .map(|condition| {
                let condition = condition
                    .value::<QMap<QMapPair_QString_QVariant>>()
                    .unwrap_or_default();
                ConditionForm {
                    kind: field(&condition, "kind"),
                    field: field(&condition, "field"),
                    operator: field(&condition, "operator"),
                    value: field(&condition, "value"),
                    case_sensitive: flag(&condition, "caseSensitive"),
                    direction: field(&condition, "direction"),
                    amount_test: field(&condition, "amountTest"),
                    amount: field(&condition, "amount"),
                    amount_to: field(&condition, "amountTo"),
                    currency: field(&condition, "currency"),
                    first_date: field(&condition, "firstDate"),
                    last_date: field(&condition, "lastDate"),
                    account: field(&condition, "account"),
                }
            })
            .collect(),
        book_to_kind: field(&map, "bookToKind"),
        book_to: field(&map, "bookTo"),
        payee: field(&map, "payee"),
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_core::AccountType;

    use super::*;
    use crate::accounts::{NewAccount, add_account};
    use crate::first_start::tests::started;
    use crate::register::tests::TestLocale;
    use crate::transaction_entry::{self, NewEntry};
    use crate::variant::read_back::{flag_of, keys, list_of, number_of, text_of};

    fn eur() -> CurrencyCode {
        CurrencyCode::new("EUR").expect("EUR is a valid code")
    }

    fn checking(book: &mut Book) -> String {
        add_account(
            book,
            &NewAccount {
                name: "Giro",
                account_type: AccountType::Checking,
                currency: "EUR",
                balance: "1000",
                date: "2026-09-01",
            },
            &TestLocale,
        )
        .expect("account added")
        .to_string()
    }

    fn category(book: &Book, path: &str) -> String {
        book.categories()
            .expect("categories")
            .into_iter()
            .find(|entry| entry.path.join(" › ") == path)
            .expect("category")
            .category
            .id
            .to_string()
    }

    fn pay(book: &mut Book, account: &str, other_party: &str, category: &str, date: &str) {
        transaction_entry::add(
            book,
            &NewEntry {
                account,
                money_in: false,
                amount: "5",
                other_party,
                category,
                date,
            },
            &TestLocale,
        )
        .expect("transaction added");
    }

    fn payee_is(name: &str, payee: &str, to: &str) -> RuleForm {
        RuleForm {
            name: name.into(),
            match_all: true,
            conditions: vec![ConditionForm {
                kind: "text".into(),
                field: "payee".into(),
                operator: "is".into(),
                value: payee.into(),
                case_sensitive: true,
                ..ConditionForm::default()
            }],
            book_to_kind: "category".into(),
            book_to: to.into(),
            ..RuleForm::default()
        }
    }

    /// A book with a Rule that has a Rule inside it, and a payee sorted into
    /// a category by hand often enough to be suggested.
    fn book_with_rules() -> (Book, String) {
        let mut book = started();
        let groceries = category(&book, "Food › Groceries");
        let eating_out = category(&book, "Food › Eating out");
        let account = checking(&mut book);
        let shop = rules::add_rule(
            &mut book,
            &payee_is("Shop", "Corner Shop", &groceries),
            &TestLocale,
        )
        .expect("Rule added")
        .to_string();
        let mut inside = payee_is("Bakery", "Bakery", "");
        inside.parent = shop;
        inside.book_to_kind = String::new();
        inside.payee = "Fresh bread".into();
        rules::add_rule(&mut book, &inside, &TestLocale).expect("Rule added");
        for day in 1..=rules::SUGGEST_AFTER {
            pay(
                &mut book,
                &account,
                "Canteen",
                &eating_out,
                &format!("2026-10-0{day}"),
            );
        }
        (book, account)
    }

    #[test]
    fn the_rules_and_the_suggestions_read_back_as_qml_reads_them() {
        let (book, _) = book_with_rules();
        let rows = rules::view(&book).expect("view");
        let suggestions = rules::suggestions(&book, &[]).expect("suggestions");
        let view = view_object(&rows, &suggestions);
        assert_eq!(keys(&view), ["rows", "suggestions"]);

        let listed = list_of(&view, "rows");
        assert_eq!(listed.len(), 2);
        assert_eq!(
            keys(&listed[0]),
            [
                "bookTo",
                "bookToKind",
                "conditions",
                "depth",
                "id",
                "name",
                "payee",
                "position",
                "siblings"
            ]
        );
        assert_eq!(text_of(&listed[0], "id"), rows[0].id);
        assert_eq!(text_of(&listed[0], "name"), "Shop");
        assert_eq!(text_of(&listed[0], "bookToKind"), "category");
        assert_eq!(text_of(&listed[0], "bookTo"), "Food › Groceries");
        assert_eq!(text_of(&listed[0], "payee"), "");
        for (key, expected) in [
            ("depth", 0),
            ("position", 0),
            ("siblings", 1),
            ("conditions", 1),
        ] {
            assert_eq!(number_of(&listed[0], key), expected, "{key}");
        }
        // The Rule inside the first one.
        assert_eq!(text_of(&listed[1], "name"), "Bakery");
        assert_eq!(text_of(&listed[1], "bookToKind"), "");
        assert_eq!(text_of(&listed[1], "payee"), "Fresh bread");
        assert_eq!(number_of(&listed[1], "depth"), 1);

        let offered = list_of(&view, "suggestions");
        assert_eq!(offered.len(), 1);
        assert_eq!(
            keys(&offered[0]),
            ["category", "categoryPath", "count", "payee"]
        );
        assert_eq!(text_of(&offered[0], "payee"), "Canteen");
        assert_eq!(text_of(&offered[0], "category"), suggestions[0].category);
        assert_eq!(text_of(&offered[0], "categoryPath"), "Food › Eating out");
        assert_eq!(
            number_of(&offered[0], "count"),
            i32::try_from(rules::SUGGEST_AFTER).expect("a small number")
        );
    }

    #[test]
    fn the_choices_read_back_as_qml_reads_them() {
        let (book, account) = book_with_rules();
        let choices = choices_object(&rules::choices(&book, eur()).expect("choices"));
        assert_eq!(keys(&choices), ["accounts", "categories", "currency"]);
        assert_eq!(text_of(&choices, "currency"), "EUR");

        let accounts = list_of(&choices, "accounts");
        assert_eq!(accounts.len(), 1);
        assert_eq!(keys(&accounts[0]), ["depth", "id", "income", "name"]);
        assert_eq!(text_of(&accounts[0], "id"), account);
        assert_eq!(text_of(&accounts[0], "name"), "Giro");
        assert!(!flag_of(&accounts[0], "income"));

        let categories = list_of(&choices, "categories");
        let find = |name: &str| {
            categories
                .iter()
                .find(|choice| text_of(choice, "name") == name)
                .unwrap_or_else(|| panic!("no category `{name}`"))
        };
        let groceries = find("Food › Groceries");
        assert_eq!(number_of(groceries, "depth"), 1);
        assert!(!flag_of(groceries, "income"));
        assert!(
            categories.iter().any(|choice| flag_of(choice, "income")),
            "the income categories are marked"
        );
    }

    #[test]
    fn the_editor_form_reads_back_and_is_what_the_screen_sends_back() {
        let (book, _) = book_with_rules();
        let id = rules::view(&book).expect("view")[0].id.clone();
        let form = rules::form_of(&book, &id, &TestLocale).expect("form");
        let shown = form_object(&form);
        assert_eq!(
            keys(&shown),
            [
                "bookTo",
                "bookToKind",
                "conditions",
                "id",
                "matchAll",
                "name",
                "parent",
                "payee"
            ]
        );
        assert_eq!(text_of(&shown, "id"), id);
        assert_eq!(text_of(&shown, "name"), "Shop");
        assert_eq!(text_of(&shown, "bookToKind"), "category");
        assert!(flag_of(&shown, "matchAll"));

        let conditions = list_of(&shown, "conditions");
        assert_eq!(conditions.len(), 1);
        assert_eq!(
            keys(&conditions[0]),
            [
                "account",
                "amount",
                "amountTest",
                "amountTo",
                "caseSensitive",
                "currency",
                "direction",
                "field",
                "firstDate",
                "kind",
                "lastDate",
                "operator",
                "value"
            ]
        );
        for (key, expected) in [
            ("kind", "text"),
            ("field", "payee"),
            ("operator", "is"),
            ("value", "Corner Shop"),
        ] {
            assert_eq!(text_of(&conditions[0], key), expected, "{key}");
        }
        assert!(flag_of(&conditions[0], "caseSensitive"));

        // The editor sends the object back as it got it: nothing is lost on
        // the way out and in again.
        assert_eq!(read_form(&shown), form);
        let new = rules::new_form("");
        assert_eq!(read_form(&form_object(&new)), new);
    }

    #[test]
    fn what_changing_a_rule_did_reads_back_as_qml_reads_it() {
        let done = rebooked_object(Rebooked {
            changed: 3,
            skipped: 1,
        });
        assert_eq!(keys(&done), ["changed", "skipped"]);
        assert_eq!(number_of(&done, "changed"), 3);
        assert_eq!(number_of(&done, "skipped"), 1);
    }
}
