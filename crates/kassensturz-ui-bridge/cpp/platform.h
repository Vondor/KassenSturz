// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What the app learns about the computer from Qt, until
// `kassensturz-platform` takes it over (docs/ARCHITECTURE.md).

#pragma once

#include <QtCore/QString>

namespace kassensturz::platform
{
// The folder for apps' data, e.g. ~/.local/share; empty if Qt found none.
QString dataFolder();

QString computerName();

// Gives every window the icon the icon theme has under `name`. Without one
// in the theme, the windows keep Qt's default.
void setWindowIcon(const QString &name);

// The name of the file system `path` is on, e.g. "ext4" or "cifs"; empty if
// Qt can't tell.
QString fileSystemType(const QString &path);

// Loads KI18n's QML library on the calling thread. Qt otherwise loads it on
// its QML loader thread, and a static object inside it then lives there while
// it filters the application's events, which Qt warns about at every start.
// Call it on the main thread before the first QML file loads. If the library
// can't be found, nothing happens: Qt loads it later, as before.
void loadKI18nQml();

// On Windows, which has no icon theme, makes Breeze's icons (built into
// KF6BreezeIcons) the theme and lets Qt find the app's own icon next to the
// program, in data/icons (D-555). Does nothing elsewhere, where the desktop's
// icon theme is used. Call it before the first window is shown.
void useBundledIcons();

// On Windows, Qt's Fusion style for the widgets that Plasma's QML style
// (org.kde.desktop) draws its controls with, unless the user chose a style
// with -style or QT_STYLE_OVERRIDE. Windows' own style drew a chosen radio
// button as an empty white circle there (M6-B142). Does nothing elsewhere.
// Call it after the QApplication exists and before the first window.
void useReadableWidgetStyle();

// On Windows, makes the user's Downloads folder (or else their home folder)
// the working folder when the app was started in the system's folder or in
// its own program folder: Windows' file dialogs open in the working folder
// when a dialog is given none, and from PowerShell that was
// C:\Windows\System32 (M6-B147). The app opens no file by a relative path,
// so nothing else depends on it. Does nothing elsewhere.
void startInUserFolder();
}
