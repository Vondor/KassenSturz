#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Resolves the recurring conflict in po/<language>/kassensturz.po when two
# pull requests both add translations (M6b retro, P3). It takes main's file,
# re-applies this branch's new and changed translations by message (msgid),
# then regenerates references and drops gone strings with
# tools/update-translations.sh, and checks the result with msgfmt:
#
#   tools/merge-po.sh [main-ref]          # default: origin/main
#
# Run it on the pull request branch, either in the middle of a conflicted
# `git merge origin/main` (it reads the three sides from git's index and
# stages the result) or before merging (it reads HEAD, the merge base and
# main-ref and rewrites the files in your working tree). Needs gettext
# (msgcat, msgmerge, msgfmt, xgettext) and python3, e.g. in the Qt container.
# A translation both sides changed differently is kept as main has it and
# reported, so check those lines.

set -eu

cd "$(dirname "$0")/.."
mainref=${1:-origin/main}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

conflicted=0
[ -n "$(git ls-files -u -- po)" ] && conflicted=1

if [ "$conflicted" -eq 0 ]; then
    base_ref=$(git merge-base HEAD "$mainref")
fi

for po in po/*/kassensturz.po; do
    [ -e "$po" ] || continue
    lang=$(basename "$(dirname "$po")")
    if [ "$conflicted" -eq 1 ]; then
        git show ":1:$po" >"$work/base.po" 2>/dev/null || : >"$work/base.po"
        git show ":2:$po" >"$work/ours.raw"
        git show ":3:$po" >"$work/theirs.raw"
    else
        git show "$base_ref:$po" >"$work/base.raw" 2>/dev/null || : >"$work/base.raw"
        git show "HEAD:$po" >"$work/ours.raw"
        git show "$mainref:$po" >"$work/theirs.raw"
        cp "$work/base.raw" "$work/base.po"
    fi
    [ -s "$work/base.po" ] || cp "$work/theirs.raw" "$work/base.po"
    msgcat --no-wrap -o "$work/base.nw" "$work/base.po"
    msgcat --no-wrap -o "$work/ours.nw" "$work/ours.raw"
    msgcat --no-wrap -o "$work/theirs.nw" "$work/theirs.raw"
    echo "merge-po: $lang"
    python3 -I tools/merge-po.py "$work/base.nw" "$work/ours.nw" "$work/theirs.nw" "$po"
done

sh tools/update-translations.sh
if [ "$conflicted" -eq 1 ]; then
    git add -- po
fi
echo "merge-po: done. Check with tools/check-translations.sh."
