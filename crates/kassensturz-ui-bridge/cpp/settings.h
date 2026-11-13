// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The app's own settings, kept with KConfig (D-112).

#pragma once

#include <QtCore/QString>

namespace kassensturz::settings
{
// `file` is a settings file's name, such as "kassensturzrc" in the user's
// settings folder, or an absolute path. An entry that isn't there reads as
// empty. The writing functions return whether the file was saved.

QString readEntry(const QString &file, const QString &group, const QString &key);

bool writeEntry(const QString &file, const QString &group, const QString &key, const QString &value);

// A path as `kassensturz_core::book_folder::remember_entry` made it: with
// "$XDG_RUNTIME_DIR" and "$HOME" for those folders and every other "$" doubled.
// Written as a plain entry; every reader expands it with `expand_entry`.
// Read it back with `readEntry`, then `expand_entry`.
bool writeExpandedEntry(const QString &file, const QString &group, const QString &key, const QString &text);
}
