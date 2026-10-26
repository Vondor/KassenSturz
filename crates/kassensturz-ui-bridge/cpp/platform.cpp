// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "kassensturz-ui-bridge/cpp/platform.h"

#include <QtCore/QLibrary>
#include <QtCore/QStandardPaths>
#include <QtCore/QStorageInfo>
#include <QtCore/QSysInfo>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>

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

void setWindowIcon(const QString &name)
{
    const QIcon icon = QIcon::fromTheme(name);
    if (!icon.isNull()) {
        QGuiApplication::setWindowIcon(icon);
    }
}

void loadKI18nQml()
{
    // Never freed: the library has to stay loaded for the QML plugin.
    auto *library = new QLibrary(QStringLiteral("KF6I18nQml"), 6);
    library->load();
}

QString fileSystemType(const QString &path)
{
    const QStorageInfo storage(path);
    return storage.isValid() ? QString::fromUtf8(storage.fileSystemType()) : QString();
}
}
