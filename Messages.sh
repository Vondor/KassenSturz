#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Extracts the translatable strings into $podir/kassensturz.pot. This follows
# KDE's Messages.sh convention: the caller sets $XGETTEXT and $podir (D-034,
# D-041). tools/update-translations.sh runs it locally.

$XGETTEXT $(find crates/kassensturz-ui-bridge/qml -name '*.qml' | sort) -o "$podir/kassensturz.pot"
