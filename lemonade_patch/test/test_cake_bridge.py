#!/usr/bin/env python3
import json
import os
import sys
import requests

base = os.environ.get("CAKE_BRIDGE_URL", "http://127.0.0.1:19001")

def get(path):
    r = requests.get(base + path, timeout=10)
    r.raise_for_status()
    return r.json()

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
r = requests.post(base + "/v1/chat/completions", json=payload, timeout=120)
r.raise_for_status()
print("chat:", json.dumps(r.json(), indent=2))
