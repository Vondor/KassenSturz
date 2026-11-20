// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A theme plugin's colours and font for the whole app (#791 step 6, D-925).

#pragma once

#include <QtCore/QString>

namespace kassensturz::theme
{
// Whether the system asks for a dark look. Call it after the QApplication
// exists.
bool systemPrefersDark();

// Uses the KDE colour scheme file at `schemePath` for Kirigami's colours,
// `palette` ("Role=#rrggbb;...", Qt's role names) for Qt's palette, and the
// font family `font` if this computer has it (empty keeps the system's).
// Call it after the QApplication exists and before the first window, in
// place of `platform::followSystemColours`.
void applyTheme(const QString &schemePath, const QString &palette, const QString &font);
}
