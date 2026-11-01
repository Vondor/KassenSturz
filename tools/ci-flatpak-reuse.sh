#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# CI helper (D-427). Run on a push to main, in a checkout of the pushed commit
# with its whole history:
#
#   GH_TOKEN=... REPO=owner/name SHA=<pushed commit> tools/ci-flatpak-reuse.sh [folder]
#
# The pull request that was merged already built the Flatpak and tested it.
# If that build was made from exactly the files now on main and carries the
# number main's build gets, the bundle is downloaded to `folder` (default
# `reused`) instead of being built a second time. Writes `reused=true` or
# `reused=false` to $GITHUB_OUTPUT and always succeeds: when anything is
# unclear, the answer is `false` and the Flatpak job builds as before.

set -eu

out=${GITHUB_OUTPUT:-/dev/null}
dir=${1:-reused}
bundle_artifact=KassenSturz-x86_64.flatpak
info_artifact=flatpak-build-info

no() {
    echo "reused=false" >> "$out"
    echo "Flatpak is built again: $*"
    exit 0
}

tree=$(git rev-parse 'HEAD^{tree}') || no "no tree"
number=$(sh tools/build-version.sh) || number=
[ -n "$number" ] || no "no build number"

head=$(gh api "repos/$REPO/commits/$SHA/pulls" --jq '.[0].head.sha // empty') || head=
[ -n "$head" ] || no "no pull request is known for this commit"

run=$(gh api "repos/$REPO/actions/workflows/ci.yml/runs?head_sha=$head&event=pull_request&per_page=20" \
    --jq '[.workflow_runs[] | select(.conclusion == "success")] | sort_by(.created_at) | last | .id // empty') || run=
[ -n "$run" ] || no "the pull request has no successful run for $head"

artifact_id() {
    gh api "repos/$REPO/actions/runs/$run/artifacts?per_page=100" \
        --jq ".artifacts[] | select(.name == \"$1\" and (.expired | not)) | .id" | head -n 1
}

info_id=$(artifact_id "$info_artifact") || info_id=
bundle_id=$(artifact_id "$bundle_artifact") || bundle_id=
[ -n "$info_id" ] && [ -n "$bundle_id" ] || no "run $run has no bundle or no build info"

work=$(mktemp -d)
gh api "repos/$REPO/actions/artifacts/$info_id/zip" > "$work/info.zip" || no "build info not downloadable"
unzip -q -o "$work/info.zip" -d "$work/info" || no "build info not readable"
[ "$(cat "$work/info/tree")" = "$tree" ] ||
    no "the pull request was built from other files (main moved on after its last run)"
[ "$(cat "$work/info/version")" = "$number" ] ||
    no "the pull request's build is numbered $(cat "$work/info/version"), main's is $number"

gh api "repos/$REPO/actions/artifacts/$bundle_id/zip" > "$work/bundle.zip" || no "bundle not downloadable"
mkdir -p "$dir"
unzip -q -o "$work/bundle.zip" -d "$dir" || no "bundle not readable"
[ -s "$dir/KassenSturz.flatpak" ] || no "the bundle is empty"

echo "reused=true" >> "$out"
echo "Flatpak $number reused from run $run (pull request head $head, same files as main)."
