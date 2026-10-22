// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The number format settings (D-002, D-114) as a QML object, for the
//! Settings page.
//!
//! QML gets each setting's current choice as the word the settings file
//! keeps for it, e.g. "comma", and an example amount written the chosen way;
//! the page owns every label (D-034).

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QMap, QMapPair_QString_QVariant, QString, QVariant};
use kassensturz_core::{CurrencyCode, Money};

use crate::format::Locale;
use crate::locale::QtLocale;
use crate::number_format::{self, NumberFormat, UserLocale};

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
        /// The user's number format.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, choices, READ, NOTIFY)]
        #[qproperty(QString, example, READ, NOTIFY)]
        #[qproperty(QString, negative_example, cxx_name = "negativeExample", READ, NOTIFY)]
        type NumberFormatSettings = super::NumberFormatSettingsRust;
    }

    extern "RustQt" {
        /// Chooses `choice` for `setting`, e.g. "comma" for
        /// "DecimalSeparator", for the app from now on and at the next
        /// start. Choosing a separator the other one uses swaps them
        /// (D-114). Returns whether the choice was saved.
        #[qinvokable]
        fn choose(
            self: Pin<&mut NumberFormatSettings>,
            setting: &QString,
            choice: &QString,
        ) -> bool;
    }

    impl cxx_qt::Initialize for NumberFormatSettings {}
}

/// The Rust side of [`qobject::NumberFormatSettings`].
#[derive(Default)]
pub struct NumberFormatSettingsRust {
    choices: QVariant,
    example: QString,
    negative_example: QString,
}

impl cxx_qt::Initialize for qobject::NumberFormatSettings {
    fn initialize(self: Pin<&mut Self>) {
        self.show(number_format::current());
    }
}

impl qobject::NumberFormatSettings {
    fn choose(self: Pin<&mut Self>, setting: &QString, choice: &QString) -> bool {
        let Some(format) =
            number_format::current().with(&setting.to_string(), &choice.to_string(), &QtLocale)
        else {
            return false;
        };
        let saved = number_format::set_current(format);
        self.show(format);
        saved
    }

    /// Shows `format`'s choices and examples.
    fn show(mut self: Pin<&mut Self>, format: NumberFormat) {
        let mut choices = QMap::<QMapPair_QString_QVariant>::default();
        for (setting, choice) in format.keys() {
            choices.insert(
                QString::from(setting),
                QVariant::from(&QString::from(choice)),
            );
        }
        let (example, negative_example) = examples(&UserLocale::new(QtLocale, format));

        let mut model = self.as_mut().rust_mut();
        model.choices = QVariant::from(&choices);
        model.example = QString::from(example.as_str());
        model.negative_example = QString::from(negative_example.as_str());
        self.as_mut().choices_changed();
        self.as_mut().example_changed();
        self.as_mut().negative_example_changed();
    }
}

/// A large amount and a negative one in the locale's own currency, or the
/// euro, written as `locale` writes them, e.g. "1.234.567,89 €" and
/// "-53,20 €": large enough to show the grouping, with decimal places to
/// show what is rounded.
fn examples(locale: &impl Locale) -> (String, String) {
    let currency = QtLocale::currency().unwrap_or(EUR);
    let minor_digits = currency.iso_minor_digits().unwrap_or(2);
    let scale = 10_i64.pow(u32::from(minor_digits));
    let format = locale.amount_format(currency);
    // `cents` hundredths of the currency, cut to the decimal places it has:
    // .89 of a euro, .890 of a Bahraini dinar, none of a yen.
    let write = |whole: i64, cents: i64| {
        let amount = Money::from_minor_units(whole * scale + cents * scale / 100, currency);
        format.format_rounded(
            amount,
            minor_digits,
            locale.shown_decimal_places(minor_digits),
            |value| locale.group_digits(value),
        )
    };
    (write(1_234_567, 89), write(-53, -20))
}

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
