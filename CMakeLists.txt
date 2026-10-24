# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: BSD-2-Clause

# CMake drives Cargo (D-063): Cargo builds the program, CMake installs what a
# desktop app needs besides it (desktop file, AppStream data, icons, translations).

cmake_minimum_required(VERSION 3.27)

project(kassensturz VERSION 0.261023.0 LANGUAGES CXX)

# The oldest versions that are tested: Qt 6.11 from the KDE Flatpak runtime
# 6.11 (D-025, D-063), and KDE Frameworks 6.29, the version in Kubuntu 26.10,
# which CI's Kubuntu job builds against (D-094). The Flatpak has 6.30.
set(QT_MIN_VERSION "6.11.0")
set(KF_MIN_VERSION "6.29.0")

find_package(ECM ${KF_MIN_VERSION} REQUIRED NO_MODULE)
set(CMAKE_MODULE_PATH ${ECM_MODULE_PATH})

include(KDEInstallDirs)
include(ECMUninstallTarget)
include(ECMFindQmlModule)
include(FeatureSummary)

find_package(Qt6 ${QT_MIN_VERSION} REQUIRED COMPONENTS Core Gui Network Qml Quick QuickControls2 Widgets)
# The Rust program links KConfig's core library for the app's settings
# (D-112); Cargo finds it through the paths passed to it below.
find_package(KF6 ${KF_MIN_VERSION} REQUIRED COMPONENTS Config I18n)
find_package(KF6QQC2DesktopStyle ${KF_MIN_VERSION})
set_package_properties(KF6QQC2DesktopStyle PROPERTIES
    TYPE RUNTIME
    PURPOSE "Gives the window Plasma's look"
)
# The QML code loads these at run time. Kirigami Addons gives the form pages
# (D-112); 1.13 is the oldest version tested, Kubuntu 26.10's. No version is
# asked for here: checking it runs qmlplugindump, which fails on this module
# in the KDE Flatpak SDK.
ecm_find_qmlmodule(org.kde.kirigami REQUIRED)
ecm_find_qmlmodule(org.kde.kirigamiaddons.formcard REQUIRED)
ecm_find_qmlmodule(org.kde.ki18n REQUIRED)
ecm_find_qmlmodule(QtQuick.Dialogs REQUIRED)

find_program(CARGO_EXECUTABLE cargo REQUIRED)

option(KASSENSTURZ_BUNDLED_SQLITE
    "Build the SQLite that rusqlite bundles; OFF uses the system's SQLite (D-061)" ON)
option(KASSENSTURZ_BANK_DIALOGS
    "Build the library for Gwenhywfar's Qt 6 bank dialogs where gwengui-qt6 is installed (D-054, Q-035)" ON)
option(KASSENSTURZ_INSTALL_SVG_ICON
    "Install the scalable SVG of the app icon next to its PNG sizes; the Flatpak turns it off, because AppStream in the Flatpak build image can't read SVG (D-165)" ON)

set(cargo_target_dir "${CMAKE_CURRENT_BINARY_DIR}/cargo")
set(cargo_args build --locked --package kassensturz-ui-bridge --bin kassensturz)
if(NOT KASSENSTURZ_BUNDLED_SQLITE)
    list(APPEND cargo_args --no-default-features)
endif()
if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(cargo_profile_dir debug)
else()
    list(APPEND cargo_args --release)
    set(cargo_profile_dir release)
endif()

# Cargo decides itself what needs rebuilding, so it runs on every build.
add_custom_target(kassensturz ALL
    COMMAND ${CMAKE_COMMAND} -E env
        QMAKE=$<TARGET_FILE:Qt6::qmake>
        "KASSENSTURZ_KCONFIG_INCLUDE_DIRS=$<TARGET_PROPERTY:KF6::ConfigCore,INTERFACE_INCLUDE_DIRECTORIES>"
        KASSENSTURZ_KCONFIG_LIBRARY_DIR=$<TARGET_LINKER_FILE_DIR:KF6::ConfigCore>
        CARGO_TARGET_DIR=${cargo_target_dir}
        ${CARGO_EXECUTABLE} ${cargo_args}
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
    USES_TERMINAL
    COMMENT "Building KassenSturz with Cargo"
    VERBATIM
)

install(PROGRAMS "${cargo_target_dir}/${cargo_profile_dir}/kassensturz" DESTINATION ${KDE_INSTALL_BINDIR})

# Gwenhywfar's Qt 6 dialogs, wrapped in a small C++ library that the app
# loads at runtime like AqBanking itself (Q-035, D-140). Its test runs
# under ctest.
enable_testing()
if(KASSENSTURZ_BANK_DIALOGS)
    add_subdirectory(crates/kassensturz-aqbanking-sys/gwengui)
endif()
install(FILES data/io.github.vondor.KassenSturz.desktop DESTINATION ${KDE_INSTALL_APPDIR})
install(FILES data/io.github.vondor.KassenSturz.metainfo.xml DESTINATION ${KDE_INSTALL_METAINFODIR})
# The app icon in the freedesktop icon theme layout: an SVG and PNG sizes (D-165).
if(KASSENSTURZ_INSTALL_SVG_ICON)
    install(DIRECTORY data/icons/hicolor DESTINATION ${KDE_INSTALL_ICONDIR})
else()
    install(DIRECTORY data/icons/hicolor DESTINATION ${KDE_INSTALL_ICONDIR} PATTERN "scalable" EXCLUDE)
endif()
ki18n_install(po)

feature_summary(WHAT ALL INCLUDE_QUIET_PACKAGES FATAL_ON_MISSING_REQUIRED_PACKAGES)
