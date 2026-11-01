#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# A quick check, with no build, that every handler of a user's action in the
# QML says that it ran, so the developer mode's debug log shows whether a
# click reached its handler (M6c.D, D-419):
#
#   tools/check-action-log.sh
#
# A handler is a line that starts with onClicked, onTriggered, onAccepted,
# onToggled, onActivated or onChosen and a colon. Its first line must be
#
#     ActionLog.taken("<QML file>_<what it does>")
#
# with a name from `kassensturz_core::vocabulary::ACTIONS`
# (crates/kassensturz-core/src/vocabulary/actions.rs) that starts with the
# file's name and that no other handler uses. Every name in the list must be
# used. A handler inside a Timer is left out: it ticks by itself. `cargo test -p kassensturz-ui-bridge action_log_audit` checks the
# same in CI; this script is the fast local run.

set -eu

cd "$(dirname "$0")/.."

qml=crates/kassensturz-ui-bridge/qml
list=crates/kassensturz-core/src/vocabulary/actions.rs

[ -d "$qml" ] && [ -f "$list" ] || { echo "check-action-log: files not found" >&2; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# The names in ACTIONS, one per line.
sed -n 's/^    "\([A-Za-z0-9_]*\)",$/\1/p' "$list" | sort > "$work/listed"

# Each handler: file, line, and the name its first line gives (or nothing).
for file in "$qml"/*.qml; do
    awk -v file="$(basename "$file" .qml)" '
        # The last line seen at each indent, to tell the element a handler is
        # in: a Timer ticks by itself, it is not a user'"'"'s action.
        {
            line = $0
            sub(/^[ \t]*/, "", line)
            indent = length($0) - length(line)
        }
        /^[ \t]*(onClicked|onTriggered|onAccepted|onToggled|onActivated|onChosen)[ \t]*:/ {
            if (indent >= 4 && (indent - 4) in last && last[indent - 4] ~ /^Timer[ \t]*\{[ \t]*$/) {
                print file "\t" FNR "\ttimer"
                last[indent] = line
                next
            }
            last[indent] = line
            handler = FNR
            if (line !~ /\{[ \t]*$/) {
                print file "\t" FNR "\tnot-a-block"
                next
            }
            getline next_line
            if (match(next_line, /ActionLog\.taken\("[A-Za-z0-9_]*"\)/)) {
                name = substr(next_line, RSTART + 17, RLENGTH - 19)
                print file "\t" handler "\t" name
            } else {
                print file "\t" handler "\tnone"
            }
        }
        !/^[ \t]*$/ { last[indent] = line }
    ' "$file"
done > "$work/handlers"

failed=0
total=$(wc -l < "$work/handlers")
timers=$(grep -c "	timer$" "$work/handlers" || true)

while IFS="	" read -r file line name; do
    case "$name" in
        timer) ;;
        not-a-block) echo "$file.qml:$line: the handler must be a block that starts with ActionLog.taken(\"${file}_...\")" >&2; failed=1 ;;
        none) echo "$file.qml:$((line + 1)): the handler must start with ActionLog.taken(\"${file}_...\")" >&2; failed=1 ;;
        "${file}_"*)
            grep -qx "$name" "$work/listed" || { echo "$file.qml:$((line + 1)): add $name to actions.rs" >&2; failed=1; } ;;
        *) echo "$file.qml:$((line + 1)): $name must start with ${file}_" >&2; failed=1 ;;
    esac
done < "$work/handlers"

grep -v "	timer$" "$work/handlers" | cut -f3 | sort > "$work/used"
dups=$(uniq -d < "$work/used" || true)
[ -z "$dups" ] || { echo "names used by two handlers: $dups" >&2; failed=1; }
unused=$(comm -23 "$work/listed" "$work/used" || true)
[ -z "$unused" ] || { echo "in actions.rs but used by no handler: $unused" >&2; failed=1; }

# A handler written inside a line would escape the reading above.
inline=$(grep -hoE '(^|[^A-Za-z])(onClicked|onTriggered|onAccepted|onToggled|onActivated|onChosen)[ \t]*:' "$qml"/*.qml | wc -l)
[ "$inline" -eq "$total" ] || { echo "check-action-log: $inline handlers mentioned, $total at the start of a line; give each its own line" >&2; failed=1; }

[ "$((total - timers))" -gt 300 ] || { echo "check-action-log: only $total handlers found" >&2; failed=1; }

if [ "$failed" -eq 0 ]; then
    echo "check-action-log: ok ($((total - timers)) handlers, $timers timers left out)"
fi
exit "$failed"
