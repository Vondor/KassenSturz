#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Checks that the build without AI (D-039) holds no AI code:
#
#   tools/sdk.sh tools/check-no-ai.sh
#
# It reads Cargo's dependency tree of the whole workspace with the `ai`
# feature off and fails when the tree names a crate that exists only for AI,
# when `kassensturz-mcp` has any dependency, or when a crate still has the
# `ai` feature switched on. As a control it checks that the same questions
# find the feature in the build with AI, so a changed command that finds
# nothing can't pass by accident. Add every AI-only crate to `AI_CRATES`
# in the pull request that introduces it.

set -eu

cd "$(dirname "$0")/.."

# Crates that exist only for AI features (docs/THIRD_PARTY.md, M5).
AI_CRATES='kassensturz-ai kassensturz-ai-tools rmcp ureq rustls tokio'

without='--workspace --no-default-features --features bundled-sqlite --locked'
with='--workspace --locked'

# `{p}` names the crate; `{f}` lists the features switched on in the build.
format='{p} [{f}]'
# shellcheck disable=SC2086
tree=$(cargo tree $without -e normal,build,dev --prefix none --format "$format")
# shellcheck disable=SC2086
tree_with=$(cargo tree $with -e normal --prefix none --format "$format")

status=0
for crate in $AI_CRATES; do
    if printf '%s\n' "$tree" | grep -q "^$crate "; then
        echo "error: the build without AI contains $crate" >&2
        status=1
    fi
done

# A crate with the `ai` feature on, whatever else is on with it.
has_ai='\[\(.*,\)\{0,1\}ai\(,.*\)\{0,1\}\]'
if printf '%s\n' "$tree" | grep -q "$has_ai"; then
    echo "error: the build without AI still switches the ai feature on" >&2
    status=1
fi
if ! printf '%s\n' "$tree_with" | grep -q "$has_ai"; then
    echo "error: control failed: the build with AI does not switch the ai feature on" >&2
    status=1
fi

mcp=$(cargo tree -p kassensturz-mcp --no-default-features --locked -e normal,build --prefix none)
if [ "$(printf '%s\n' "$mcp" | grep -c .)" -ne 1 ]; then
    echo "error: kassensturz-mcp without AI has dependencies:" >&2
    printf '%s\n' "$mcp" >&2
    status=1
fi

[ "$status" -eq 0 ] && echo "ok: the build without AI holds no AI crate"
exit "$status"
