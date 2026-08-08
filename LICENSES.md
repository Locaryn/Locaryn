# Lochor licensing

Lochor is **open-core**: a permissive Apache-2.0 core (CLI, desktop app, local
daemon, provider-supervisor, all shared packages, examples, docs) plus a
Business Source License (BSL 1.1) module for the enterprise collaboration
features of the remote server. The BSL module converts to Apache-2.0 four years
after each tagged release, following the HashiCorp/Sentry pattern.

## Per-component breakdown

| Path | License | Notes |
| --- | --- | --- |
| `apps/cli/` | Apache-2.0 | Thin client over the daemon |
| `apps/desktop/` | Apache-2.0 | Tauri v2 + React frontend |
| `services/daemon/` | Apache-2.0 | Local loopback daemon |
| `services/provider-supervisor/` | Apache-2.0 | Local runtime supervisor (Ollama/llama.cpp/LM Studio/vLLM) |
| `services/remote-server/` (core) | Apache-2.0 | Auth, gateway, TLS, audit, provider proxying, sessions |
| `services/remote-server/enterprise/` | BSL 1.1 | Team context sharing, DGX Spark orchestration, concurrent-client gate |
| `packages/*` (all 16 Rust crates) | Apache-2.0 | Shared core |
| `packages-ui/*` | Apache-2.0 | Shared React components |
| `examples/` | Apache-2.0 (CC0 for the data) | Sample bundles |
| `migrations/` | Apache-2.0 | SQL migrations |
| `docs/` | CC-BY-4.0 | Documentation |

## Why this split

- The **core** stays permissive so the open-source community, plugin authors, and
  individual developers can self-host a complete local product (desktop + CLI +
  daemon + supervisor) with no restriction.
- The **enterprise remote-server module** is what enterprises pay for:
  cross-team context/file sharing on large collaborative projects, multi-tenant
  governance, NVIDIA DGX Spark orchestration, and SSO/mTLS. It is sold to
  enterprises who run DGX Spark clusters and want to share project context
  across collaborators with optimizations for large codebases.
- A **functional concurrent-client gate** lives in the binary (not in the licence
  text): the free remote-server build caps concurrent authenticated sessions.
  This keeps the surface simple while preserving the commercial story.

## BSL 1.1 change date

Each tagged release of `services/remote-server/enterprise/` becomes Apache-2.0
exactly four years after its release date. The change date is recorded in the
`Change Date` field at the top of every BSL-licensed file.

See `docs/adr/ADR-0003-open-core-license.md` for the full rationale.

## Third-party dependencies & bundled engines

Everything Lochor bundles or depends on is **permissively licensed** — there is
**no GPL/AGPL/SSPL** anywhere — so the paid, closed-source build is compliant.
Two obligations remain: **ship the notices** and **don't modify MPL-2.0 files**.

- **`THIRD_PARTY_LICENSES/`** — the notices that must travel with every
  distribution (llama.cpp, ggml, stable-diffusion.cpp — all MIT), the audit
  results, and the models policy (weights are user-downloaded, never bundled).
- **`deny.toml`** — cargo-deny gate; only permissive licenses may enter the
  Rust tree. Any GPL/AGPL fails CI.
- **`scripts/license-audit.{ps1,sh}`** — one command reproduces the full audit
  (Rust via cargo-deny, frontend via `pnpm licenses`, and the future Python
  sidecar via `pip-licenses`).

Audit snapshot: 708 third-party Rust crates + the React/Tauri frontend, all
permissive (the only `LGPL` string, `r-efi`, is one option of an `OR` we resolve
to MIT; 5 `MPL-2.0` CSS crates are used unmodified). Re-run the script after
adding the Python model-editing sidecar.
