#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Summarises a Qwen stress-test results folder into one Markdown table.
Figures and change sets are hints only; the final score is read by hand.

  qwen-summary.py RESULTS_FOLDER > summary.md
"""
import glob, json, os, sys
from collections import Counter

folder = sys.argv[1]
print("| Run | Qs | Errors | Figures hit | Change sets | Apply errors | Refused calls | Median s | Max s | Error codes |")
print("|---|---|---|---|---|---|---|---|---|---|")
for path in sorted(glob.glob(os.path.join(folder, "*.jsonl"))):
    name = os.path.basename(path)[:-6]
    if name.startswith("proxy-"):
        continue
    rows = [json.loads(line) for line in open(path) if line.strip()]
    if not rows:
        continue
    errors = Counter(r["failed"] for r in rows if r.get("failed"))
    with_expect = [r for r in rows if r.get("expected_figures_found") is not None]
    hits = sum(1 for r in with_expect if r["expected_figures_found"])
    secs = sorted((r.get("millis") or float(r.get("seconds") or 0) * 1000) / 1000 for r in rows)
    print(f"| {name} | {len(rows)} | {sum(errors.values())} | {hits}/{len(with_expect)} | "
          f"{sum(len(r['changes']) for r in rows)} | {sum(len(r.get('apply_errors', [])) for r in rows)} | "
          f"{sum(r['refused_calls'] for r in rows)} | {secs[len(secs)//2]:.0f} | {secs[-1]:.0f} | "
          f"{', '.join(f'{k} x{v}' for k, v in errors.items()) or 'none'} |")

print("\n| Proxy log | Requests | HTTP errors | Raw <tool_call> text | <think> in text | Prompt tokens max | Completion tok/s |")
print("|---|---|---|---|---|---|---|")
for path in sorted(glob.glob(os.path.join(folder, "proxy-*.jsonl"))):
    rows = [json.loads(line) for line in open(path) if line.strip()]
    posts = [r for r in rows if r.get("path", "").endswith("/chat/completions")]
    if not posts:
        continue
    usage = [r["usage"] for r in posts if r.get("usage")]
    tokens = sum(u.get("completion_tokens", 0) for u in usage)
    seconds = sum(r["seconds"] for r in posts if r.get("usage")) or 1
    print(f"| {os.path.basename(path)[6:-6]} | {len(posts)} | {sum(1 for r in posts if r.get('status') != 200)} | "
          f"{sum(1 for r in posts if r.get('raw_tool_text'))} | {sum(1 for r in posts if r.get('think_tags'))} | "
          f"{max((u.get('prompt_tokens', 0) for u in usage), default=0)} | {tokens / seconds:.1f} |")
