#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# A quick QML syntax check that needs no build and no Kirigami install, so a
# lost line or a typo is found in seconds instead of in CI or in the running
# app (M6b retro, P2):
#
#   tools/check-qml-syntax.sh [file.qml ...]
#
# Without arguments it checks every .qml file under crates/. It does two
# things:
#
#   1. Runs qmllint over each file and fails on syntax errors only. Warnings
#      such as unknown Kirigami types or unqualified ids are ignored, because
#      the QML modules are not installed here; CI and the app cover those.
#   2. Fails on JavaScript methods that are newer than the engine of the
#      oldest Qt we support (String.replaceAll, Array.at, findLast,
#      toSorted, ...), which parse fine but fail only when the line runs.
#      A line that is checked and known to be safe ends with the comment
#      "qml-syntax-ok".
#
# qmllint is looked for as $QMLLINT, qmllint6, qmllint and in the usual Qt
# directories. Without it, step 2 still runs and the script says so.

set -u

cd "$(dirname "$0")/.."

if [ "$#" -gt 0 ]; then
    files=$(printf '%s\n' "$@")
else
    files=$(find crates -name '*.qml' | sort)
fi
[ -n "$files" ] || { echo "check-qml-syntax: no QML files found" >&2; exit 1; }

qmllint=${QMLLINT:-}
if [ -z "$qmllint" ]; then
    for candidate in qmllint6 qmllint /usr/lib/qt6/bin/qmllint \
        /usr/lib/x86_64-linux-gnu/qt6/bin/qmllint /usr/lib64/qt6/bin/qmllint; do
        if command -v "$candidate" >/dev/null 2>&1; then
            qmllint=$candidate
            break
        fi
    done
fi

failed=0

# 1. Syntax errors. qmllint prints a syntax error as "file:line:col: text" or
#    "Error: file:line:col: text"; everything else it prints is a warning or
#    an info line and is dropped.
if [ -n "$qmllint" ]; then
    for f in $files; do
        out=$("$qmllint" "$f" 2>&1 || true)
        errors=$(printf '%s\n' "$out" \
            | grep -E '^(Error: )?[^ ]+\.qml:[0-9]+(:[0-9]+)?: ' || true)
        if [ -n "$errors" ]; then
            printf '%s\n' "$errors" >&2
            failed=1
        fi
    done
else
    echo "check-qml-syntax: qmllint not found (set QMLLINT); only the method check runs" >&2
fi

# 2. Too-new JavaScript methods.
pattern='\.(replaceAll|at|findLast|findLastIndex|toSorted|toReversed|toSpliced|with)\(|structuredClone\(|Object\.groupBy\(|Array\.fromAsync\('
# shellcheck disable=SC2086
hits=$(grep -nE "$pattern" $files 2>/dev/null | grep -v 'qml-syntax-ok' || true)
if [ -n "$hits" ]; then
    printf '%s\n' "$hits" >&2
    echo "check-qml-syntax: the lines above use JavaScript methods that older Qt engines lack." >&2
    echo "Use an older equivalent, or end the line with a comment 'qml-syntax-ok' if it is safe." >&2
    failed=1
fi

if [ "$failed" -eq 0 ]; then
    echo "check-qml-syntax: ok ($(printf '%s\n' "$files" | wc -l) files)"
fi
exit "$failed"
