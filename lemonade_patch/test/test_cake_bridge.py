#!/usr/bin/env python3
import json
import os
import sys
import urllib.request

base = os.environ.get("CAKE_BRIDGE_URL", "http://127.0.0.1:19001")

def get(path):
    with urllib.request.urlopen(base + path, timeout=10) as r:
        return json.load(r)

print("health:", json.dumps(get("/health"), indent=2))
print("fingerprint:", json.dumps(get("/internal/runtime-fingerprint"), indent=2))
print("capabilities:", json.dumps(get("/internal/capabilities"), indent=2))
print("topology:", json.dumps(get("/api/v1/topology"), indent=2))
print("models:", json.dumps(get("/v1/models"), indent=2))

payload = {
    "messages": [{"role": "user", "content": "Reply with exactly: cake-ok"}],
    "stream": False,
    "max_tokens": 16,
}
req = urllib.request.Request(
    base + "/v1/chat/completions",
    data=json.dumps(payload).encode(),
    headers={"Content-Type": "application/json"},
)
with urllib.request.urlopen(req, timeout=120) as r:
    print("chat:", json.dumps(json.load(r), indent=2))
