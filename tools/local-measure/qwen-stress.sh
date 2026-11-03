#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Qwen stress test for KassenSturz's built-in AI (hand-run, never in CI).
# Measures only: synthetic Space Colony book, loopback model server, nothing
# in the product changes. Plan: ai-tests/qwen-stress/PLAN.md in the shared
# project folder.
#
#   OUT=/path/to/results MODELS="qwen3.5-9b qwen3.6-35b-a3b" sh tools/local-measure/qwen-stress.sh [S1 S2 S3 S4]
#
# Env: MODELS (default: every installed model whose name contains "qwen"),
#      LMS (lms command, default lms), RUN (prefix to run binaries, e.g. an
#      SDK wrapper; default none), OUT (results folder), PHASES (default all).
set -u
cd "$(dirname "$0")/../.."
LMS=${LMS:-lms}
RUN=${RUN:-}
OUT=${OUT:-$HOME/qwen-stress-results}
QDIR=tools/local-measure/questions
BIN=target/release/examples/questionnaire_local
PROXY=tools/local-measure/qwen-proxy.py
PORT=1235
mkdir -p "$OUT"
PHASES=${*:-S1 S2 S3 S4}
[ -n "${MODELS:-}" ] || MODELS=$($LMS ls 2>/dev/null | grep -io '[a-z0-9._/-]*qwen[a-z0-9._/-]*' | sort -u | tr '\n' ' ')
echo "models: $MODELS" | tee -a "$OUT/stress.log"

$RUN cargo build --quiet --release -p kassensturz-ai-chat --features ai \
    --example questionnaire_local --example describe_charts_local || exit 1

log() { echo "$(date -u +%FT%TZ) $*" | tee -a "$OUT/stress.log"; }

load() { # model context
    $LMS unload --all >/dev/null 2>&1
    if $LMS load "$1" --context-length "$2" --gpu max -y >/dev/null 2>&1; then
        nvidia-smi --query-gpu=memory.used --format=csv,noheader > "$OUT/vram-$(echo "$1" | tr / _)-$2.txt" 2>/dev/null
        return 0
    fi
    log "LOAD FAILED $1 ctx $2"; return 1
}

proxy() { # think fault delay tag
    stop_proxy
    python3 "$PROXY" --listen $PORT --think "$1" --fault "$2" --delay "$3" --log "$OUT/proxy-$4.jsonl" &
    echo $! > "$OUT/.proxy.pid"; sleep 1
}
stop_proxy() { [ -f "$OUT/.proxy.pid" ] && kill "$(cat "$OUT/.proxy.pid")" 2>/dev/null; rm -f "$OUT/.proxy.pid"; }
trap 'stop_proxy; $LMS unload --all >/dev/null 2>&1' EXIT INT TERM

ask() { # model set tag [extra args]
    m=$1; set_=$2; tag=$3; shift 3
    log "run $tag"
    $RUN "$BIN" --address http://127.0.0.1:$PORT/v1 --model "$m" --questions "$QDIR/$set_.json" \
        --out "$OUT/$tag.jsonl" "$@" > "$OUT/$tag.md" 2> "$OUT/$tag.err"
}

for m in $MODELS; do
    mt=$(echo "$m" | tr / _)
    case " $PHASES " in *" S1 "*)
        # S1 compatibility grid: context x thinking, one core run each.
        for ctx in 8192 16384 32768; do
            load "$m" $ctx || continue
            for think in keep off; do
                proxy $think none 0 "$mt-S1-$ctx-$think"
                ask "$m" core13 "$mt-S1-$ctx-think$think"
            done
        done;;
    esac
    case " $PHASES " in *" S2 "*)
        # S2 depth at the largest context that loads: repeats, German, edge, long chat, charts, reading-only grid.
        load "$m" 32768 || load "$m" 16384 || continue
        proxy keep none 0 "$mt-S2"
        for r in 1 2 3; do ask "$m" core13 "$mt-S2-core-run$r"; done
        ask "$m" german13 "$mt-S2-german"
        ask "$m" edge16 "$mt-S2-edge"
        ask "$m" long40 "$mt-S2-long40"
        ask "$m" core13 "$mt-S2-gridread" --grid read
        log "run $mt-S2-charts"
        $RUN target/release/examples/describe_charts_local --address http://127.0.0.1:$PORT/v1 --model "$m" \
            > "$OUT/$mt-S2-charts.md" 2> "$OUT/$mt-S2-charts.err";;
    esac
    case " $PHASES " in *" S3 "*)
        # S3 reply shapes Qwen servers really produce, injected into real replies.
        load "$m" 32768 || load "$m" 16384 || continue
        for fault in raw_tool_text think_in_text reasoning_only args_object no_call_id stop_with_calls parallel_dup fenced_args length_cut; do
            proxy keep $fault 0 "$mt-S3-$fault"
            ask "$m" core13 "$mt-S3-$fault" --only 1,3,5,6,7
        done;;
    esac
    case " $PHASES " in *" S4 "*)
        # S4 slow server: one answer later than the 300 s limit must end as a clear timeout.
        load "$m" 16384 || continue
        proxy keep none 310 "$mt-S4-timeout"
        ask "$m" core13 "$mt-S4-timeout" --only 3;;
    esac
done
stop_proxy
$LMS unload --all >/dev/null 2>&1
log "done"
