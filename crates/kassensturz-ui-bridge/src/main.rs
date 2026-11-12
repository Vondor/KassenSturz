// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KassenSturz's desktop app.
//!
//! It opens the book the app manages, or the one the user opened last
//! (D-033), and shows it through one QML page per screen.

// On Windows the app is a window program, not a console one: without this,
// Windows opens an empty terminal window next to it (M6-B175). Started from
// PowerShell, as the install sheet's developer mode does, it still runs; its
// messages go to the debug log, not to the terminal.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod accounts;
mod accounts_model;
mod action_log_audit;
mod ai_audit;
#[cfg(feature = "ai")]
mod ai_chat;
mod ai_chat_model;
#[cfg(feature = "ai")]
mod ai_requests;
#[cfg(feature = "ai")]
mod ai_settings;
mod ai_settings_model;
mod ai_switch_model;
mod alarms;
mod alarms_model;
mod app_icon_audit;
mod bank_connection;
mod bank_connection_model;
mod bank_fetch;
mod book_from_file;
mod book_from_file_model;
mod book_session;
mod books;
mod budget_bars_model;
mod budgets;
mod budgets_model;
mod cards;
mod cards_model;
mod categories;
mod categories_model;
mod category_groups;
mod category_groups_model;
mod chart_palette;
mod chart_palette_model;
mod chart_spec;
#[cfg(test)]
mod chart_spec_hostile_tests;
mod chart_spec_model;
mod charts;
mod charts_model;
mod custom_charts;
mod dashboard_file;
mod dashboard_file_model;
mod dashboards;
mod developer_mode;
mod errors;
mod example_books;
mod example_books_model;
mod export;
mod export_model;
mod fetch_gap;
mod fetch_gap_model;
mod first_start;
mod first_start_model;
mod format;
mod formulas;
mod formulas_model;
mod grid_editor;
mod grid_editor_model;
mod history;
mod history_model;
mod kmy_reports_import;
mod kmy_reports_import_model;
mod language;
mod language_model;
mod locale;
mod network_guard;
mod number_format;
mod number_format_model;
mod overview;
mod overview_model;
mod page_stack_audit;
mod parse;
mod parts_entry;
mod period_range;
mod period_range_model;
mod plain_text_audit;
mod platform;
mod properties;
mod properties_model;
mod register;
mod register_columns;
mod register_model;
mod restart;
mod rules;
mod rules_model;
mod saved_views;
mod saved_views_model;
mod scope;
mod secret_field_audit;
mod session;
mod settings;
mod statement_import;
mod statement_import_model;
mod tags;
mod tags_model;
#[cfg(test)]
mod test_folder;
mod test_profile;
mod test_profile_model;
mod transaction_entry;
mod transactions_tile;
mod transactions_tile_model;
mod transfer_prompt;
mod update_all;
mod variant;
mod window_state_model;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};
use cxx_qt_lib_extras::QApplication;

const MAIN_QML: &str = "qrc:/qt/qml/io/github/vondor/kassensturz/qml/Main.qml";

fn main() {
    // Before anything changes the environment: a restart in another language
    // starts the app the way it was started (Q-395).
    restart::remember_at_start();
    // Then: in a test, every folder the app uses moves to the test
    // folder before anything reads one (Q-399).
    if test_profile::apply_at_start() && cfg!(windows) {
        platform::use_test_locations();
    }
    // Before Qt starts: KI18n reads the language when it first translates.
    language::apply_at_start();
    // A QApplication rather than a QGuiApplication: Plasma's style and, from
    // M4, Gwenhywfar's bank dialogs need Qt Widgets (D-054).
    let mut app = QApplication::new();
    // Naming the desktop file after the app ID (D-049) gives the window the
    // right name and icon, also on Wayland.
    QGuiApplication::set_desktop_file_name(&QString::from(platform::APP_ID));
    platform::use_bundled_icons();
    platform::use_readable_widget_style();
    platform::follow_system_colours();
    platform::start_in_user_folder();
    platform::set_window_icon();
    platform::load_ki18n_qml();
    crate::note!(AppStarted);
    // Plasma's style, unless the user chose another one.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        QQuickStyle::set_style(&QString::from("org.kde.desktop"));
    }

    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        engine
            .as_mut()
            .on_object_creation_failed(|_, url| {
                // Qt has printed why. Without a window there's nothing more to
                // do, and a process without a window would only hang around.
                eprintln!("kassensturz: couldn't load the user interface from {url}");
                session::close();
                std::process::exit(1);
            })
            .release();
        // Before the first file loads: no screen reaches the network (D-143).
        network_guard::install(engine.as_mut());
        engine.load(&QUrl::from(MAIN_QML));
    }

    let exit_code = app.as_mut().map_or(1, |app| app.exec());
    // Qt objects go in reverse order of creation: the engine before the app.
    drop(engine);
    drop(app);
    // `exit` doesn't drop what the thread keeps, so the book is closed here,
    // which removes its lock file (D-038).
    session::close();
    std::process::exit(exit_code);
}
