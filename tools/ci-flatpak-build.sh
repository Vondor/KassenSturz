#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# CI's Flatpak build, inside the pinned KDE image (#904, Q-558):
#
#   tools/ci-flatpak-build.sh <bundle file> [more flatpak-builder options]
#
# It replaces the flatpak-builder action, whose `--install-deps-from=flathub`
# also *updated* the runtime and SDK the pinned image already holds whenever
# Flathub had a newer build of them. That update failed now and then with
# "Invalid cross-device link" inside the container, so the Flatpak job failed
# on runs that changed nothing about the Flatpak. Here the image's runtime and
# SDK are used exactly as they are; only what is missing is installed, and
# nothing is ever updated. The image is pinned by digest, so the build stays
# the same until the pin is moved on purpose (D-025, D-063).
#
# Builds into `flatpak_app` (the folder release.yml searches for the
# registration number) and writes the bundle with Flathub as its runtime repo.
# Never add `set -x`: release.yml calls this while the number is in the tree.

set -eu

bundle=${1:?usage: ci-flatpak-build.sh <bundle file> [flatpak-builder options]}
shift

manifest=packaging/flatpak/io.github.vondor.KassenSturz.yml
app=io.github.vondor.KassenSturz
refs="org.kde.Platform//6.11 org.kde.Sdk//6.11 org.freedesktop.Sdk.Extension.rust-stable//25.08"
repo_url=https://dl.flathub.org/repo/flathub.flatpakrepo

flatpak remote-add --user --if-not-exists flathub "$repo_url"

# `flatpak info` looks in every installation. A plain `install` of something
# not there is the only network step; it never touches what is installed.
for ref in $refs; do
    if flatpak info "$ref" > /dev/null 2>&1; then
        echo "ci-flatpak-build: $ref is in the image"
    else
        echo "ci-flatpak-build: $ref is missing; installing it"
        flatpak install --user -y --noninteractive flathub "$ref"
    fi
done

# Builder looks for the SDK in the user installation first when told to.
user=
if flatpak info --user org.kde.Sdk//6.11 > /dev/null 2>&1; then
    user=--user
fi

# shellcheck disable=SC2086 # $user is one option or nothing
xvfb-run --auto-servernum flatpak-builder $user --disable-rofiles-fuse \
    --force-clean --repo=repo --state-dir=.flatpak-builder "$@" \
    flatpak_app "$manifest"

flatpak build-bundle --runtime-repo="$repo_url" repo "$bundle" "$app"
ls -l "$bundle"
