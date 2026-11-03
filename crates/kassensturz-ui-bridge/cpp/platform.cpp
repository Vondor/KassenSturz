// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "kassensturz-ui-bridge/cpp/platform.h"

#include <QtCore/QLibrary>
#include <QtCore/QStandardPaths>
#include <QtCore/QStorageInfo>
#include <QtCore/QSysInfo>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>

#ifdef Q_OS_WIN
#include <QtCore/QCoreApplication>
#include <QtCore/QDir>
#include <QtWidgets/QApplication>
#include <QtWidgets/QStyle>

#include <breezeicons.h>
#endif

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

void useBundledIcons()
{
#ifdef Q_OS_WIN
    QStringList paths = QIcon::themeSearchPaths();
    paths.append(QDir(QCoreApplication::applicationDirPath()).filePath(QStringLiteral("data/icons")));
    QIcon::setThemeSearchPaths(paths);
    BreezeIcons::initIcons();
    QIcon::setThemeName(QStringLiteral("breeze"));
#endif
}

void useReadableWidgetStyle()
{
#ifdef Q_OS_WIN
    if (qEnvironmentVariableIsSet("QT_STYLE_OVERRIDE")) {
        return;
    }
    const QStringList arguments = QCoreApplication::arguments();
    for (const QString &argument : arguments) {
        if (argument == QLatin1StringView("-style") || argument.startsWith(QLatin1StringView("-style="))) {
            return;
        }
    }
    QApplication::setStyle(QStringLiteral("Fusion"));
#endif
}

void startInUserFolder()
{
#ifdef Q_OS_WIN
    const QString current = QDir::cleanPath(QDir::currentPath());
    const QString systemRoot = QDir::cleanPath(QDir::fromNativeSeparators(qEnvironmentVariable("SystemRoot")));
    const QString programFolder = QDir::cleanPath(QCoreApplication::applicationDirPath());
    const bool inSystem = !systemRoot.isEmpty()
        && (current.compare(systemRoot, Qt::CaseInsensitive) == 0
            || current.startsWith(systemRoot + QLatin1Char('/'), Qt::CaseInsensitive));
    const bool inProgram = current.compare(programFolder, Qt::CaseInsensitive) == 0;
    if (!inSystem && !inProgram) {
        return;
    }
    for (const auto location : {QStandardPaths::DownloadLocation, QStandardPaths::HomeLocation}) {
        const QString folder = QStandardPaths::writableLocation(location);
        if (!folder.isEmpty() && QDir::setCurrent(folder)) {
            return;
        }
    }
#endif
}
}
