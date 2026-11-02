# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: BSD-2-Clause

# KDE Craft's recipe ("blueprint") for the Windows app (M7a.7, D-554).
# CI builds it from the checked-out sources with `craft --src-dir`, so the
# Git address below is only what Craft asks every blueprint for. Craft takes
# Qt 6 and KDE Frameworks from its binary cache, builds KassenSturz with the
# project's own CMake (which drives Cargo, D-063) and collects everything the
# program loads into one folder for the packages.

import info
from CraftCore import CraftCore
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
        # Windows has no icon theme: the app loads Breeze's icons from the
        # library breeze-icons builds (D-555).
        self.runtimeDependencies["kde/frameworks/tier1/breeze-icons"] = None
        # Plasma's look, which main.rs asks for (org.kde.desktop).
        self.runtimeDependencies["kde/frameworks/tier3/qqc2-desktop-style"] = None
        # AqBanking and Gwenhywfar are not in Craft's cache for Visual Studio,
        # so this build has no bank connections; the app says so (D-054, D-554).


class Package(CMakePackageBase):
    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.subinfo.options.configure.args += [
            # Gwenhywfar's Qt dialogs need Gwenhywfar, which isn't here.
            "-DKASSENSTURZ_BANK_DIALOGS=OFF",
        ]

    def createPackage(self):
        self.defines["appname"] = "kassensturz"
        self.defines["company"] = "KassenSturz contributors"
        self.defines["executable"] = r"bin\kassensturz.exe"
        self.defines["icon"] = self.blueprintDir().parent.parent / "kassensturz.ico"
        self.defines["website"] = "https://github.com/Vondor/KassenSturz"
        self.blacklist_file.append(self.blueprintDir() / "blacklist.txt")
        return super().createPackage()
