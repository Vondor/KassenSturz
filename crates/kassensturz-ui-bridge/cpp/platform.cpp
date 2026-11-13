// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "kassensturz-ui-bridge/cpp/platform.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QFile>
#include <QtCore/QLibrary>
#include <QtCore/QLocale>
#include <QtCore/QProcess>
#include <QtCore/QProcessEnvironment>
#include <QtCore/QStandardPaths>
#include <QtCore/QStorageInfo>
#include <QtCore/QStringList>
#include <QtCore/QSysInfo>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>

#include <cerrno>
#include <cstdio>

#ifdef KASSENSTURZ_WITH_DBUS
#include <QtDBus/QDBusArgument>
#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusMessage>
#endif

#ifdef Q_OS_UNIX
#include <fcntl.h>
#include <unistd.h>
#endif

#ifdef Q_OS_WIN
#include <QtCore/QCoreApplication>
#include <QtCore/QDir>
#include <QtWidgets/QApplication>
#include <QtGui/QPainter>
#include <QtGui/QPainterPath>
#include <QtGui/QStyleHints>
#include <QtWidgets/QProxyStyle>
#include <QtWidgets/QStyle>
#include <QtWidgets/QStyleFactory>
#include <QtWidgets/QStyleOption>

#include <KColorSchemeManager>
#include <breezeicons.h>

#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

namespace kassensturz::platform
{
#ifdef Q_OS_WIN
namespace
{
// Fusion with one change: in dark mode it draws a radio button's ring a
// little darker than the window and its dot in dimmed text colour, so on
// Breeze Dark the empty circles all but vanished and a chosen one showed a
// faint grey dot (M6-B154). Fusion's own check box already draws a lighter
// outline in dark mode; the radio button now does the same, with the dot in
// the selection colour. Light mode stays exactly Fusion's.
class ReadableFusion : public QProxyStyle
{
public:
    ReadableFusion()
        : QProxyStyle(QStyleFactory::create(QStringLiteral("Fusion")))
    {
    }

    void drawPrimitive(PrimitiveElement element, const QStyleOption *option, QPainter *painter, const QWidget *widget) const override
    {
        if (element != PE_IndicatorRadioButton || QGuiApplication::styleHints()->colorScheme() != Qt::ColorScheme::Dark) {
            QProxyStyle::drawPrimitive(element, option, painter, widget);
            return;
        }
        painter->save();
        painter->setRenderHint(QPainter::Antialiasing, true);
        const QRect &rect = option->rect;
        const QPointF center = rect.center() + QPoint(1, 1);
        const qreal radius = (rect.width() + (rect.width() + 1) % 2) / 2.0 - 1;
        const QPalette &palette = option->palette;
        painter->setBrush((option->state & State_Sunken) ? palette.window() : palette.base());
        const bool keyboardFocus = (option->state & State_HasFocus) && (option->state & State_KeyboardFocusChange);
        painter->setPen(keyboardFocus ? palette.highlight().color() : palette.text().color());
        painter->drawEllipse(center, radius, radius);
        if (option->state & State_On) {
            painter->setPen(Qt::NoPen);
            painter->setBrush(palette.highlight());
            const qreal dot = radius / 2.0;
            painter->drawEllipse(center, dot, dot);
        }
        painter->restore();
    }
};
} // namespace
#endif

QString dataFolder()
{
    return QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation);
}

QString appFilePath()
{
    return QCoreApplication::applicationFilePath();
}

bool startDetached(const QString &program, const QStringList &arguments, const QStringList &environment)
{
    QProcess process;
    process.setProgram(program);
    process.setArguments(arguments);
    if (!environment.isEmpty()) {
        QProcessEnvironment whole;
        for (const QString &entry : environment) {
            const qsizetype equals = entry.indexOf(QLatin1Char('='));
            if (equals > 0) {
                whole.insert(entry.left(equals), entry.mid(equals + 1));
            }
        }
        process.setProcessEnvironment(whole);
    }
    return process.startDetached();
}

void quitApp()
{
    QCoreApplication::quit();
}

void useTestLocations()
{
    QStandardPaths::setTestModeEnabled(true);
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

std::int64_t freeSpace(const QString &path)
{
    QStorageInfo storage(path);
    storage.refresh();
    // A drive that reports no size, such as some FUSE file systems, would
    // read as full.
    if (!storage.isValid() || !storage.isReady() || storage.bytesTotal() <= 0) {
        return -1;
    }
    return storage.bytesAvailable();
}

QString dataSize(std::int64_t bytes)
{
    return QLocale().formattedDataSize(bytes);
}

int renameNoReplace(const QString &from, const QString &to)
{
#ifdef Q_OS_WIN
    const auto source = reinterpret_cast<const wchar_t *>(from.utf16());
    const auto target = reinterpret_cast<const wchar_t *>(to.utf16());
    // Without MOVEFILE_REPLACE_EXISTING, Windows refuses an existing target.
    if (MoveFileExW(source, target, MOVEFILE_WRITE_THROUGH)) {
        return 0;
    }
    return static_cast<int>(GetLastError());
#else
    const QByteArray source = QFile::encodeName(from);
    const QByteArray target = QFile::encodeName(to);
#if defined(Q_OS_LINUX)
    if (::renameat2(AT_FDCWD, source.constData(), AT_FDCWD, target.constData(), RENAME_NOREPLACE) == 0) {
        return 0;
    }
    // EINVAL: the file system doesn't take the flag, e.g. some FUSE ones.
    if (errno != EINVAL && errno != ENOSYS) {
        return errno;
    }
#elif defined(Q_OS_DARWIN)
    if (::renamex_np(source.constData(), target.constData(), RENAME_EXCL) == 0) {
        return 0;
    }
    if (errno != ENOTSUP && errno != EINVAL) {
        return errno;
    }
#endif
    // A hard link refuses an existing name too; then the old name goes. A
    // file system without hard links refuses the move instead.
    if (::link(source.constData(), target.constData()) != 0) {
        return errno;
    }
    if (::unlink(source.constData()) != 0) {
        const int error = errno;
        ::unlink(target.constData());
        return error;
    }
    return 0;
#endif
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
    // Qt's QApplication takes ownership of the style.
    QApplication::setStyle(new ReadableFusion);
#endif
}

void followSystemColours()
{
#ifdef Q_OS_WIN
    // With no colour scheme chosen, KDE's manager applies Breeze or Breeze
    // Dark after Windows' own light or dark mode, and again whenever that
    // changes; both schemes are built into the library. It sets the palette
    // that Qt's widgets and Kirigami's colours both read, so they agree.
    KColorSchemeManager::instance();
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

QString portalHostPath(const QString &documentId)
{
#ifdef KASSENSTURZ_WITH_DBUS
    // The portal's Documents interface (version 5) answers GetHostPaths
    // with the path on the host for each document ID it is asked for:
    // `as` in, `a{say}` out, the paths as byte strings ending with a NUL.
    // It is asked by ID, never by path: a path gets no answer (M6-B240).
    if (documentId.isEmpty()) {
        return QString();
    }
    QDBusMessage call = QDBusMessage::createMethodCall(
        QStringLiteral("org.freedesktop.portal.Documents"),
        QStringLiteral("/org/freedesktop/portal/documents"),
        QStringLiteral("org.freedesktop.portal.Documents"),
        QStringLiteral("GetHostPaths"));
    call << QStringList{documentId};

    const QDBusMessage reply = QDBusConnection::sessionBus().call(call, QDBus::Block, 2000);
    if (reply.type() != QDBusMessage::ReplyMessage || reply.arguments().isEmpty()) {
        return QString();
    }
    const QDBusArgument answer = reply.arguments().constFirst().value<QDBusArgument>();
    QString found;
    answer.beginMap();
    while (!answer.atEnd()) {
        QString key;
        QByteArray value;
        answer.beginMapEntry();
        answer >> key >> value;
        answer.endMapEntry();
        if (key == documentId) {
            if (value.endsWith('\0')) {
                value.chop(1);
            }
            found = QFile::decodeName(value);
        }
    }
    answer.endMap();
    return found;
#else
    Q_UNUSED(documentId)
    return QString();
#endif
}
}
