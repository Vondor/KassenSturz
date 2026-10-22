// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KassenSturz's desktop app.
//!
//! It opens the book the app manages, or the one the user opened last
//! (D-033), and shows it through one QML page per screen. The screens are
//! still the first-run mock (M1.M, D-068) until the M3 steps replace them.

mod book_session;
mod books;
mod first_run;
mod first_run_model;
mod format;
mod locale;
mod parse;
mod platform;
mod register;
mod session;
mod settings;
#[cfg(test)]
mod test_folder;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};
use cxx_qt_lib_extras::QApplication;

const MAIN_QML: &str = "qrc:/qt/qml/io/github/vondor/kassensturz/qml/Main.qml";

fn main() {
    // A QApplication rather than a QGuiApplication: Plasma's style and, from
    // M4, Gwenhywfar's bank dialogs need Qt Widgets (D-054).
    let mut app = QApplication::new();
    // Naming the desktop file after the app ID (D-049) gives the window the
    // right name and icon, also on Wayland.
    QGuiApplication::set_desktop_file_name(&QString::from(platform::APP_ID));
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
