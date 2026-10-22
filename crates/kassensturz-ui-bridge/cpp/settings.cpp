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

QString readPathEntry(const QString &file, const QString &group, const QString &key)
{
    return settings::group(file, group).readPathEntry(key, QString());
}

bool writePathEntry(const QString &file, const QString &group, const QString &key, const QString &path)
{
    KConfigGroup entries = settings::group(file, group);
    entries.writePathEntry(key, path);
    return entries.sync();
}
}
