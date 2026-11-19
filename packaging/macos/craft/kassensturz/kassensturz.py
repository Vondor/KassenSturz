# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: BSD-2-Clause

# KDE Craft's recipe ("blueprint") for the experimental macOS app (D-875).
# CI builds it from the checked-out sources with `craft --src-dir`, so the
# Git address below is only what Craft asks every blueprint for. Craft takes
# Qt 6 and KDE Frameworks from its binary cache and builds KassenSturz with
# the project's own CMake (which drives Cargo, D-063) into Craft's folder.
# The .app and the .dmg are made from that folder by
# packaging/macos/make-dmg.sh, not by Craft's packagers.

import info
from Package.CMakePackageBase import CMakePackageBase


class subinfo(info.infoclass):
    def setTargets(self):
        self.svnTargets["main"] = "https://github.com/Vondor/KassenSturz.git"
        self.defaultTarget = "main"
        self.description = "Personal finance for everyday people"
        self.displayName = "KassenSturz"

    def setDependencies(self):
        self.buildDependencies["virtual/base"] = None
        self.buildDependencies["kde/frameworks/extra-cmake-modules"] = None
        # ki18n_install() turns po/ into .mo files with gettext's msgfmt.
        self.buildDependencies["libs/gettext"] = None
        self.runtimeDependencies["libs/qt/qtbase"] = None
        self.runtimeDependencies["libs/qt/qtdeclarative"] = None
        self.runtimeDependencies["libs/qt/qtsvg"] = None
        self.runtimeDependencies["kde/frameworks/tier1/kconfig"] = None
        self.runtimeDependencies["kde/frameworks/tier1/ki18n"] = None
        self.runtimeDependencies["kde/frameworks/tier1/kirigami"] = None
        self.runtimeDependencies["kde/unreleased/kirigami-addons"] = None
        # macOS has no icon theme: the app loads Breeze's icons from the
        # library breeze-icons builds (as on Windows, D-555).
        self.runtimeDependencies["kde/frameworks/tier1/breeze-icons"] = None
        # Breeze and Breeze Dark after macOS' light or dark mode.
        self.runtimeDependencies["kde/frameworks/tier2/kcolorscheme"] = None
        # The charts load org.kde.quickcharts (ChartView.qml).
        self.runtimeDependencies["kde/frameworks/tier1/kquickcharts"] = None
        # Plasma's look, which main.rs asks for (org.kde.desktop).
        self.runtimeDependencies["kde/frameworks/tier3/qqc2-desktop-style"] = None
        # No AqBanking on this build, so no bank connections (D-054, D-875).


class Package(CMakePackageBase):
    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.subinfo.options.configure.args += [
            # Gwenhywfar's Qt dialogs need Gwenhywfar, which isn't here.
            "-DKASSENSTURZ_BANK_DIALOGS=OFF",
        ]
