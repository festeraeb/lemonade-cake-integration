# Lemonade + Cake integration spike

This package is a production-shaped integration scaffold, not a disposable OpenAI passthrough.

## Architecture

Lemonade remains the public API/model lifecycle layer.

`CakeServer` is a thin C++ `WrappedServer` adapter. It launches the Rust
`lemonade-cake-bridge` as the backend process on Lemonade's assigned private port.

The Rust bridge:

- owns the Cake child process lifecycle,
- verifies the runtime identity,
- waits for Cake readiness,
- forwards OpenAI-compatible chat/model traffic,
- preserves Cake's `/api/v1/topology`,
- exposes a normalized capability/fingerprint surface for Forge/PAMP,
- reserves a shadow-only placement-plan contract for future expert placement work.

Cake itself remains unmodified in this first implementation.

## Why a Rust bridge?

Current Lemonade `WrappedServer` backends are C++ subprocess adapters. The project is
Rust-first, so the C++ layer stays intentionally thin and all Cake-specific process,
topology, observability, and future placement logic live in Rust.

## Important assumptions

Cake documents these endpoints:

- `POST /v1/chat/completions`
- `GET /v1/models`
- `GET /api/v1/topology`

Cake's exact CLI flags for binding a custom port may change. The bridge therefore
accepts a command template instead of hard-coding Cake's CLI. Set
`LEMONADE_CAKE_COMMAND` or pass `--cake-command` with `{model}` and `{port}`
placeholders.

Example:

```bash
LEMONADE_CAKE_COMMAND='cake serve {model} --port {port}' \
  lemonade-cake-bridge \
  --listen-port 19001 \
  --cake-port 19002 \
  --model evilsocket/Qwen2.5-Coder-1.5B-Instruct
```

If current Cake uses a different port flag, only the command template changes.

## Validation order

1. Build `rust_bridge`.
2. Run the bridge against a known-working Cake command template.
3. Verify:
   - `/health`
   - `/internal/runtime-fingerprint`
   - `/internal/capabilities`
   - `/api/v1/topology`
   - `/v1/models`
   - blocking `/v1/chat/completions`
   - streaming `/v1/chat/completions`
4. Copy the Lemonade patch files into the current Lemonade tree.
5. Add `"cake|cake"` to `LEMON_BACKENDS`.
6. Build Lemonade.
7. Run a Lemonade load/chat smoke test.
8. Only after that, add model registry entries and packaging/install automation.

## PAMP boundary

This package does **not** move experts.

It exposes enough evidence for a future `MoeTelemetryProvider`:

- backend identity,
- topology,
- runtime capabilities,
- future placement-plan validation.

`POST /internal/placement/apply` intentionally returns `501 Not Implemented`.
PAMP remains shadow-only.

Fix 2026-10-22: the `cake_port` descriptor default is now `""` (string); a numeric default made `RecipeOptions::get_option` throw `type must be string, but is number` on every Cake load.
