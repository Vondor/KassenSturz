#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Builds and installs the Flatpak for the current user from the checked-out
# commit, offline as on Flathub (M6-B165):
#
#   tools/build-flatpak.sh [more flatpak-builder options]
#
# The Flatpak's copy of the sources leaves out .git, so inside the build
# tools/build-version.sh can't count anything and the app, `flatpak info` and
# the AppStream file would show the hand-set version in Cargo.toml. As CI's
# Flatpak job does, this writes the build's number and commit into the files
# build-version and build-commit first (both ignored by git), which build.rs
# and CMakeLists.txt read (D-226).

set -eu

cd "$(dirname "$0")/.."

version=$(sh tools/build-version.sh)
if [ -z "$version" ]; then
    echo "build-flatpak: tools/build-version.sh printed no number; the checkout needs its git history" >&2
    exit 1
fi
printf '%s\n' "$version" > build-version
git rev-parse --short=7 HEAD > build-commit
echo "build-flatpak: building $version ($(cat build-commit))"

flatpak run org.flatpak.Builder --user --install --force-clean --disable-rofiles-fuse \
    --repo=build/flatpak-repo "$@" build/flatpak packaging/flatpak/io.github.vondor.KassenSturz.yml
