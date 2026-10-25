#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Prints the version of the build made from the checked-out commit (D-226):
#
#   tools/build-version.sh
#
# The form is 0.YYMMDD.N of D-152: the date is the commit's date in UTC, N
# counts the commits of that date on the branch's first-parent line, starting
# at 0. On main every merged pull request is one such commit, so each merge
# raises the number and no pull request edits a version file. Needs the full
# history (CI checks out with fetch-depth 0). Prints nothing, and succeeds,
# when git can't tell; the build then keeps the version of `Cargo.toml`.

set -eu

cd "$(dirname "$0")/.."

date=$(TZ=UTC git log -1 --date=format-local:%y%m%d --format=%cd 2>/dev/null) || exit 0
[ -n "$date" ] || exit 0
count=$(TZ=UTC git log --first-parent --date=format-local:%y%m%d --format=%cd 2>/dev/null |
    grep -c "^$date\$") || exit 0
[ "$count" -ge 1 ] || exit 0
major=$(sed -n 's/^version = "\([0-9]*\)\..*/\1/p' Cargo.toml | head -n 1)
echo "${major:-0}.$date.$((count - 1))"
