// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Generates the C++ side of the cxx-qt bridges and compiles it, together
//! with the QML module, into the app.

use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("io.github.vondor.kassensturz").qml_files([
        "qml/Main.qml",
        "qml/AccountPage.qml",
        "qml/AmountLine.qml",
        "qml/FormError.qml",
        "qml/MockNotice.qml",
        "qml/OverviewPage.qml",
        "qml/RegisterPage.qml",
        "qml/TransactionPage.qml",
        "qml/TransactionRow.qml",
        "qml/WelcomePage.qml",
    ]))
    .files(["src/first_run_model.rs", "src/locale.rs"])
    .cpp_file("cpp/locale.cpp")
    .build();
}
