#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# CI helper (D-957). Run just before a job saves Cargo's cache:
#
#   tools/ci-prune-cargo-cache.sh <target folder>...
#
# It removes what this workspace's own crates built and keeps what the
# crates from crates.io built. A fresh checkout makes Cargo build every
# crate of the workspace again whatever the cache holds, so their files in
# the cache are never used, and they are most of it: measured on 21 November
# 2026, the test programs alone were 15 GB of a 16 GB folder, the other
# crates 1.1 GB. With them, three of these caches were 11.6 GB packed, more
# than the repository's 10 GB limit, so they kept pushing each other out.
#
# Removed, in every profile folder (the folder that holds `deps`):
#   - the programs and libraries copied beside `deps`
#   - in `deps`: programs (no extension, or .exe with its .pdb) and every
#     file of a crate whose name starts with kassensturz
#   - the build notes (`.fingerprint`) and build script output (`build`)
#     of those crates, so Cargo doesn't expect files that are gone
#   - `incremental` and `examples`
# A folder that doesn't exist is skipped. Prints what it kept.
set -eu

if [ "$#" -eq 0 ]; then
    echo "usage: $0 <target folder>..." >&2
    exit 2
fi

for target in "$@"; do
    if [ ! -d "$target" ]; then
        echo "$target: not there, skipped"
        continue
    fi
    before=$(du -sm "$target" | cut -f 1)
    find "$target" -maxdepth 4 -type d -name deps | while IFS= read -r deps; do
        profile=$(dirname "$deps")
        find "$profile" -maxdepth 1 -type f -delete
        rm -rf "$profile/incremental" "$profile/examples"
        find "$deps" -maxdepth 1 -type f \
            \( ! -name '*.*' -o -name '*.exe' -o -name '*.pdb' -o -name '*kassensturz*' \) -delete
        for folder in "$profile/.fingerprint" "$profile/build"; do
            if [ -d "$folder" ]; then
                find "$folder" -maxdepth 1 -type d -name 'kassensturz*' -exec rm -rf {} +
            fi
        done
    done
    after=$(du -sm "$target" | cut -f 1)
    echo "$target: $before MB before, $after MB kept"
done
