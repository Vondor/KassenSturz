// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KassenSturz's desktop app.
//!
//! For now this is the first-run mock (M1.M, D-068): add an account, add a
//! transaction and see the overview, in a book in memory that is never saved.

mod first_run;
mod first_run_model;
mod format;
mod locale;
mod parse;
mod register;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};
use cxx_qt_lib_extras::QApplication;

/// The app ID (D-049). Naming the desktop file after it gives the window the
/// right name and icon, also on Wayland.
const APP_ID: &str = "io.github.vondor.KassenSturz";

const MAIN_QML: &str = "qrc:/qt/qml/io/github/vondor/kassensturz/qml/Main.qml";

fn main() {
    // A QApplication rather than a QGuiApplication: Plasma's style and, from
    // M4, Gwenhywfar's bank dialogs need Qt Widgets (D-054).
    let mut app = QApplication::new();
    QGuiApplication::set_desktop_file_name(&QString::from(APP_ID));
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
                std::process::exit(1);
            })
            .release();
        engine.load(&QUrl::from(MAIN_QML));
    }

    let exit_code = app.as_mut().map_or(1, |app| app.exec());
    // Qt objects go in reverse order of creation: the engine before the app.
    drop(engine);
    drop(app);
    std::process::exit(exit_code);
}
