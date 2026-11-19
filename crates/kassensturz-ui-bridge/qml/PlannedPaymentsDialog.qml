// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The planned payments in a window (US7.1, D-769), opened by the accounts
// page's button and the Overview's; the same list is the Plans page of the
// menu (B-388, D-863). What it shows is PlannedPaymentsView.qml.
//
//     plannedPayments.show();

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.Dialog {
    id: dialog

    objectName: "plannedPaymentsDialog"

    required property PlannedPayments plans
    required property AccountRegister register

    // For the smoke test: the form for a new or changed plan, and what the
    // list shows.
    readonly property alias form: content.form
    readonly property alias planList: content.planList
    readonly property alias toConfirm: content.toConfirm

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // Opens and reads the plans; it opens even if reading them fails, so
    // the button never does nothing (as MonthlyMovesDialog, M6-B242).
    function show() {
        open();
        content.refresh();
        Qt.callLater(() => {
            if (!dialog.visible) {
                dialog.open();
            }
        });
    }

    title: i18nContext.i18nc("@title:window", "Planned payments")
    standardButtons: Kirigami.Dialog.Close
    preferredWidth: Kirigami.Units.gridUnit * 32

    PlannedPaymentsView {
        id: content

        plans: dialog.plans
        register: dialog.register
    }
}
