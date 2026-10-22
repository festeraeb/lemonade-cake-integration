#!/bin/bash
# Validate lemonade-cake-bridge against local cake per Forge_lemon-cake README order.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRIDGE="${BRIDGE:-$ROOT/rust_bridge/target/release/lemonade-cake-bridge}"
MODEL="${MODEL:-evilsocket/Qwen3-0.6B}"
OUT="${OUT:-${TMPDIR:-/tmp}/validate}"
LOG="$OUT/bridge_run.log"
PIDFILE="$OUT/bridge.pid"
LISTEN_PORT="${LISTEN_PORT:-19101}"
CAKE_PORT="${CAKE_PORT:-8080}"

# cake 0.1.0 has no `serve` subcommand or `--port`; the API is `master --api`.
# Set USE_FAKE_CAKE=1 to launch scripts/fake_cake.py (a STUB, not Cake) instead.
# The template runs under `sh -lc`; `exec` keeps the bridge's kill on the real process.
if [ "${USE_FAKE_CAKE:-0}" = "1" ]; then
  export LEMONADE_CAKE_COMMAND="exec python3 -I $ROOT/scripts/fake_cake.py --model {model} --api 127.0.0.1:{port} --device {device}"
  echo "NOTE: running against fake_cake.py STUB; results do not describe Cake"
else
  export LEMONADE_CAKE_COMMAND="${LEMONADE_CAKE_COMMAND:-cake master --model {model} --api 127.0.0.1:{port} --device {device}}"
fi
mkdir -p "$OUT"

echo "== starting bridge =="
setsid "$BRIDGE" --listen-port "$LISTEN_PORT" --cake-port "$CAKE_PORT" --model "$MODEL" > "$LOG" 2>&1 < /dev/null &
BRIDGE_PID=$!
echo "$BRIDGE_PID" > "$PIDFILE"
echo "bridge pid: $BRIDGE_PID"

# Wait for bridge health to report ok (cake model load can take ~1-2 min)
for i in $(seq 1 120); do
  sleep 5
  code=$(curl -s -o "$OUT/health.json" -w '%{http_code}' http://127.0.0.1:$LISTEN_PORT/health || true)
  if [ "$code" = "200" ]; then
    echo "health OK after $((i*5))s"
    break
  fi
done

echo "== endpoint checks =="
curl -s http://127.0.0.1:$LISTEN_PORT/health > "$OUT/health.json"; echo "health: $?"
curl -s http://127.0.0.1:$LISTEN_PORT/internal/runtime-fingerprint > "$OUT/fingerprint.json"; echo "fingerprint: $?"
curl -s http://127.0.0.1:$LISTEN_PORT/internal/capabilities > "$OUT/capabilities.json"; echo "capabilities: $?"
curl -s http://127.0.0.1:$LISTEN_PORT/api/v1/topology > "$OUT/topology.json"; echo "topology: $?"
curl -s http://127.0.0.1:$LISTEN_PORT/v1/models > "$OUT/models.json"; echo "models: $?"

echo "== blocking chat completion =="
curl -s -X POST http://127.0.0.1:$LISTEN_PORT/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"'"$MODEL"'","messages":[{"role":"user","content":"Reply with exactly: BRIDGE OK"}],"max_tokens":32,"stream":false}' \
  > "$OUT/chat_blocking.json"; echo "blocking chat: $?"

echo "== streaming chat completion =="
curl -s -N -X POST http://127.0.0.1:$LISTEN_PORT/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"'"$MODEL"'","messages":[{"role":"user","content":"Count from one to five."}],"max_tokens":48,"stream":true}' \
  > "$OUT/chat_stream.txt"; echo "streaming chat: $?"

# The bridge kills its Cake child on SIGTERM.
kill "$BRIDGE_PID" 2>/dev/null
sleep 2
echo "== done =="
