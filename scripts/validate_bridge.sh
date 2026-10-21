#!/bin/bash
# Validate lemonade-cake-bridge against local cake per Forge_lemon-cake README order.
set -u
BRIDGE=/home/cesarops/repos/lemonade_cake_integration/rust_bridge/target/release/lemonade-cake-bridge
LOG=/tmp/bridge_run.log
OUT=/tmp/validate

export LEMONADE_CAKE_COMMAND='cake run {model} --api 127.0.0.1:{port}'
# NOTE: upstream cake-cli main-branch has a clap id collision: Serve's own
# `--api/-address` field clashes with flattened Args.{api,address}, which makes
# `cake serve <model>` fail with "required argument was not provided: address".
# The Run subcommand reaches the identical master+API code path, so the bridge
# launches Cake via `cake run {model} --api 127.0.0.1:{port}` instead.
mkdir -p "$OUT"

echo "== starting bridge =="
nohup "$BRIDGE" --listen-port 19101 --cake-port 8080 --model evilsocket/Qwen3-0.6B > "$LOG" 2>&1 &
BRIDGE_PID=$!
echo "bridge pid: $BRIDGE_PID"

# Wait for bridge health to report ok (cake model load can take ~1-2 min)
for i in $(seq 1 120); do
  sleep 5
  code=$(curl -s -o "$OUT/health.json" -w '%{http_code}' http://127.0.0.1:19101/health || true)
  if [ "$code" = "200" ]; then
    echo "health OK after $((i*5))s"
    break
  fi
done

echo "== endpoint checks =="
curl -s http://127.0.0.1:19101/health > "$OUT/health.json"; echo "health: $?"
curl -s http://127.0.0.1:19101/internal/runtime-fingerprint > "$OUT/fingerprint.json"; echo "fingerprint: $?"
curl -s http://127.0.0.1:19101/internal/capabilities > "$OUT/capabilities.json"; echo "capabilities: $?"
curl -s http://127.0.0.1:19101/api/v1/topology > "$OUT/topology.json"; echo "topology: $?"
curl -s http://127.0.0.1:19101/v1/models > "$OUT/models.json"; echo "models: $?"

echo "== blocking chat completion =="
curl -s -X POST http://127.0.0.1:19101/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"evilsocket/Qwen3-0.6B","messages":[{"role":"user","content":"Reply with exactly: BRIDGE OK"}],"max_tokens":32,"stream":false}' \
  > "$OUT/chat_blocking.json"; echo "blocking chat: $?"

echo "== streaming chat completion =="
curl -s -N -X POST http://127.0.0.1:19101/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"evilsocket/Qwen3-0.6B","messages":[{"role":"user","content":"Count from one to five."}],"max_tokens":48,"stream":true}' \
  > "$OUT/chat_stream.txt"; echo "streaming chat: $?"

kill "$BRIDGE_PID" 2>/dev/null
sleep 2
pkill -f 'cake serve' 2>/dev/null
echo "== done =="
