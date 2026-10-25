// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KassenSturz's desktop app.
//!
//! It opens the book the app manages, or the one the user opened last
//! (D-033), and shows it through one QML page per screen.

mod accounts;
mod accounts_model;
mod ai_audit;
#[cfg(feature = "ai")]
mod ai_settings;
mod ai_settings_model;
mod ai_switch_model;
mod app_icon_audit;
mod bank_connection;
mod bank_connection_model;
mod bank_fetch;
mod book_from_file;
mod book_from_file_model;
mod book_session;
mod books;
mod categories;
mod categories_model;
mod errors;
mod export;
mod export_model;
mod fetch_gap;
mod fetch_gap_model;
mod first_start;
mod first_start_model;
mod format;
mod history;
mod history_model;
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
mod plain_text_audit;
mod platform;
mod register;
mod register_model;
mod rules;
mod rules_model;
mod secret_field_audit;
mod session;
mod settings;
mod statement_import;
mod statement_import_model;
#[cfg(test)]
mod test_folder;
mod transaction_entry;
mod transfer_prompt;
mod variant;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};
use cxx_qt_lib_extras::QApplication;

const MAIN_QML: &str = "qrc:/qt/qml/io/github/vondor/kassensturz/qml/Main.qml";

fn main() {
    // Before Qt starts: KI18n reads the language when it first translates.
    language::apply_at_start();
    // A QApplication rather than a QGuiApplication: Plasma's style and, from
    // M4, Gwenhywfar's bank dialogs need Qt Widgets (D-054).
    let mut app = QApplication::new();
    // Naming the desktop file after the app ID (D-049) gives the window the
    // right name and icon, also on Wayland.
    QGuiApplication::set_desktop_file_name(&QString::from(platform::APP_ID));
    platform::set_window_icon();
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
