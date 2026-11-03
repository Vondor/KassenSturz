// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What the app learns about the computer from Qt, until
// `kassensturz-platform` takes it over (docs/ARCHITECTURE.md).

#pragma once

#include <QtCore/QString>
#include <QtCore/QStringList>

namespace kassensturz::platform
{
// The folder for apps' data, e.g. ~/.local/share; empty if Qt found none.
QString dataFolder();

// The app's own executable file.
QString appFilePath();

// Starts `program` with `arguments` on its own, outliving this process. An
// empty `environment` ("NAME=value" entries) keeps this process's; otherwise
// it is the new process's whole environment. Returns whether it started.
bool startDetached(const QString &program, const QStringList &arguments, const QStringList &environment);

// Ends the event loop, so the app ends.
void quitApp();

// Switches QStandardPaths to Qt's test locations, apart from the user's real
// ones: the test profile on Windows (M7a.9).
void useTestLocations();

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
}
