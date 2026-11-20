// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import org.kde.kirigamiaddons.formcard as FormCard
import io.github.vondor.kassensturz

// A grey explanation under a setting or a form that a UI density plugin may
// hide (D-927). It is only ever for an explanation the page works without:
// a warning, an error or a question before a change is never a HintText,
// which `density_audit` checks.
FormCard.FormSectionText {
    visible: !Look.hideHints
    textFormat: Text.PlainText
}
