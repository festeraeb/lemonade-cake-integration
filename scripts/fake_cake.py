#!/usr/bin/env python3
"""STUB standing in for `cake master --api`; it is NOT Cake and runs no model.

Serves just enough of Cake's HTTP surface for the bridge's readiness check and
runtime fingerprint (GET /v1/models returning an OpenAI-style `data` array):

  GET  /v1/models
  GET  /api/v1/topology
  POST /v1/chat/completions   (blocking JSON, or SSE when "stream": true)

Every payload is labelled "fake-cake-stub" so stub results cannot be mistaken
for real Cake output. Accepts and ignores the real Cake master flags so it can
be dropped into LEMONADE_CAKE_COMMAND:

  LEMONADE_CAKE_COMMAND='exec python3 -I scripts/fake_cake.py --model {model} --api 127.0.0.1:{port} --device {device}'
"""
import argparse
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

STUB = "fake-cake-stub"
MODEL = "stub-model"
REPLY = "[STUB: not Cake] cake-ok"


def log(msg):
    print(f"[{STUB}] {msg}", file=sys.stderr, flush=True)


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        log("%s %s" % (self.command, self.path))

    def _json(self, code, body):
        data = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.send_header("X-Stub", STUB)
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path == "/v1/models":
            self._json(200, {"object": "list", "stub": STUB,
                             "data": [{"id": MODEL, "object": "model", "owned_by": STUB}]})
        elif self.path == "/api/v1/topology":
            self._json(200, {"stub": STUB, "note": "fabricated single-node topology, not Cake",
                             "workers": [], "master": {"name": STUB, "device": "none"}})
        else:
            self._json(404, {"error": "not found", "stub": STUB})

    def do_POST(self):
        length = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(length) if length else b"{}"
        if self.path != "/v1/chat/completions":
            return self._json(404, {"error": "not found", "stub": STUB})
        try:
            req = json.loads(raw or b"{}")
        except ValueError:
            return self._json(400, {"error": "invalid JSON", "stub": STUB})
        base = {"id": "chatcmpl-" + STUB, "created": int(time.time()), "model": MODEL,
                "system_fingerprint": STUB}
        if not req.get("stream"):
            return self._json(200, {**base, "object": "chat.completion", "stub": STUB,
                                    "choices": [{"index": 0, "finish_reason": "stop",
                                                 "message": {"role": "assistant", "content": REPLY}}],
                                    "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0}})
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.send_header("X-Stub", STUB)
        self.end_headers()
        pieces = REPLY.split(" ")
        for i, piece in enumerate(pieces):
            delta = {"content": piece + (" " if i < len(pieces) - 1 else "")}
            if i == 0:
                delta["role"] = "assistant"
            chunk = {**base, "object": "chat.completion.chunk",
                     "choices": [{"index": 0, "delta": delta, "finish_reason": None}]}
            self.wfile.write(f"data: {json.dumps(chunk)}\n\n".encode())
            self.wfile.flush()
        done = {**base, "object": "chat.completion.chunk",
                "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]}
        self.wfile.write(f"data: {json.dumps(done)}\n\ndata: [DONE]\n\n".encode())
        self.wfile.flush()
        self.close_connection = True


def main():
    ap = argparse.ArgumentParser(description="Stub for cake master --api (NOT Cake)")
    ap.add_argument("--api", default="127.0.0.1:8080", help="host:port to serve on")
    # Accepted and ignored, mirroring `cake master`.
    ap.add_argument("--model")
    ap.add_argument("--device")
    ap.add_argument("--address")
    ap.add_argument("--cpu", action="store_true")
    args = ap.parse_args()
    host, _, port = args.api.rpartition(":")
    server = ThreadingHTTPServer((host or "127.0.0.1", int(port)), Handler)
    log(f"STUB serving on {args.api}; this is not Cake and runs no model")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
