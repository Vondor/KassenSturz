// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The QML object behind reports from plugins (#791 step 5, D-926): the
//! "From plugins" list on the Statistics page and the report page. The
//! work is in `plugin_reports.rs`; this file only moves text between it
//! and QML (D-065).

use cxx_qt_lib::{QString, QVariant};

use crate::errors::ErrorText;
use crate::format::Locale;
use crate::number_format::app_locale;
use crate::plugin_reports::{self, list_object, period_range, table_object};
use crate::variant::{error_object, map, text};
use crate::{ai_switch_model, session};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        /// Qt's variant type.
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// Reports from plugins.
        #[qobject]
        #[qml_element]
        type PluginReports = super::PluginReportsRust;
    }

    extern "RustQt" {
        /// Every report the added plugins offer: each `{ file, plugin, id,
        /// name }`, by plugin name.
        #[qinvokable]
        fn reports(self: &PluginReports) -> QVariant;

        /// Works out the report `id` of the plugin file `file` for the
        /// account with `account` as its ID (empty: all accounts) and the
        /// period word `period` (`month`, `3-months`, `12-months`, `year`,
        /// `all`). The table `{ title, columns, rows }`, or an error
        /// object.
        #[qinvokable]
        fn run(
            self: &PluginReports,
            file: &QString,
            id: &QString,
            account: &QString,
            period: &QString,
        ) -> QVariant;
    }
}

/// The Rust side of [`qobject::PluginReports`].
#[derive(Default)]
pub struct PluginReportsRust;

impl qobject::PluginReports {
    fn reports(&self) -> QVariant {
        list_object(&plugin_reports::report_plugins(ai_switch_model::ai_on()))
    }

    fn run(&self, file: &QString, id: &QString, account: &QString, period: &QString) -> QVariant {
        let file = file.to_string();
        let Some(plugin) = plugin_reports::report_plugins(ai_switch_model::ai_on())
            .into_iter()
            .find(|plugin| plugin.file == file)
        else {
            return error_object(&ErrorText::plain("input.plugin-gone"));
        };
        let locale = app_locale();
        let Some(range) = period_range(&period.to_string(), locale.today()) else {
            return error_object(&ErrorText::plain("input.no-such-entry"));
        };
        let account = account.to_string();
        let ids: Vec<String> = if account.is_empty() {
            Vec::new()
        } else {
            vec![account]
        };
        let done = session::with_book(|book| {
            plugin_reports::run(book, &plugin, &id.to_string(), &ids, range, &locale)
        })
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")));
        match done {
            Ok(table) => map([
                ("table", table_object(&table, &locale)),
                ("plugin", text(&plugin.name)),
            ]),
            Err(error) => error_object(&error),
        }
    }
}
