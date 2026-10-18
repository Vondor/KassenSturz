// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Generates the C++ side of the cxx-qt bridges and compiles it, together
//! with the QML module, into the app.

use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("io.github.vondor.kassensturz").qml_file("qml/Main.qml"),
    )
    .files(["src/locale.rs", "src/transaction_model.rs"])
    .cpp_file("cpp/locale.cpp")
    .build();
}
