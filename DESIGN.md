# Design notes

## V1 boundary

This implementation deliberately separates three concerns:

1. **Lemonade lifecycle/API** — C++ `CakeServer`.
2. **Cake-specific integration** — Rust bridge.
3. **Forge/PAMP policy** — external consumer of topology/capability telemetry.

No expert movement is performed.

## Runtime identity

The Rust bridge verifies `/v1/models` before announcing readiness and exposes
`/internal/runtime-fingerprint`. The Lemonade C++ adapter refuses to accept the
backend as Cake unless that fingerprint verifies.

This directly prevents the recent class of error where a healthy endpoint was
mistaken for the intended runtime.

## Topology

Cake's native `/api/v1/topology` is preserved instead of normalized away.
Forge should consume this raw evidence first. A normalized Forge topology graph
can be derived later without losing Cake-specific fields.

## Future PAMP hooks

The bridge capability object already reserves:

- MoE router visibility
- expert residency visibility
- expert transfer visibility
- expert control
- placement validation/application

These are intentionally `unknown` or `shadow_only` until instrumentation proves
what Cake exposes in the exact model/runtime configuration.

## Future expert-fabric branch

The likely research progression is:

1. observe Cake tensor topology,
2. identify MoE expert tensors,
3. map expert -> tensor bytes -> device placement,
4. measure local placement and movement,
5. validate proposed placement plans in shadow mode,
6. only then add controlled placement mutation,
7. only after local proof, test inter-node expert-community placement.

This keeps Lemonade + Cake useful even if the expert-fabric experiment fails.
