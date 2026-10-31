#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Asks a small AI model on this computer for the ten described charts of the
# fake model (M6c.10b, D-356, D-412) and prints a Markdown table: whether each
# first call of propose_view was taken, how many refused calls came before
# one was, whether the chart was prepared and with which dataset, and the
# share prepared at the first try and after corrections. Copy the table into
# the AI questionnaire (ai-tests/local-model-results.md in the project's
# shared folder).
#
#   tools/describe-charts-local.sh --address http://127.0.0.1:1234/v1 --model qwen3.5-9b
#
# Run by hand, never in CI. It uses only the synthetic Space Colony book from
# the repository, asks only a model on this computer (a loopback address; any
# other is refused), and keeps no settings: the provider is set up in a
# temporary settings file that is removed at the end. The address is the one
# the app's AI settings would take (with /v1 for LM Studio, llama.cpp and
# Ollama's OpenAI-compatible server).

set -eu

cd "$(dirname "$0")/.."

if [ "$#" -eq 0 ]; then
    echo "usage: tools/describe-charts-local.sh --address http://127.0.0.1:1234/v1 --model NAME" >&2
    exit 2
fi

exec cargo run --quiet --release -p kassensturz-ai-chat --example describe_charts_local -- "$@"
