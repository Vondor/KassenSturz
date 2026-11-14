#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Checks that the app ends cleanly on SIGTERM and SIGHUP (B-276): a logout,
# a shutdown or `pkill` must end the event loop like quitting does, so the
# book is closed and its lock file removed. Without the handler the signal
# kills the app on the spot and the lock file stays behind.
#
#   tools/check-sigterm.sh build/stage/usr/bin/kassensturz
#
# The app runs in the test profile (KASSENSTURZ_TEST=1: every folder in a
# test folder) under Qt's `offscreen` platform. It fails if the app ends by
# itself before the signal (and says what the app printed), if it doesn't end
# within 20 seconds of the signal, or if it ends with anything but status 0,
# which includes being killed by the signal. It never passes without having
# sent the signal to a running app.

set -eu

app=${1:?usage: tools/check-sigterm.sh PATH-TO-THE-APP}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

for signal in TERM HUP; do
    home="$work/home-$signal"
    mkdir -p "$home"
    KASSENSTURZ_TEST=1 QT_QPA_PLATFORM=offscreen HOME="$home" \
        XDG_DATA_HOME="$home/data" XDG_CONFIG_HOME="$home/config" \
        XDG_CACHE_HOME="$home/cache" XDG_STATE_HOME="$home/state" \
        "$app" > /dev/null 2> "$work/err-$signal" &
    pid=$!
    # Time to load the screens and install the handler.
    sleep 6
    if ! kill -0 "$pid" 2> /dev/null; then
        status=0
        wait "$pid" || status=$?
        echo "The app ended by itself before SIG$signal (status $status):" >&2
        cat "$work/err-$signal" >&2
        exit 1
    fi
    kill -s "$signal" "$pid"
    waited=0
    while kill -0 "$pid" 2> /dev/null; do
        if [ "$waited" -ge 20 ]; then
            kill -s KILL "$pid" 2> /dev/null || true
            echo "The app did not end within 20 seconds of SIG$signal." >&2
            exit 1
        fi
        sleep 1
        waited=$((waited + 1))
    done
    status=0
    wait "$pid" || status=$?
    if [ "$status" -ne 0 ]; then
        echo "SIG$signal ended the app with status $status, not through its own way out." >&2
        cat "$work/err-$signal" >&2
        exit 1
    fi
    echo "SIG$signal ends the app like quitting: ok"
done
