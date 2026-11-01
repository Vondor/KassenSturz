// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// *New dashboard…* on the Overview (M6d.1, D-436): asks for a name and makes
// an empty dashboard at the end of the list, which then opens being
// arranged. The same dialog renames the dashboard shown (`renaming`). A
// name another dashboard has is refused by Rust and said here, and the
// dialog stays open. M6d.2 adds the templates to pick from (a
// `TemplatePicture` per template) when it is not renaming.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.Dialog {
    id: dialog

    // A `DashboardLayout`.
    required property var layout
    // Whether the dialog renames the dashboard shown instead.
    property bool renaming: false

    // A new dashboard was made; it is shown, being arranged.
    signal made()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // Opens the dialog to make a new dashboard, with a name no dashboard
    // has yet.
    function openNew() {
        renaming = false;
        const taken = (layout.view.dashboards ?? []).map(dashboard => dashboard.name.toLowerCase());
        let name = i18nContext.i18nc("@item the name a new dashboard starts with", "New dashboard");
        for (let number = 2; taken.includes(name.toLowerCase()); number++) {
            name = i18nContext.i18nc("@item the name a new dashboard starts with, %1 a number", "New dashboard %1", number);
        }
        start(name);
    }

    // Opens the dialog to rename the dashboard shown.
    function openRename() {
        renaming = true;
        start(layout.view.name ?? "");
    }

    function start(name: string) {
        nameField.text = name;
        error.result = ({});
        open();
        nameField.selectAll();
        nameField.forceActiveFocus();
    }

    function finish() {
        const result = renaming ? layout.renameDashboard(nameField.text) : layout.newDashboard(nameField.text);
        if (result.code !== undefined) {
            error.result = result;
            return;
        }
        close();
        if (!renaming) {
            made();
        }
    }

    title: renaming
        ? i18nContext.i18nc("@title:window", "Rename this dashboard")
        : i18nContext.i18nc("@title:window", "New dashboard")
    standardButtons: Kirigami.Dialog.NoButton
    preferredWidth: Kirigami.Units.gridUnit * 22
    padding: Kirigami.Units.largeSpacing

    customFooterActions: [
        Kirigami.Action {
            icon.name: dialog.renaming ? "edit-rename" : "list-add"
            enabled: nameField.text.trim() !== ""
            text: dialog.renaming
                ? dialog.i18nContext.i18nc("@action:button", "Rename")
                : dialog.i18nContext.i18nc("@action:button make the new dashboard", "Make it")
            onTriggered: {
                ActionLog.taken("NewDashboardDialog_keep")
                dialog.finish()
            }
        },
        Kirigami.Action {
            icon.name: "dialog-cancel"
            text: dialog.i18nContext.i18nc("@action:button", "Cancel")
            onTriggered: {
                ActionLog.taken("NewDashboardDialog_cancel")
                dialog.close()
            }
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Controls.Label {
            Layout.fillWidth: true
            visible: !dialog.renaming
            wrapMode: Text.Wrap
            text: dialog.i18nContext.i18nc("@info", "A new dashboard starts empty. You add charts, tables and figures to it, and switch between your dashboards with the tabs at the top of the Overview.")
        }

        Controls.TextField {
            id: nameField

            Layout.fillWidth: true
            maximumLength: 200
            Accessible.name: dialog.i18nContext.i18nc("@label:textbox", "Name of the dashboard")
            onAccepted: {
                ActionLog.taken("NewDashboardDialog_nameAccepted")
                dialog.finish()
            }
        }

        FormError {
            id: error

            Layout.fillWidth: true
        }
    }
}
