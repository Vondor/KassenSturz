#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Loopback proxy for the Qwen stress test (hand-run, never in CI).

Sits between KassenSturz's chat (OpenAI-compatible, non-streaming) and a local
model server, logs every exchange (sizes, usage, timing, tool-call shape) and
can change requests or replies the way Qwen models and their servers really
differ, so the test shows which shapes KassenSturz copes with. Listens and
forwards on loopback only; nothing leaves the computer.

  qwen-proxy.py --listen 1235 --upstream http://127.0.0.1:1234 \
      --think off --fault raw_tool_text --log proxy.jsonl

--think on|off|keep   off adds chat_template_kwargs.enable_thinking=false and a
                      "/no_think" line to the last user message (Qwen3 soft
                      switch); keep sends the request unchanged.
--fault NAME          one reply change, applied to every reply (see FAULTS).
--delay SECONDS       wait before answering (timeout handling).
"""
import argparse, json, re, sys, time, urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

FAULTS = {
    "none": "reply unchanged",
    "raw_tool_text": "tool calls moved into the text as <tool_call>{json}</tool_call> (server did not parse Qwen's format)",
    "think_in_text": "a <think>...</think> block put in front of the text",
    "reasoning_only": "text moved to reasoning_content, content empty",
    "args_object": "arguments sent as a JSON object instead of a string",
    "no_call_id": "tool call ids removed",
    "stop_with_calls": "finish_reason 'stop' although there are tool calls",
    "parallel_dup": "every tool call sent twice",
    "fenced_args": "arguments wrapped in a ```json fence",
    "length_cut": "text cut in half and finish_reason 'length'",
}


def change_request(body, think):
    if think == "off":
        body.setdefault("chat_template_kwargs", {})["enable_thinking"] = False
        for message in reversed(body.get("messages", [])):
            if message.get("role") == "user" and isinstance(message.get("content"), str):
                message["content"] += "\n/no_think"
                break
    return body


def change_reply(reply, fault):
    for choice in reply.get("choices", []):
        message = choice.get("message", {})
        calls = message.get("tool_calls") or []
        text = message.get("content") or ""
        if fault == "raw_tool_text" and calls:
            parts = [
                "<tool_call>\n" + json.dumps({"name": c["function"]["name"],
                                              "arguments": json.loads(c["function"].get("arguments") or "{}")})
                + "\n</tool_call>"
                for c in calls
            ]
            message["content"] = (text + "\n" if text else "") + "\n".join(parts)
            message.pop("tool_calls", None)
            choice["finish_reason"] = "stop"
        elif fault == "think_in_text":
            message["content"] = "<think>\nLet me look at the book first.\n</think>\n" + text
        elif fault == "reasoning_only" and text and not calls:
            message["reasoning_content"] = text
            message["content"] = ""
        elif fault == "args_object":
            for c in calls:
                try:
                    c["function"]["arguments"] = json.loads(c["function"].get("arguments") or "{}")
                except ValueError:
                    pass
        elif fault == "no_call_id":
            for c in calls:
                c.pop("id", None)
        elif fault == "stop_with_calls" and calls:
            choice["finish_reason"] = "stop"
        elif fault == "parallel_dup" and calls:
            message["tool_calls"] = calls + [dict(c, id=(c.get("id") or "x") + "-dup") for c in calls]
        elif fault == "fenced_args":
            for c in calls:
                c["function"]["arguments"] = "```json\n" + (c["function"].get("arguments") or "{}") + "\n```"
        elif fault == "length_cut" and text and not calls:
            message["content"] = text[: len(text) // 2]
            choice["finish_reason"] = "length"
    return reply


class Handler(BaseHTTPRequestHandler):
    def _forward(self, data):
        request = urllib.request.Request(ARGS.upstream + self.path, data=data, method=self.command,
                                         headers={"Content-Type": "application/json"})
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        with opener.open(request, timeout=3600) as response:
            return response.status, response.read()

    def _send(self, status, payload):
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_GET(self):
        try:
            status, payload = self._forward(None)
        except Exception as error:  # report, never crash the run
            status, payload = 502, json.dumps({"error": str(error)}).encode()
        self._send(status, payload)

    def do_POST(self):
        raw = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        started = time.time()
        entry = {"path": self.path, "request_bytes": len(raw), "think": ARGS.think, "fault": ARGS.fault}
        try:
            body = change_request(json.loads(raw), ARGS.think)
            entry["messages"] = len(body.get("messages", []))
            entry["tools"] = len(body.get("tools") or [])
            status, payload = self._forward(json.dumps(body).encode())
            reply = json.loads(payload)
            choice = (reply.get("choices") or [{}])[0]
            message = choice.get("message", {})
            content = message.get("content") or ""
            entry.update({
                "status": status,
                "usage": reply.get("usage"),
                "finish": choice.get("finish_reason"),
                "calls": [c.get("function", {}).get("name") for c in message.get("tool_calls") or []],
                "content_chars": len(content),
                "reasoning_chars": len(message.get("reasoning_content") or ""),
                "raw_tool_text": "<tool_call>" in content,
                "think_tags": "<think>" in content,
            })
            reply = change_reply(reply, ARGS.fault)
            payload = json.dumps(reply).encode()
        except urllib.error.HTTPError as error:
            status, payload = error.code, error.read()
            entry.update({"status": status, "error": payload[:300].decode("utf-8", "replace")})
        except Exception as error:
            status, payload = 502, json.dumps({"error": str(error)}).encode()
            entry.update({"status": 502, "error": str(error)})
        if ARGS.delay:
            time.sleep(ARGS.delay)
        entry["seconds"] = round(time.time() - started, 1)
        with open(ARGS.log, "a") as log:
            log.write(json.dumps(entry) + "\n")
        self._send(status, payload)

    def log_message(self, *args):
        pass


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--listen", type=int, default=1235)
    parser.add_argument("--upstream", default="http://127.0.0.1:1234")
    parser.add_argument("--think", choices=["on", "off", "keep"], default="keep")
    parser.add_argument("--fault", choices=sorted(FAULTS), default="none")
    parser.add_argument("--delay", type=float, default=0)
    parser.add_argument("--log", default="proxy.jsonl")
    ARGS = parser.parse_args()
    if not re.match(r"^https?://(127\.0\.0\.1|localhost|\[::1\])(:\d+)?$", ARGS.upstream):
        sys.exit("qwen-proxy: the upstream must be on this computer")
    ThreadingHTTPServer(("127.0.0.1", ARGS.listen), Handler).serve_forever()
