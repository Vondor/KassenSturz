// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The words for a Rule the AI made that the user hasn't opened yet (B-249),
// in a file of its own so the Rules page holds no AI text: they show only
// while AI is on (D-013); while it is off the Rule's own summary is all.

import QtQuick
import org.kde.ki18n
import io.github.vondor.kassensturz

Item {
    id: mark

    required property AiSwitch ai

    visible: ai.enabled

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function note(summary: string): string {
        if (!mark.ai.enabled)
            return summary;
        return i18nContext.i18nc("@info %1 says what a Rule does", "Made by the AI, not checked by you yet. %1", summary);
    }
}
