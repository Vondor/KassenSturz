// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton

import QtQuick

// Kirigami's InlineMessage, PlaceholderMessage and PromptDialog have no
// `textFormat`: they show a text as rich text when Qt thinks it looks like
// HTML, and rich text can load pictures from the web. A sentence of the app
// with a payee, a file's text or a name in it goes through here first, so
// that nothing in it can look like markup (D-143, threat T-08). Qt takes a
// text for markup if it has a `<` (before the first line break) or a written
// `&lt;`.
QtObject {
    function plain(text: var): string {
        return String(text).replace(/</g, "‹").replace(/&(?=lt;)/gi, "&​");
    }
}
