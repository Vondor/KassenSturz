// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Generates the C++ side of the cxx-qt bridges and compiles it, together
//! with the QML module, into the app.

use std::path::PathBuf;
use std::process::Command;

use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    println!(
        "cargo:rustc-env=KASSENSTURZ_BUILD_COMMIT={}",
        build_commit()
    );
    let kconfig = KConfigCore::find();
    let mut builder = CxxQtBuilder::new_qml_module(
        QmlModule::new("io.github.vondor.kassensturz")
            .qml_files([
                "qml/Main.qml",
                "qml/AccountCard.qml",
                "qml/AccountDetailsPage.qml",
                "qml/AccountTypes.qml",
                "qml/AccountsPage.qml",
                "qml/AiErrorMessage.qml",
                "qml/AiSettingsEntry.qml",
                "qml/AiSettingsPage.qml",
                "qml/AmountLabel.qml",
                "qml/AmountLine.qml",
                "qml/Balance.qml",
                "qml/BankAccountNames.qml",
                "qml/BankCheckPage.qml",
                "qml/BankDialogs.qml",
                "qml/BankFetchPage.qml",
                "qml/BankSetupPage.qml",
                "qml/BookFromFileDonePage.qml",
                "qml/BookFromFilePage.qml",
                "qml/BookProblemPage.qml",
                "qml/CategoriesPage.qml",
                "qml/CategoryPage.qml",
                "qml/ChangeNames.qml",
                "qml/CsvColumnBox.qml",
                "qml/CsvSettingsPage.qml",
                "qml/DefaultCategories.qml",
                "qml/ExportPage.qml",
                "qml/FormError.qml",
                "qml/HistoryPage.qml",
                "qml/ImportDonePage.qml",
                "qml/ImportPreviewPage.qml",
                "qml/ImportStatementsPage.qml",
                "qml/MockNotice.qml",
                "qml/NewAccountPage.qml",
                "qml/OverviewPage.qml",
                "qml/PartRow.qml",
                "qml/PartsPage.qml",
                "qml/RegisterPage.qml",
                "qml/ReplacedCurrency.qml",
                "qml/RuleCondition.qml",
                "qml/RulePage.qml",
                "qml/RulesPage.qml",
                "qml/SecretField.qml",
                "qml/SettingsPage.qml",
                "qml/TransactionPage.qml",
                "qml/TransactionRow.qml",
                "qml/TransferPrompt.qml",
                "qml/WelcomePage.qml",
            ])
            // A helper, not a screen: `Plain.plain()` (D-143).
            .qml_files([QmlFile::from("qml/Plain.qml").singleton(true)]),
    )
    .qt_module("Network")
    .files([
        "src/accounts_model.rs",
        "src/ai_settings_model.rs",
        "src/ai_switch_model.rs",
        "src/bank_connection_model.rs",
        "src/book_from_file_model.rs",
        "src/book_session.rs",
        "src/categories_model.rs",
        "src/export_model.rs",
        "src/fetch_gap_model.rs",
        "src/first_start_model.rs",
        "src/history_model.rs",
        "src/language_model.rs",
        "src/locale.rs",
        "src/network_guard.rs",
        "src/number_format_model.rs",
        "src/overview_model.rs",
        "src/platform.rs",
        "src/register_model.rs",
        "src/rules_model.rs",
        "src/settings.rs",
        "src/statement_import_model.rs",
    ])
    .cpp_files([
        "cpp/local_only.cpp",
        "cpp/locale.cpp",
        "cpp/platform.cpp",
        "cpp/settings.cpp",
    ]);
    // SAFETY: only adds include folders for KConfig's headers, which
    // cxx-qt-build's own settings for the compiler don't depend on.
    builder = unsafe {
        builder.cc_builder(|cc| {
            for dir in &kconfig.include_dirs {
                cc.include(dir);
            }
        })
    };
    builder.build();

    println!(
        "cargo:rustc-link-search=native={}",
        kconfig.library_dir.display()
    );
    println!("cargo:rustc-link-lib=KF6ConfigCore");
}

/// The short id of the commit this build is made from, so a tester can tell
/// builds apart that have the same version number (D-152). In order: the
/// `KASSENSTURZ_COMMIT` variable, the file `build-commit` in the repository's
/// top folder (CI writes it before the Flatpak build, whose copy of the sources
/// has no `.git`), and git itself. Empty when none of them knows.
fn build_commit() -> String {
    println!("cargo:rerun-if-env-changed=KASSENSTURZ_COMMIT");
    println!("cargo:rerun-if-changed=../../build-commit");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    let from_git = || {
        let output = Command::new("git")
            .args(["rev-parse", "--short=7", "HEAD"])
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    };
    let commit = std::env::var("KASSENSTURZ_COMMIT")
        .ok()
        .or_else(|| std::fs::read_to_string("../../build-commit").ok())
        .or_else(from_git)
        .unwrap_or_default();
    commit
        .trim()
        .chars()
        .take(7)
        .filter(char::is_ascii_hexdigit)
        .collect()
}

/// Where KConfig's core library is (D-112). The app's settings are kept
/// with it, the only KDE Frameworks library the Rust program links.
struct KConfigCore {
    include_dirs: Vec<PathBuf>,
    library_dir: PathBuf,
}

impl KConfigCore {
    /// CMake passes what its `KF6::ConfigCore` target says (D-063). A plain
    /// `cargo build` or `cargo test` looks next to Qt, where the KDE
    /// Flatpak SDK and the distributions put KDE Frameworks.
    fn find() -> Self {
        println!("cargo:rerun-if-env-changed=KASSENSTURZ_KCONFIG_INCLUDE_DIRS");
        println!("cargo:rerun-if-env-changed=KASSENSTURZ_KCONFIG_LIBRARY_DIR");
        println!("cargo:rerun-if-env-changed=QMAKE");
        if let (Some(includes), Some(library_dir)) = (
            std::env::var_os("KASSENSTURZ_KCONFIG_INCLUDE_DIRS"),
            std::env::var_os("KASSENSTURZ_KCONFIG_LIBRARY_DIR"),
        ) {
            // A CMake list: separated by semicolons.
            let include_dirs = includes
                .to_string_lossy()
                .split(';')
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from)
                .collect();
            return Self {
                include_dirs,
                library_dir: library_dir.into(),
            };
        }
        let prefix = qmake_query("QT_INSTALL_PREFIX");
        Self {
            // KConfigCore's headers include the version header in KConfig.
            include_dirs: vec![
                prefix.join("include/KF6/KConfigCore"),
                prefix.join("include/KF6/KConfig"),
            ],
            library_dir: qmake_query("QT_INSTALL_LIBS"),
        }
    }
}

/// What Qt's `qmake` says about Qt, from the `qmake` cxx-qt-build uses too:
/// the one `QMAKE` names, otherwise `qmake6` or `qmake` on the `PATH`.
fn qmake_query(variable: &str) -> PathBuf {
    let candidates = match std::env::var_os("QMAKE") {
        Some(qmake) => vec![qmake],
        None => vec!["qmake6".into(), "qmake".into()],
    };
    for qmake in candidates {
        let Ok(output) = Command::new(&qmake).args(["-query", variable]).output() else {
            continue;
        };
        if output.status.success() {
            return PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        }
    }
    panic!("couldn't run qmake to find {variable}: set QMAKE to Qt 6's qmake");
}
