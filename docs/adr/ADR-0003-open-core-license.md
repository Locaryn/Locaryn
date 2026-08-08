# ADR-0003 — Open-core license (Apache-2.0 + BSL 1.1)

## Context
Lochor must be open-source and oriented toward open models, while allowing a future commercial path: sell an enterprise remote-server module to organizations running NVIDIA DGX Spark clusters, offering cross-team context/file sharing optimized for large collaborative projects. The user explicitly asked for a license that keeps most functionality free while reserving a server-side collaboration feature as commercial.

## Decision
- **Core (everything except enterprise module): Apache-2.0.** Includes apps/cli, apps/desktop, services/daemon, services/provider-supervisor, services/remote-server core (auth, gateway, TLS, audit, provider proxying, sessions), all `packages/*` crates, `packages-ui/*`, examples, migrations, docs (CC-BY-4.0 for prose).
- **Enterprise module (`services/remote-server/enterprise/`): BSL 1.1.** Covers team context sharing, DGX Spark orchestration, concurrent-client gate, multi-tenant governance hooks. Each tagged release converts to Apache-2.0 exactly **4 years** after its release date (recorded in the `Change Date` field at the top of every BSL file).
- **Functional concurrent-client gate** lives in the binary (free build caps concurrent authenticated sessions), not in the licence text. Keeps the legal surface simple.

## Consequences
- **Positive:** Community can self-host a complete local product (desktop + CLI + daemon + supervisor + non-enterprise remote) with no restriction; commercial story preserved; BSL change date builds trust (HashiCorp/Sentry pattern).
- **Negative:** BSL is not OSI-approved — some distributors may hesitate; requires clear messaging. Two-license repo needs tooling (cargo-deny allowlist, license headers per file).
- **Neutral:** The enterprise module is an optional crate; the remote-server compiles and runs fully without it (free tier).

## Alternatives considered
- **Pure Apache-2.0 + hosted SaaS:** rejected — user explicitly wants to sell the on-prem enterprise module, not a hosted tier in V1.
- **AGPL-3.0 for the remote-server:** rejected — copyleft strong but doesn't create a commercial lever for enterprise features; could deter enterprise adoption of the gateway itself.
- **GPL-3.0 + commercial dual-license:** rejected — more restrictive on the core than desired; we want the core maximally permissive.
- **Pure BSL for the whole remote-server:** rejected — would limit the open-source gateway adoption; we want the secured gateway itself to be Apache.

## References
- `LICENSES.md`
- `docs/architecture/03-tech-decisions.md` (D8, D9)
