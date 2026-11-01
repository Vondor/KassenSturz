// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The Reports section of the done page after a book was made from a
// KMyMoney file (M6d.6a, D-445): how many reports became saved views, then
// every report under Kept as they were, Kept with changes or Not kept, each
// change or reason in plain words. A period that became fixed days shows
// KMyMoney's period beside the days and the day of the import, so it can be
// compared with KMyMoney at a glance (Q-319 = A). Rust gives keywords and
// formatted dates (D-034, D-065); the words are written here. Names come
// from the file and are shown as plain text.

import QtQuick
import QtQuick.Layouts
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n

ColumnLayout {
    id: summary

    // The `reports` list of the done page's report.
    required property var reports
    readonly property int keptCount: reports.filter(line => line.outcome !== "not-kept").length

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    Layout.fillWidth: true
    spacing: 0
    visible: reports.length > 0

    // KMyMoney's period by the name its report dialog gives it.
    function periodName(word: string): string {
        switch (word) {
        case "today":
            return i18nContext.i18nc("@item a period in KMyMoney's report settings", "Today");
        case "last7days":
            return i18nContext.i18nc("@item a period in KMyMoney's report settings", "Last 7 days");
        case "last30days":
            return i18nContext.i18nc("@item a period in KMyMoney's report settings", "Last 30 days");
        case "last3months":
            return i18nContext.i18nc("@item a period in KMyMoney's report settings", "Last 3 months");
        case "last6months":
            return i18nContext.i18nc("@item a period in KMyMoney's report settings", "Last 6 months");
        case "last11months":
            return i18nContext.i18nc("@item a period in KMyMoney's report settings", "Last 11 months");
        case "last3tonext3months":
            return i18nContext.i18nc("@item a period in KMyMoney's report settings", "Last 3 months to next 3 months");
        case "monthtodate":
            return i18nContext.i18nc("@item a period in KMyMoney's report settings", "Month to date");
        case "untiltoday":
            return i18nContext.i18nc("@item a period in KMyMoney's report settings", "Until today");
        case "alldates":
            return i18nContext.i18nc("@item a period in KMyMoney's report settings", "All dates");
        case "currentquarter":
            return i18nContext.i18nc("@item a period in KMyMoney's report settings", "Current quarter");
        case "lastquarter":
            return i18nContext.i18nc("@item a period in KMyMoney's report settings", "Last quarter");
        default:
            return word;
        }
    }

    // The kind of a filter that was left out.
    function filterName(kind: string): string {
        switch (kind) {
        case "payee":
            return i18nContext.i18nc("@item a kind of filter in a KMyMoney report", "who was paid");
        case "tag":
            return i18nContext.i18nc("@item a kind of filter in a KMyMoney report", "tag");
        case "text":
            return i18nContext.i18nc("@item a kind of filter in a KMyMoney report", "text");
        case "amount":
            return i18nContext.i18nc("@item a kind of filter in a KMyMoney report", "amount");
        case "state":
            return i18nContext.i18nc("@item a kind of filter in a KMyMoney report", "status");
        case "type":
            return i18nContext.i18nc("@item a kind of filter in a KMyMoney report", "kind of transaction");
        default:
            return i18nContext.i18nc("@item a kind of filter in a KMyMoney report", "number");
        }
    }

    // One reason in plain words: a change of a kept report, or why one was
    // not kept.
    function reasonText(reason: string, line: var): string {
        switch (reason) {
        case "range-fixed":
            return line.first === ""
                ? i18nContext.i18nc("@info %1 is a period, %2 and %3 are days", "\"%1\" became the days up to %2 (as on %3, the day of the import)", periodName(line.period), line.last, line.imported)
                : i18nContext.i18nc("@info %1 is a period, %2, %3 and %4 are days", "\"%1\" became %2 to %3 (as on %4, the day of the import)", periodName(line.period), line.first, line.last, line.imported);
        case "by-month":
            return i18nContext.i18nc("@info a change to a report", "shown by month");
        case "totals-only":
            return i18nContext.i18nc("@info a change to a report", "shows the totals of money in and out");
        case "top-only":
            return i18nContext.i18nc("@info a change to a report", "shows only the top categories");
        case "chart-changed":
            return i18nContext.i18nc("@info a change to a report", "shown as another kind of chart");
        case "income-left-out":
            return i18nContext.i18nc("@info a change to a report", "shows only money out: the money in was left out");
        case "filter-left-out":
            return i18nContext.i18nc("@info %1 is a list of kinds of filters", "the filter by %1 was left out", line.filters.map(kind => filterName(kind)).join(", "));
        case "account-left-out":
            return i18nContext.i18nc("@info a change to a report", "an account KassenSturz doesn't hold was left out of its filter");
        case "currency":
            return i18nContext.i18nc("@info a change to a report", "shown in the book's currency");
        case "options-left-out":
            return i18nContext.i18nc("@info a change to a report", "some of its settings were left out");
        case "range-future":
            return i18nContext.i18nc("@info why a report was not kept", "it shows only days to come");
        case "income-only":
            return i18nContext.i18nc("@info why a report was not kept", "it shows only money in, which a saved view can't show by category yet");
        case "transactions-list":
            return i18nContext.i18nc("@info why a report was not kept", "a list of transactions");
        case "investments":
            return i18nContext.i18nc("@info why a report was not kept", "an investment report");
        case "tax":
            return i18nContext.i18nc("@info why a report was not kept", "a tax report");
        case "budget":
            return i18nContext.i18nc("@info why a report was not kept", "a budget report");
        case "forecast":
            return i18nContext.i18nc("@info why a report was not kept", "a forecast");
        case "information":
            return i18nContext.i18nc("@info why a report was not kept", "an account or loan information report");
        default:
            return i18nContext.i18nc("@info why a report was not kept", "its settings can't be shown the same way");
        }
    }

    function linesOf(outcome: string): var {
        return reports.filter(line => line.outcome === outcome);
    }

    FormCard.FormHeader {
        title: summary.i18nContext.i18nc("@title:group", "Reports")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            textItem.textFormat: Text.PlainText
            text: summary.i18nContext.i18ncp("@info %1 is how many reports the file has, %2 how many became saved views", "%2 of your %1 report is now a saved view in the Statistic Builder.", "%2 of your %1 reports are now saved views in the Statistic Builder.", summary.reports.length, summary.keptCount)
        }
    }

    Repeater {
        model: [
            { outcome: "kept", title: summary.i18nContext.i18nc("@title:group reports", "Kept as they were") },
            { outcome: "changed", title: summary.i18nContext.i18nc("@title:group reports", "Kept with changes") },
            { outcome: "not-kept", title: summary.i18nContext.i18nc("@title:group reports", "Not kept") }
        ]

        delegate: ColumnLayout {
            id: group

            required property var modelData
            readonly property var lines: summary.linesOf(modelData.outcome)

            Layout.fillWidth: true
            spacing: 0
            visible: lines.length > 0

            FormCard.FormHeader {
                title: group.modelData.title
            }

            FormCard.FormCard {
                Repeater {
                    model: group.lines

                    delegate: FormCard.FormTextDelegate {
                        required property var modelData

                        textItem.textFormat: Text.PlainText
                        descriptionItem.textFormat: Text.PlainText
                        text: modelData.name
                        description: modelData.reasons.map(reason => summary.reasonText(reason, modelData)).join("; ")
                    }
                }
            }
        }
    }
}
