#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Updates the translation files in po/ after strings in the UI changed: it
# extracts the strings with Messages.sh and merges them into every
# po/<language>/kassensturz.po. Run it in the SDK:
#
#   tools/sdk.sh tools/update-translations.sh
#
# The keywords are KI18n's, as KDE's own string extraction passes them.

set -eu

cd "$(dirname "$0")/.."

podir=$(mktemp -d)
trap 'rm -rf "$podir"' EXIT

XGETTEXT="xgettext --from-code=UTF-8 -C --kde \
    --package-name=kassensturz --msgid-bugs-address=https://github.com/Vondor/KassenSturz/issues \
    -ci18n -ki18n:1 -ki18nc:1c,2 -ki18np:1,2 -ki18ncp:1c,2,3 \
    -kxi18n:1 -kxi18nc:1c,2 -kxi18np:1,2 -kxi18ncp:1c,2,3"
export XGETTEXT podir
sh Messages.sh

if [ "${1:-}" = "--print-template" ]; then
    cat "$podir/kassensturz.pot"
    exit
fi

for po in po/*/kassensturz.po; do
    [ -e "$po" ] || continue
    msgmerge --quiet --update --backup=none --no-fuzzy-matching "$po" "$podir/kassensturz.pot"
    # Translations of strings that are gone stay in git's history.
    msgattrib --no-obsolete --output-file="$po" "$po"
    msgfmt --check --output-file=/dev/null "$po"
done
