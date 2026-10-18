#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Runs a command in KassenSturz's one build environment: the KDE Flatpak SDK
# with the Rust SDK extension (D-063). Developers and CI use it the same way:
#
#   tools/sdk.sh cargo test --workspace
#   tools/sdk.sh cargo run --bin kassensturz
#
# Needs (from Flathub): org.kde.Sdk//6.11 and
# org.freedesktop.Sdk.Extension.rust-stable//25.08.
#
# Cargo's downloads go to .sdk-cache/ in the repository, or to
# $KASSENSTURZ_SDK_CACHE if set.

set -eu

kde_sdk="org.kde.Sdk//6.11"

repo=$(cd "$(dirname "$0")/.." && pwd)
cache=${KASSENSTURZ_SDK_CACHE:-"$repo/.sdk-cache"}
mkdir -p "$cache"

# The display sockets let `cargo run` open the app window; builds don't use
# them. Network access is only for Cargo's downloads: the app itself never gets
# it (AGENTS.md §3.1 rule 4).
exec flatpak run \
    --die-with-parent \
    --share=network \
    --share=ipc \
    --socket=wayland \
    --socket=fallback-x11 \
    --device=dri \
    --filesystem="$repo" \
    --filesystem="$cache" \
    --env=CARGO_HOME="$cache/cargo" \
    --command=sh \
    "$kde_sdk" \
    -c '. /usr/lib/sdk/rust-stable/enable.sh && exec "$@"' sh "$@"
