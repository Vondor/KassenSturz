#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Checks that the translations in po/ are valid, complete and up to date with
# the strings in the UI, without changing anything (D-034, D-126):
#
#   tools/sdk.sh tools/check-translations.sh
#
# It fails when a string has no translation yet, when a translation is left
# over from a string that no longer exists, when a translation leaves out or
# changes a placeholder (%1), or when an English string uses a word that
# AGENTS.md section 6 and docs/GLOSSARY.md keep out of the interface. The fix
# is tools/update-translations.sh and then the missing translations.

set -eu

cd "$(dirname "$0")/.."

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

podir=$work
XGETTEXT="xgettext --from-code=UTF-8 -C --kde \
    --package-name=kassensturz --msgid-bugs-address=https://github.com/Vondor/KassenSturz/issues \
    -ci18n -ki18n:1 -ki18nc:1c,2 -ki18np:1,2 -ki18ncp:1c,2,3 \
    -kxi18n:1 -kxi18nc:1c,2 -kxi18np:1,2 -kxi18ncp:1c,2,3"
export XGETTEXT podir
sh Messages.sh

failed=0

# Accounting words that don't belong in the default UI (AGENTS.md section 6).
# The comments and contexts (lines starting with # or msgctxt) are for
# translators and may use them.
if grep -v '^#' "$work/kassensturz.pot" | grep -v '^msgctxt' \
    | grep -iwE 'payees?|ledger|debits?|reconcile|reconciliation|double-entry|journal|equity'; then
    echo "check-translations: the strings above use accounting jargon; see docs/GLOSSARY.md" >&2
    failed=1
fi

for po in po/*/kassensturz.po; do
    [ -e "$po" ] || continue
    copy="$work/$(basename "$(dirname "$po")").po"
    cp "$po" "$copy"
    msgmerge --quiet --update --backup=none --no-fuzzy-matching "$copy" "$work/kassensturz.pot"
    stats=$(msgfmt --check --statistics --output-file=/dev/null "$copy" 2>&1) || {
        echo "check-translations: $po is not valid:" >&2
        echo "$stats" >&2
        failed=1
        continue
    }
    case "$stats" in
        *fuzzy* | *untranslated*)
            echo "check-translations: $po is incomplete ($stats)." >&2
            echo "Run tools/update-translations.sh and translate what is missing." >&2
            failed=1
            ;;
    esac
    if grep -q '^#~' "$copy"; then
        echo "check-translations: $po has translations of strings that are gone." >&2
        echo "Run tools/update-translations.sh to remove them." >&2
        failed=1
    fi
done

exit "$failed"
