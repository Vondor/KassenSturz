// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "kassensturz-ui-bridge/cpp/theme.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QHash>
#include <QtCore/QStringList>
#include <QtGui/QColor>
#include <QtGui/QFont>
#include <QtGui/QFontDatabase>
#include <QtGui/QGuiApplication>
#include <QtGui/QPalette>
#include <QtGui/QStyleHints>

namespace kassensturz::theme
{
bool systemPrefersDark()
{
    return QGuiApplication::styleHints()->colorScheme() == Qt::ColorScheme::Dark;
}

void applyTheme(const QString &schemePath, const QString &palette, const QString &font)
{
    // KColorScheme reads the scheme named by this property of the
    // application, which is how KDE's own colour scheme manager switches an
    // app's colours; Kirigami's Plasma style takes its colours from there.
    qApp->setProperty("KDE_COLOR_SCHEME_PATH", schemePath);

    static const QHash<QString, QPalette::ColorRole> roles = {
        {QStringLiteral("Window"), QPalette::Window},
        {QStringLiteral("WindowText"), QPalette::WindowText},
        {QStringLiteral("Base"), QPalette::Base},
        {QStringLiteral("AlternateBase"), QPalette::AlternateBase},
        {QStringLiteral("Text"), QPalette::Text},
        {QStringLiteral("Button"), QPalette::Button},
        {QStringLiteral("ButtonText"), QPalette::ButtonText},
        {QStringLiteral("Highlight"), QPalette::Highlight},
        {QStringLiteral("HighlightedText"), QPalette::HighlightedText},
        {QStringLiteral("Link"), QPalette::Link},
        {QStringLiteral("LinkVisited"), QPalette::LinkVisited},
        {QStringLiteral("ToolTipBase"), QPalette::ToolTipBase},
        {QStringLiteral("ToolTipText"), QPalette::ToolTipText},
        {QStringLiteral("PlaceholderText"), QPalette::PlaceholderText},
        {QStringLiteral("Accent"), QPalette::Accent},
    };
    QPalette colours = QGuiApplication::palette();
    const QStringList entries = palette.split(QLatin1Char(';'), Qt::SkipEmptyParts);
    for (const QString &entry : entries) {
        const qsizetype equals = entry.indexOf(QLatin1Char('='));
        if (equals < 0) {
            continue;
        }
        const auto role = roles.constFind(entry.left(equals));
        const QColor colour = QColor::fromString(entry.mid(equals + 1));
        if (role == roles.constEnd() || !colour.isValid()) {
            continue;
        }
        colours.setColor(QPalette::Active, *role, colour);
        colours.setColor(QPalette::Inactive, *role, colour);
        if (*role == QPalette::Window || *role == QPalette::Base || *role == QPalette::Button) {
            colours.setColor(QPalette::Disabled, *role, colour);
        }
    }
    // Disabled text: the secondary text colour.
    const QColor placeholder = colours.color(QPalette::Active, QPalette::PlaceholderText);
    for (const auto role : {QPalette::WindowText, QPalette::Text, QPalette::ButtonText}) {
        colours.setColor(QPalette::Disabled, role, placeholder);
    }
    QGuiApplication::setPalette(colours);

    if (!font.isEmpty() && QFontDatabase::hasFamily(font)) {
        QFont chosen = QGuiApplication::font();
        chosen.setFamilies({font});
        QGuiApplication::setFont(chosen);
    }
}
}
