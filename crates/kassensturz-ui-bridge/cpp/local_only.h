// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The screens never reach the network (D-143, threat T-08): the QML engine
// gets a network access manager that refuses every address that isn't a
// local file or a Qt resource, so a text that Qt shows as rich text can't
// load a picture from the web even if a label lacks `textFormat`.

#pragma once

#include <QtCore/QString>

class QQmlApplicationEngine;

namespace kassensturz::network_guard
{
// Gives the engine the refusing network access manager. Call it before the
// engine loads its first file.
void installLocalOnly(QQmlApplicationEngine &engine);

// Asks a fresh refusing network access manager for `url`, waiting for the
// answer; true if it was refused rather than sent. For tests.
bool isRefused(const QString &url);
}
