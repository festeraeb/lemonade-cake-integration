#!/bin/bash
# Full-chain validation: Lemonade -> lemonade-cake-bridge -> Cake -> Qwen3-0.6B
set -u
LEMOND=/home/cesarops/repos/lemonade-upstream/build/lemond
OUT=/tmp/e2e
LOG_LEMOND=$OUT/lemond.log
mkdir -p "$OUT"

pkill -f 'lemonade-cake-bridge' 2>/dev/null
pkill -f 'cake run' 2>/dev/null
sleep 1

echo "== [1] starting lemond on :8100 =="
nohup "$LEMOND" --port 8100 > "$LOG_LEMOND" 2>&1 &
LEMOND_PID=$!
echo "lemond pid: $LEMOND_PID"

for i in $(seq 1 30); do
  sleep 2
  code=$(curl -s -o "$OUT/health_initial.json" -w '%{http_code}' http://127.0.0.1:8100/api/v1/health || true)
  [ "$code" = "200" ] && { echo "lemond healthy after $((i*2))s"; break; }
done

echo "== [2] loading model with recipe=cake =="
curl -s -X POST http://127.0.0.1:8100/api/v1/load \
  -H 'Content-Type: application/json' \
  -d '{"model":"evilsocket/Qwen3-0.6B","recipe":"cake"}' > "$OUT/load.json"
echo "load exit: $?"
cat "$OUT/load.json"; echo

# give the bridge time for fingerprint verification + watchdog start
sleep 5

echo "== [3] locate bridge child process/port =="
pgrep -af 'lemonade-cake-bridge|cake run' > "$OUT/procs.txt" 2>&1
ss -tlnp > "$OUT/listeners.txt" 2>&1
BRIDGE_PORT=$(ss -tlnp 2>/dev/null | grep lemonade-cake-bridge | grep -oE '127\.0\.0\.1:[0-9]+' | head -1 | cut -d: -f2)
echo "bridge port: ${BRIDGE_PORT:-unknown}"

echo "== [4] blocking chat via Lemonade chain =="
curl -s --max-time 240 -X POST http://127.0.0.1:8100/api/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"qwen3","messages":[{"role":"user","content":"Reply with exactly: CHAIN OK"}],"max_tokens":40,"stream":false}' \
  > "$OUT/chat_blocking.json"
echo "blocking exit: $?"

echo "== [5] streaming chat via Lemonade chain =="
curl -s -N --max-time 240 -X POST http://127.0.0.1:8100/api/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"qwen3","messages":[{"role":"user","content":"Count one to five."}],"max_tokens":48,"stream":true}' \
  > "$OUT/chat_stream.txt"
echo "streaming exit: $?"

echo "== [6] telemetry should embed cake topology/fingerprint =="
curl -s http://127.0.0.1:8100/api/v1/health > "$OUT/health_loaded.json"

if [ -n "${BRIDGE_PORT:-}" ]; then
  echo "== [7] placement apply must be 501 (direct to bridge) =="
  code=$(curl -s -o "$OUT/placement_apply.json" -w '%{http_code}' -X POST \
    "http://127.0.0.1:${BRIDGE_PORT}/internal/placement/apply" \
    -H 'Content-Type: application/json' \
    -d '{"schema":"test","assignments":[]}')
  echo "placement apply HTTP: $code"
fi

echo "== [8] unload =="
curl -s -X POST http://127.0.0.1:8100/api/v1/unload > "$OUT/unload.json"
sleep 6
pgrep -af 'lemonade-cake-bridge|cake run' > "$OUT/procs_after_unload.txt" 2>&1 || true

echo "== [9] reload (restart) =="
curl -s -X POST http://127.0.0.1:8100/api/v1/load \
  -H 'Content-Type: application/json' \
  -d '{"model":"evilsocket/Qwen3-0.6B","recipe":"cake"}' > "$OUT/reload.json"
sleep 8
curl -s --max-time 240 -X POST http://127.0.0.1:8100/api/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"qwen3","messages":[{"role":"user","content":"Say hi."}],"max_tokens":24,"stream":false}' \
  > "$OUT/chat_after_reload.json"
echo "post-reload chat exit: $?"

echo "== [10] final unload + shutdown =="
curl -s -X POST http://127.0.0.1:8100/api/v1/unload > "$OUT/unload2.json"
sleep 6
kill "$LEMOND_PID" 2>/dev/null
sleep 4
pgrep -af 'lemonade-cake-bridge|cake run' > "$OUT/procs_final.txt" 2>&1 || echo "no leftover processes" >> "$OUT/procs_final.txt"
echo "== done =="
