#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# Synthetic end-to-end CLI smoke test; no real financial data.
set -euo pipefail
cd "$(dirname "$0")/.."
tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT
export KASSENSTURZ_BOOK="$tmpdir/SyntheticHousehold.ksz"
runner=(cargo run --locked --quiet --bin kassensturz-cli --)
"${runner[@]}" create
"${runner[@]}" account add "Checking account" --type checking --currency EUR
"${runner[@]}" category add Groceries --kind expense --currency EUR
"${runner[@]}" transaction add --date 2026-10-05 --account "Checking account" --amount -53.20 --category Groceries
"${runner[@]}" register "Checking account" > "$tmpdir/register.txt"
"${runner[@]}" transfer suggest > "$tmpdir/transfers.txt"
"${runner[@]}" rule add Supermarket --if "payee contains market" --category Groceries
"${runner[@]}" rule run --dry-run > "$tmpdir/rules.txt"
test -s "$KASSENSTURZ_BOOK"
grep -Fq 'Checking account' "$tmpdir/register.txt"
printf 'PASS: synthetic CLI book, transaction, register, transfer suggestion and dry-run commands\n'
