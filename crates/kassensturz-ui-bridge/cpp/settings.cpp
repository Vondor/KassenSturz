// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "kassensturz-ui-bridge/cpp/settings.h"

#include <KConfigGroup>
#include <KSharedConfig>

namespace kassensturz::settings
{
namespace
{
KConfigGroup group(const QString &file, const QString &name)
{
    // Only this file: the system-wide KDE settings ("kdeglobals") hold
    // nothing KassenSturz reads.
    return KSharedConfig::openConfig(file, KConfig::SimpleConfig)->group(name);
}
}

QString readEntry(const QString &file, const QString &group, const QString &key)
{
    return settings::group(file, group).readEntry(key, QString());
}

bool writeEntry(const QString &file, const QString &group, const QString &key, const QString &value)
{
    KConfigGroup entries = settings::group(file, group);
    entries.writeEntry(key, value);
    return entries.sync();
}

bool writeExpandedEntry(const QString &file, const QString &group, const QString &key, const QString &text)
{
    KConfigGroup entries = settings::group(file, group);
    // KConfig's own `writePathEntry` can only put `$HOME` in, and doubles every
    // other `$`; `text` already holds the variables it should keep, so it is
    // written as a plain entry and read back by `expand_entry`, which every
    // reader of this entry uses. An older entry written by `writePathEntry`
    // (`Key[$e]`) goes first, so the file never holds the key twice.
    entries.deleteEntry(key);
    entries.writeEntry(key, text);
    return entries.sync();
}
}
