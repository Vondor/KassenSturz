// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One plugin as its manifest describes it (D-847), on the Plugins page:
// its name and version, who made it, its description, what it adds and what
// it may do. Every text from the plugin file is untrusted and shown as plain
// text only (AGENTS.md §8); the lists of what it adds and may do come
// already in the page's own words.

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n

ColumnLayout {
    id: details

    // An object of `Plugins.items` or `Plugins.pending`.
    property var plugin: ({})
    // What it adds and what it may do, one line each.
    property string pointsText: ""
    property string permissionsText: ""

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    Layout.fillWidth: true
    spacing: 0

    FormCard.FormTextDelegate {
        text: details.i18nContext.i18nc("@info %1 is a plugin's name, %2 its version", "%1, version %2", details.plugin.name ?? "", details.plugin.version ?? "")
        textItem.textFormat: Text.PlainText
        textItem.wrapMode: Text.Wrap
        description: (details.plugin.author ?? "") === ""
            ? ""
            : details.i18nContext.i18nc("@info %1 is who made a plugin, as the plugin says", "Made by %1, as the plugin says", details.plugin.author)
        descriptionItem.textFormat: Text.PlainText
    }

    FormCard.FormTextDelegate {
        visible: (details.plugin.description ?? "") !== ""
        text: details.plugin.description ?? ""
        textItem.textFormat: Text.PlainText
        textItem.wrapMode: Text.Wrap
    }

    FormCard.FormTextDelegate {
        text: details.i18nContext.i18nc("@title:group", "What it adds")
        description: details.pointsText
        descriptionItem.textFormat: Text.PlainText
        descriptionItem.wrapMode: Text.Wrap
    }

    FormCard.FormTextDelegate {
        text: details.i18nContext.i18nc("@title:group", "What it may do")
        description: details.permissionsText === ""
            ? details.i18nContext.i18nc("@info", "Nothing beyond what it adds: it can't read your book or use the internet.")
            : details.permissionsText
        descriptionItem.textFormat: Text.PlainText
        descriptionItem.wrapMode: Text.Wrap
    }

    FormCard.FormTextDelegate {
        visible: (details.plugin.homepage ?? "") !== "" || (details.plugin.license ?? "") !== ""
        // Shown as text, never as a link: KassenSturz doesn't open the
        // address for the plugin (D-847).
        text: details.plugin.homepage ?? ""
        textItem.textFormat: Text.PlainText
        description: (details.plugin.license ?? "") === ""
            ? ""
            : details.i18nContext.i18nc("@info %1 is a license name, as the plugin says", "License: %1", details.plugin.license)
        descriptionItem.textFormat: Text.PlainText
    }
}
