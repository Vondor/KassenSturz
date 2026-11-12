// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Generates the C++ side of the cxx-qt bridges and compiles it, together
//! with the QML module, into the app.

use std::path::PathBuf;
use std::process::Command;

use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    println!(
        "cargo:rustc-env=KASSENSTURZ_BUILD_VERSION={}",
        build_version()
    );
    println!(
        "cargo:rustc-env=KASSENSTURZ_BUILD_COMMIT={}",
        build_commit()
    );
    let kconfig = KConfigCore::find();
    // The Windows app brings its own icons (D-555); Cargo builds this script
    // for the computer it runs on, so the target comes from Cargo's variable.
    let windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    let breeze_icons = windows.then(|| WindowsLibrary::find("BREEZEICONS", "BreezeIcons"));
    // And follows Windows' light or dark mode with KDE's colour schemes (D-581).
    let color_scheme = windows.then(|| WindowsLibrary::find("COLORSCHEME", "KColorScheme"));
    let mut builder = CxxQtBuilder::new_qml_module(
        QmlModule::new("io.github.vondor.kassensturz")
            .qml_files([
                "qml/Main.qml",
                "qml/AccountCard.qml",
                "qml/KpiCard.qml",
                "qml/KpiGrid.qml",
                "qml/KpiNames.qml",
                "qml/PeriodChoice.qml",
                "qml/StatisticWizard.qml",
                "qml/ColumnBoard.qml",
                "qml/ProgressMeter.qml",
                "qml/AccountGrid.qml",
                "qml/AccountDetailsPage.qml",
                "qml/AccountTypes.qml",
                "qml/AccountsPage.qml",
                "qml/AiChangeCorrection.qml",
                "qml/AiBudgetChange.qml",
                "qml/AiJoinTransferChange.qml",
                "qml/AiRulesSortChange.qml",
                "qml/AiChartCard.qml",
                "qml/AiChangeReviewPage.qml",
                "qml/AiChatLogLine.qml",
                "qml/AiChatMessage.qml",
                "qml/AiChatPanel.qml",
                "qml/AiErrorMessage.qml",
                "qml/AiPermissionGrid.qml",
                "qml/AiRequestReview.qml",
                "qml/AiSettingsEntry.qml",
                "qml/AiSettingsPage.qml",
                "qml/AmountLabel.qml",
                "qml/AmountLine.qml",
                "qml/Balance.qml",
                "qml/BankAccountNames.qml",
                "qml/BankCheckPage.qml",
                "qml/BankDialogs.qml",
                "qml/BankFetchPage.qml",
                "qml/BankLoginPage.qml",
                "qml/BankSetupPage.qml",
                "qml/BookFromFileDonePage.qml",
                "qml/KmyReportsSummary.qml",
                "qml/KmyReportsImportPage.qml",
                "qml/BookFromFilePage.qml",
                "qml/BookProblemPage.qml",
                "qml/BudgetBars.qml",
                "qml/UpdateAllWidget.qml",
                "qml/PeriodHeader.qml",
                "qml/BudgetWords.qml",
                "qml/BudgetPage.qml",
                "qml/BudgetsPage.qml",
                "qml/CategoriesPage.qml",
                "qml/CategoryGroupPage.qml",
                "qml/CategoryGroupsPage.qml",
                "qml/CategoryPage.qml",
                "qml/ChangeNames.qml",
                "qml/FormulaField.qml",
                "qml/FormulaWords.qml",
                "qml/ChartColorsCard.qml",
                "qml/ChartOverlays.qml",
                "qml/SpecPage.qml",
                "qml/ChartTable.qml",
                "qml/ChartView.qml",
                "qml/ComponentPreview.qml",
                "qml/CsvColumnBox.qml",
                "qml/DataTable.qml",
                "qml/CsvSettingsPage.qml",
                "qml/DefaultCategories.qml",
                "qml/DashboardFilePage.qml",
                "qml/ExportPage.qml",
                "qml/FormError.qml",
                "qml/FilterBar.qml",
                "qml/FilterNotes.qml",
                "qml/FilterPicker.qml",
                "qml/AddMenu.qml",
                "qml/GridNames.qml",
                "qml/ChartExamples.qml",
                "qml/TitleWords.qml",
                "qml/GridItem.qml",
                "qml/DashboardGrid.qml",
                "qml/HelpPage.qml",
                "qml/HistoryPage.qml",
                "qml/ImportDonePage.qml",
                "qml/ImportPreviewPage.qml",
                "qml/ImportStatementsPage.qml",
                "qml/MockNotice.qml",
                "qml/NewAccountPage.qml",
                "qml/OverviewPage.qml",
                "qml/DashboardTabs.qml",
                "qml/DashboardMenu.qml",
                "qml/NewDashboardDialog.qml",
                "qml/TemplatePicture.qml",
                "qml/PartRow.qml",
                "qml/PartsPage.qml",
                "qml/PropertiesPanel.qml",
                "qml/PropertiesSheet.qml",
                "qml/RegisterPage.qml",
                "qml/RemoveOriginalDialog.qml",
                "qml/ReplacedCurrency.qml",
                "qml/RuleCondition.qml",
                "qml/RulePage.qml",
                "qml/RulesApply.qml",
                "qml/RulesPage.qml",
                "qml/SecretField.qml",
                "qml/SettingsPage.qml",
                "qml/StatisticsPage.qml",
                "qml/TransactionPage.qml",
                "qml/TransactionRow.qml",
                "qml/TransferPrompt.qml",
                "qml/TransferPartnerDialog.qml",
                "qml/ExampleBookChooser.qml",
                "qml/ExampleBooksPage.qml",
                "qml/WelcomePage.qml",
                "qml/WizardAccountsPage.qml",
                "qml/WizardEntriesPage.qml",
                "qml/WizardStartPage.qml",
            ])
            // Helpers, not screens: `Plain.plain()` (D-143) and the colors of
            // the charts (D-283).
            .qml_files([
                QmlFile::from("qml/Plain.qml").singleton(true),
                QmlFile::from("qml/ChartPalette.qml").singleton(true),
                // `ActionLog.taken(name)` in every action handler (M6c.D).
                QmlFile::from("qml/ActionLog.qml").singleton(true),
            ]),
    )
    .qt_module("Network")
    .files([
        "src/accounts_model.rs",
        "src/ai_chat_model.rs",
        "src/ai_settings_model.rs",
        "src/ai_switch_model.rs",
        "src/bank_connection_model.rs",
        "src/budget_bars_model.rs",
        "src/budgets_model.rs",
        "src/transactions_tile_model.rs",
        "src/book_from_file_model.rs",
        "src/book_session.rs",
        "src/categories_model.rs",
        "src/category_groups_model.rs",
        "src/developer_mode.rs",
        "src/example_books_model.rs",
        "src/export_model.rs",
        "src/fetch_gap_model.rs",
        "src/formulas_model.rs",
        "src/first_start_model.rs",
        "src/grid_editor_model.rs",
        "src/history_model.rs",
        "src/kmy_reports_import_model.rs",
        "src/language_model.rs",
        "src/window_state_model.rs",
        "src/locale.rs",
        "src/network_guard.rs",
        "src/number_format_model.rs",
        "src/cards_model.rs",
        "src/chart_palette_model.rs",
        "src/chart_spec_model.rs",
        "src/charts_model.rs",
        "src/dashboard_file_model.rs",
        "src/overview_model.rs",
        "src/period_range_model.rs",
        "src/platform.rs",
        "src/properties_model.rs",
        "src/register_model.rs",
        "src/rules_model.rs",
        "src/saved_views_model.rs",
        "src/settings.rs",
        "src/statement_import_model.rs",
        "src/test_profile_model.rs",
    ])
    .cpp_files([
        "cpp/local_only.cpp",
        "cpp/locale.cpp",
        "cpp/platform.cpp",
        "cpp/settings.cpp",
    ]);
    // Visual Studio reads a source file in the system's code page unless told
    // otherwise, and the C++ that qmlcachegen writes from the QML keeps its
    // string literals as they are, so "↑" came out as "â†‘" (M6-B141).
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc");
    // SAFETY: only adds include folders for KConfig's and Breeze's headers
    // and, for Visual Studio, the flag that reads sources as UTF-8; cxx-qt-build's
    // own settings for the compiler don't depend on either.
    builder = unsafe {
        builder.cc_builder(|cc| {
            if msvc {
                cc.flag("/utf-8");
            }
            for dir in &kconfig.include_dirs {
                cc.include(dir);
            }
            for library in breeze_icons.iter().chain(&color_scheme) {
                for dir in &library.include_dirs {
                    cc.include(dir);
                }
            }
        })
    };
    builder.build();

    println!(
        "cargo:rustc-link-search=native={}",
        kconfig.library_dir.display()
    );
    println!("cargo:rustc-link-lib=KF6ConfigCore");
    // The program's icon on Windows (M6-B143): CMake compiles
    // packaging/windows/kassensturz.rc, and the linker of Visual Studio
    // takes the .res as it is. A plain `cargo build` has no icon.
    println!("cargo:rerun-if-env-changed=KASSENSTURZ_WINDOWS_RESOURCE");
    if let Some(resource) = std::env::var_os("KASSENSTURZ_WINDOWS_RESOURCE").filter(|_| windows) {
        println!(
            "cargo:rerun-if-changed={}",
            PathBuf::from(&resource).display()
        );
        println!(
            "cargo:rustc-link-arg-bin=kassensturz={}",
            PathBuf::from(resource).display()
        );
    }
    if let Some(breeze_icons) = &breeze_icons {
        // Breeze's icons, built into a library (D-555).
        println!(
            "cargo:rustc-link-search=native={}",
            breeze_icons.library_dir.display()
        );
        println!("cargo:rustc-link-lib=KF6BreezeIcons");
    }
    if let Some(color_scheme) = &color_scheme {
        // KDE's colour schemes, which follow Windows' dark mode (D-581).
        println!(
            "cargo:rustc-link-search=native={}",
            color_scheme.library_dir.display()
        );
        println!("cargo:rustc-link-lib=KF6ColorScheme");
    }
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

/// Where a KDE Frameworks library that only the Windows app links is: Breeze's
/// icons (D-555) and the colour schemes (D-581).
struct WindowsLibrary {
    include_dirs: Vec<PathBuf>,
    library_dir: PathBuf,
}

impl WindowsLibrary {
    /// CMake passes what its target says in `KASSENSTURZ_<name>_INCLUDE_DIRS`
    /// and `KASSENSTURZ_<name>_LIBRARY_DIR`, as for KConfig. A plain
    /// `cargo build` looks next to Qt, where KDE Craft installs the header
    /// folder `include/KF6/<headers>` and the library.
    fn find(name: &str, headers: &str) -> Self {
        let includes_variable = format!("KASSENSTURZ_{name}_INCLUDE_DIRS");
        let library_variable = format!("KASSENSTURZ_{name}_LIBRARY_DIR");
        println!("cargo:rerun-if-env-changed={includes_variable}");
        println!("cargo:rerun-if-env-changed={library_variable}");
        if let (Some(includes), Some(library_dir)) = (
            std::env::var_os(&includes_variable),
            std::env::var_os(&library_variable),
        ) {
            return Self {
                include_dirs: cmake_list(&includes),
                library_dir: library_dir.into(),
            };
        }
        Self {
            include_dirs: vec![qmake_query("QT_INSTALL_HEADERS").join(format!("KF6/{headers}"))],
            library_dir: qmake_query("QT_INSTALL_LIBS"),
        }
    }
}

/// The folders of a CMake list, which separates them with semicolons.
fn cmake_list(list: &std::ffi::OsStr) -> Vec<PathBuf> {
    list.to_string_lossy()
        .split(';')
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .collect()
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

/// The version of this build in D-152's form (D-226): the
/// `KASSENSTURZ_VERSION` variable, the file `build-version` in the
/// repository's top folder (CI writes it before the Flatpak build, as it does
/// `build-commit`), `tools/build-version.sh`, and at last the version in
/// `Cargo.toml`. A value that is not three numbers joined by points is left out.
fn build_version() -> String {
    println!("cargo:rerun-if-env-changed=KASSENSTURZ_VERSION");
    println!("cargo:rerun-if-changed=../../build-version");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    let from_script = || {
        let output = Command::new("sh")
            .arg("../../tools/build-version.sh")
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    };
    let is_version = |text: &str| {
        let parts: Vec<&str> = text.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    };
    [
        std::env::var("KASSENSTURZ_VERSION").ok(),
        std::fs::read_to_string("../../build-version").ok(),
        from_script(),
    ]
    .into_iter()
    .flatten()
    .map(|text| text.trim().to_owned())
    .find(|text| is_version(text))
    .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap_or_default())
}
