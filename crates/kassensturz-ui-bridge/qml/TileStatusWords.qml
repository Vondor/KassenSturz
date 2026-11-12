// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The words of the status icons (US5.4, D-734, `docs/STATUS_TILES.md` §2 and
// §5): short words for beside the icon and the screen reader, and one plain
// sentence for the hover text, per tile type and state. Rust sends only the
// state word and a count (D-034).

import QtQuick
import org.kde.ki18n

QtObject {
    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property string clickHint: i18nContext.i18nc("@info:tooltip on a tile shown as a small status icon", "Click to see the whole tile.")

    // The short words.
    function words(type: string, status: string, count: int): string {
        if (status === "running") {
            return i18nContext.i18nc("@info a status icon: bank data is being fetched", "Updating…");
        }
        switch (type) {
        case "notice.alarms":
            switch (status) {
            case "fine":
                return i18nContext.i18nc("@info a status icon: no warning applies", "All fine");
            case "to-do":
                return i18nContext.i18ncp("@info a status icon, %1 is how many", "%1 warning applies", "%1 warnings apply", count);
            case "failed":
                return i18nContext.i18nc("@info a status icon", "Warnings can't be read");
            default:
                return count > 0
                    ? i18nContext.i18ncp("@info a status icon, %1 is how many", "%1 warning can't be checked", "%1 warnings can't be checked", count)
                    : i18nContext.i18nc("@info a status icon", "No warnings set yet");
            }
        case "notice.unsorted":
            switch (status) {
            case "fine":
                return i18nContext.i18nc("@info a status icon: every transaction has a category", "All sorted");
            case "to-do":
                return i18nContext.i18ncp("@info a status icon, %1 is how many", "%1 without a category", "%1 without a category", count);
            default:
                return i18nContext.i18nc("@info a status icon", "Not known yet");
            }
        case "notice.fetch_gap":
            switch (status) {
            case "fine":
                return i18nContext.i18nc("@info a status icon: every bank account was updated recently", "Bank data is recent");
            case "to-do":
                return i18nContext.i18ncp("@info a status icon, %1 is how many", "%1 account not updated for long", "%1 accounts not updated for long", count);
            default:
                return i18nContext.i18nc("@info a status icon", "Not known yet");
            }
        case "bank.update_all":
            switch (status) {
            case "fine":
                return i18nContext.i18nc("@info a status icon: the last bank update worked", "Last update worked");
            case "to-do":
                return i18nContext.i18ncp("@info a status icon, %1 is how many", "%1 notice to check", "%1 notices to check", count);
            case "failed":
                return i18nContext.i18nc("@info a status icon", "Last update failed");
            default:
                return i18nContext.i18nc("@info a status icon: no account can be updated from a bank yet", "Not set up yet");
            }
        }
        return "";
    }

    // The hover sentence, before the click hint.
    function sentence(type: string, status: string, count: int): string {
        if (status === "running") {
            return i18nContext.i18nc("@info:tooltip", "New transactions are being fetched from your bank right now.");
        }
        switch (type) {
        case "notice.alarms":
            switch (status) {
            case "fine":
                return i18nContext.i18ncp("@info:tooltip %1 is how many", "All fine. Your warning doesn't apply today.", "All fine. None of your %1 warnings applies today.", count);
            case "to-do":
                return i18nContext.i18ncp("@info:tooltip %1 is how many", "%1 of your warnings applies today.", "%1 of your warnings apply today.", count);
            case "failed":
                return i18nContext.i18nc("@info:tooltip", "The warnings of this tile could not be read.");
            default:
                return count > 0
                    ? i18nContext.i18ncp("@info:tooltip %1 is how many", "%1 warning could not be checked today.", "%1 warnings could not be checked today.", count)
                    : i18nContext.i18nc("@info:tooltip", "No warnings set yet. You can add one in this tile's settings.");
            }
        case "notice.unsorted":
            return status === "to-do"
                ? i18nContext.i18ncp("@info:tooltip %1 is how many", "%1 transaction has no category yet.", "%1 transactions have no category yet.", count)
                : status === "fine"
                    ? i18nContext.i18nc("@info:tooltip", "Every transaction has a category.")
                    : "";
        case "notice.fetch_gap":
            return status === "to-do"
                ? i18nContext.i18ncp("@info:tooltip %1 is how many", "%1 bank account was not updated for a long time.", "%1 bank accounts were not updated for a long time.", count)
                : status === "fine"
                    ? i18nContext.i18nc("@info:tooltip", "Every bank account was updated recently.")
                    : "";
        case "bank.update_all":
            switch (status) {
            case "fine":
                return i18nContext.i18nc("@info:tooltip", "The last update of your bank accounts worked.");
            case "to-do":
                return i18nContext.i18ncp("@info:tooltip %1 is how many", "The last update brought %1 notice to check.", "The last update brought %1 notices to check.", count);
            case "failed":
                return count > 0
                    ? i18nContext.i18ncp("@info:tooltip %1 is how many", "The last update didn't work for %1 account. You can try again.", "The last update didn't work for %1 accounts. You can try again.", count)
                    : i18nContext.i18nc("@info:tooltip", "The last update didn't work. You can try again.");
            default:
                return i18nContext.i18nc("@info:tooltip", "No account is updated from a bank yet, or bank connections can't be used here.");
            }
        }
        return "";
    }
}
