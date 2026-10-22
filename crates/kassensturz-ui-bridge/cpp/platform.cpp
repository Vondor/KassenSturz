// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "kassensturz-ui-bridge/cpp/platform.h"

#include <QtCore/QStandardPaths>
#include <QtCore/QStorageInfo>
#include <QtCore/QSysInfo>

namespace kassensturz::platform
{
QString dataFolder()
{
    return QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation);
}

QString computerName()
{
    return QSysInfo::machineHostName();
}

QString fileSystemType(const QString &path)
{
    const QStorageInfo storage(path);
    return storage.isValid() ? QString::fromUtf8(storage.fileSystemType()) : QString();
}
}
