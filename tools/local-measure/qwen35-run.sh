#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# Hand-run measurement of cheap local Qwen3.5 models on KassenSturz (synthetic
# Space Colony books only). Usage: run.sh MODEL [RUNS]   e.g. run.sh qwen3.5-9b 3
# Needs LM Studio's server on 127.0.0.1:1234 with MODEL loaded.
set -eu
MODEL=$1; RUNS=${2:-3}
WT=${WT:-$HOME/.cache/ks-qwen-test/wt}
OUT=${OUT:-/srv/data/Coding/Claude/ai-tests-qwen35}
mkdir -p "$OUT"
cd "$WT"
for r in $(seq 1 "$RUNS"); do
  cargo run --quiet --release -p kassensturz-ai-chat --features ai --example questionnaire_local -- \
    --address http://127.0.0.1:1234/v1 --model "$MODEL" --out "$OUT/$MODEL-q13-run$r.jsonl" \
    > "$OUT/$MODEL-q13-run$r.md" 2> "$OUT/$MODEL-q13-run$r.err" || true
done
tools/describe-charts-local.sh --address http://127.0.0.1:1234/v1 --model "$MODEL" \
  > "$OUT/$MODEL-charts.md" 2> "$OUT/$MODEL-charts.err" || true
