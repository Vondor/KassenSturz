#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Starts a KassenSturz program of the snap with its data in the folder that
# stays the same from one revision to the next (B-266).
#
# snapd gives every revision its own home, ~/snap/kassensturz/<revision>,
# and copies it on an update. A book, settings or backups kept there would
# be left behind by a rollback or a revision that fails to copy. Everything
# goes to $SNAP_USER_COMMON instead, which all revisions share.
set -eu

common="$SNAP_USER_COMMON"
old="$SNAP_USER_DATA"

for kind in .local/share .config .cache; do
    if [ ! -e "$common/$kind" ]; then
        mkdir -p "$common/$kind"
        # Data a revision before this one kept in its own folder is copied,
        # never moved: the old folder stays as it was.
        if [ "$kind" != .cache ] && [ -d "$old/$kind" ]; then
            cp -a "$old/$kind/." "$common/$kind/" 2>/dev/null || true
        fi
    fi
done

export XDG_DATA_HOME="$common/.local/share"
export XDG_CONFIG_HOME="$common/.config"
export XDG_CACHE_HOME="$common/.cache"
# Without this Qt may not offer the accessibility tree to screen readers
# inside the snap (the Flatpak does).
export QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1

exec "$@"
