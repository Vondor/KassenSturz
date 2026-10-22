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

// The name of the file system `path` is on, e.g. "ext4" or "cifs"; empty if
// Qt can't tell.
QString fileSystemType(const QString &path);
}
