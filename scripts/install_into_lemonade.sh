#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LEMONADE_ROOT="${1:?usage: install_into_lemonade.sh /path/to/lemonade}"

install -d "$LEMONADE_ROOT/src/cpp/include/lemon/backends/cake"
install -d "$LEMONADE_ROOT/src/cpp/server/backends/cake"

install -m 0644 \
  "$ROOT/lemonade_patch/src/cpp/include/lemon/backends/cake/cake.h" \
  "$LEMONADE_ROOT/src/cpp/include/lemon/backends/cake/cake.h"

install -m 0644 \
  "$ROOT/lemonade_patch/src/cpp/include/lemon/backends/cake/cake_server.h" \
  "$LEMONADE_ROOT/src/cpp/include/lemon/backends/cake/cake_server.h"

install -m 0644 \
  "$ROOT/lemonade_patch/src/cpp/server/backends/cake/cake_server.cpp" \
  "$LEMONADE_ROOT/src/cpp/server/backends/cake/cake_server.cpp"

python3 - "$LEMONADE_ROOT/CMakeLists.txt" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = '    "vllm|vllm"\n'
entry = '    "cake|cake"\n'

if entry in text:
    print("Cake backend already registered.")
elif needle in text:
    path.write_text(text.replace(needle, needle + entry))
    print("Registered Cake backend after vLLM.")
else:
    raise SystemExit("Could not find vLLM registry line; edit LEMON_BACKENDS manually.")
PY

echo "Installed Cake source patch. Build the Rust bridge separately:"
echo "  cd \"$ROOT/rust_bridge\" && cargo build --release"
