#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Tells whether a change needs a recorded review by an independent review
# agent (D-786, docs/INDEPENDENT_REVIEW.md):
#
#   tools/needs-independent-review.sh [BASE]     # BASE defaults to origin/main
#
# It lists the changed files (BASE...HEAD) that match a line of
# tools/review-areas.txt. Exit status 0 = review needed (files are printed),
# 1 = no file matches, 2 = it could not read the change.
set -eu

base="${1:-origin/main}"
here="$(cd "$(dirname "$0")" && pwd)"
files="$(git diff --name-only "$base"...HEAD)" || exit 2

found=0
while IFS= read -r f; do
    [ -n "$f" ] || continue
    while IFS= read -r pat; do
        case "$pat" in '' | '#'*) continue ;; esac
        # shellcheck disable=SC2254
        case "$f" in
        $pat)
            printf '%s\n' "$f"
            found=1
            break
            ;;
        esac
    done <"$here/review-areas.txt"
done <<EOF2
$files
EOF2

[ "$found" -eq 1 ]
