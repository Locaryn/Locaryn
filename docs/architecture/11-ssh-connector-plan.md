# Lochor Store + SSH Connector — Implementation Plan

> Produced by a multi-agent design pass (explore → design panel → synthesis). This is the reference for building the extensions/connectors Store and the SSH server connector.

## 1. Recommended architecture

Ship an **SSH-specific connector** (dedicated `ssh_servers` table + `SshServerRepo` parallel to `ProviderRepo`), **not** a new `ExtensionKind` and **not** a fully-generic connector spine. Keep one cheap piece of extensibility: a static in-code connector catalog (`Vec<ConnectorType>`) so the Store's Discover tab is data-driven and a second connector is a drop-in later; the SSH config form is hand-written. SSH I/O lives in a new `packages/ssh` crate (`lochor-ssh`, russh). `agent-runtime` stays decoupled via a `ServerStore` trait object on `ToolContext`. Secrets live only in the OS Keychain (`packages/auth`); the DB holds `secret_ref`. Host keys are TOFU-pinned with explicit user confirmation; the AI only sees servers whose per-server `ai_access` was explicitly widened by the user.

## 2. SSH library

`russh 0.62` (default-features off, feature `ring`) + `russh-sftp 2.1` + `zeroize 1`. Native-tokio async, password + key auth, ProxyJump, exec, SFTP, no C toolchain. `ring` backend builds on MSVC without NASM/CMake.

## 3. Data model

New migration `migrations/0003_ssh_servers.sql`. **No column ever holds a password or private key** — only `secret_ref` (Keychain key). Table `ssh_servers`: id, name, description, host, port, username, auth_method (`password|key|agent`), secret_ref, key_path, jump_json, host_key_algo, host_key_sha256, host_key_verified, ai_access (`none|read_only|approval|trusted`), capabilities (JSON), scope, status, enabled, last_connected_at, created_at, updated_at, `UNIQUE(name, scope)`.

Secrets: password → Keychain `lochor/ssh/{id}`; key → reference `key_path` on disk, vault only passphrase; agent → nothing vaulted. Delete returns the freed `secret_ref` so the command layer deletes it from the Keychain (storage never imports auth).

Shared types: `SshServer` (no secret field), `SshAuthMethod`, `SshAiAccess`, `SshJump`. Reuse `ExtensionScope`.

## 4. Rust

New crate `packages/ssh` (`lochor-ssh`): `SshTarget`, `SshAuth`, `SshClient::{connect,run,probe}`, host-key handler that captures on first contact and constant-time compares against a pin on reconnect (hard fail on change). Probe: whoami/id, sudo -n true, uname, `ls -la $HOME` (read), SFTP write/read/delete `~/.syncho_probe` (write), all self-cleaning.

Tauri commands (desktop `lib.rs`): `list_connector_types`, `list_ssh_servers`, `test_ssh_connection` (streams `SshTestEvent`, no persist), `confirm_ssh_host_key`, `save_ssh_server` (server-side gated on a `test_token` + confirmed host key), `update_ssh_server`, `set_ssh_ai_access` (only user-initiated widening), `delete_ssh_server`. `Core` gains `keychain` + `pending_tests`.

## 5. Sandbox arm — risk-based approval (replaces name-based match)

> The previous "fix the requires_approval Sandbox arm from name-based to
> risk-based" todo is solved by the design in this section. Everything
> downstream — the modal, the API contract, the persistence layer — derives
> from this rule table.

### 5.1 Risk matrix

Each callable surface declares its **declared risk**. The runtime ALSO knows
whether a call is `is_remote`. The two combine into the **effective risk**
that drives the modal.

| Class                | Declared risk | Examples                              | Why |
|---|---|---|---|
| **read**             | `Low`         | `read_file`, `search`, `read_artifact`| Read-only, no state mutation, idempotent. |
| **write**            | `Medium`      | `write_file`, `update_server_description`, `list_dir` (no delete) | Local mutation; recoverable via git; user-visible diff. |
| **exec**             | `High`        | `run_command`, `apply_patch`, `git_commit` | Arbitrary shell in the project root; can delete files, spawn children. |
| **remote_exec**      | `Critical`    | `ssh_run_command`, MCP HTTP that mutates | Crosses the local trust boundary; cannot be undone locally; may cascade. |
| **dangerous_remote** | `Critical`    | `ssh_run_command` with `sudo: true`, MCP that touches production | Auto-denied unless the user types a confirmation. |

The four tiers compose into a single integer scale (`Low=0`, `Medium=1`,
`High=2`, `Critical=3`) so the rule table is a plain comparison.

**Remote escalation rule.** When the tool's `ToolContext::remote_target`
is `Some`, the runtime sets `effective_risk = Critical` *regardless* of the
declared tier. This is the line that closes the doc-11 §5 v1 loophole
(name-based bypass in `requires_approval`'s Sandbox arm) — a tool that
acts on a remote target can never be trusted on declared risk alone.

### 5.2 The rule table (canonical)

```
input: spec.risk (declared), ctx.trust, ctx.remote_target

1. Hard-block layer
   IF trust == Sandbox AND declared >= Medium
     → hard_blocked = true, modal shows Why-only (no Allow button)

2. Auto-approval layer (silent run, no modal)
   IF declared == Low AND NOT is_remote AND trust == Trusted
     → needs_user_consent = false (silent)

3. Escalation (always)
   IF is_remote
     → effective_risk = Critical
     → escalated_to_critical = (declared != Critical)

4. Modal layer
   ELSE (everything else)
     → needs_user_consent = true, modal opens with full payload
```

The boolean `requires_approval(spec, trust)` legacy helper IS preserved
(it now delegates to the rule table), so the daemon/CLI keep compiling.
New code should call `approval_decision(spec, ctx)` to receive the rich
`ApprovalDecision` struct (reason, diff, effective risk, hard_blocked).

### 5.3 The approval event (transport contract)

The runtime emits this SSE event before any tool call whose decision
said `needs_user_consent = true`:

```rust
StreamEvent::ToolApproval {
    call_id: uuid,                     // ties the response back to the call
    tool: spec.name,                   // matches a ToolSpec in the registry
    args: args,                        // serialised verbatim
    risk: effective_risk,              // after escalation (Critical if remote)
    reason: agent_reason.unwrap_or(spec.description),
    diff: render_diff(spec, args, ctx), // unified-diff head/tail for writes,
                                        // command preview for shell, server
                                        // + cmd for SSH. None for read-only.
    is_remote: bool,                   // drives the "Remote" banner
}
```

The frontend shows the modal, then POSTs back to:

```rust
approve_tool_call {
    call_id,
    decision: ApprovalVerdict::{Allow | Deny},
    scope:    RiskScope::{Once | Session | Project | Always},
    note:     Option<String>,          // user-typed rationale for audit
}
```

The runtime resumes the agent loop on `Allow`, marks the task as
denied on `Deny`, and updates the in-memory allowlist on
`Project` / `Always`.

### 5.4 Scopes — what they mean, what they gate

| Scope     | Storage                                                                       | UI affordance |
|---|---|---|
| `Once`     | Not stored. Only this call. Default for High/Critical.                       | Default chip; disabled when escalate above forces a wider choice. |
| `Session`  | `HashMap<session_id, HashSet<(tool, fingerprint)>>` in memory; lost on quit.  | Chip; only available when call has meaningful fingerprint. |
| `Project`  | Migration `0004_approval_decisions.sql` (V1.1). Keyed by `(project_id, tool)`. | Chip; warns before save ("every edit in this project will auto-run"). |
| `Always`   | Same table as Project, plus global flag overrides per server `ai_access`.     | Chip; hidden for Critical unless the user types the project path to confirm. |

The `is_allowed_for(risk_tier, scope)` matrix in `lochor-shared-types`
is the single source of truth — both the Rust rule table and the React
modal call into it, so they cannot disagree.

### 5.5 Critical-only hardening

Three extra rules that apply when `effective_risk == Critical`:

1. **Type-to-confirm.** The Allow button reveals a `<input>` that requires
   the user to type the project path (or server name) before it activates.
   Bypassing via Enter alone is disabled.
2. **No `Always` shortcut.** The modal hides the `Always` chip and
   requires the user to explicitly click `Project` to whitelist a
   server's AI access.
3. **Diff is mandatory.** If `render_diff()` returns `None` for a Critical
   call, the runtime REJECTS the call (returns `ApprovalDecision {
   hard_blocked: true, reason: "Cannot preview remote impact" }`) — the
   modal appears with only the Deny button so the user understands.

### 5.6 Agent integration

`ServerStore` trait on `ToolContext`. Tools shipped in order:

1. `update_server_description` (Medium, local) — first; uses the rule
   table directly without the remote escalation.
2. `ssh_run_command` (Critical, auto-escalated even if spec said High).
3. `ssh_list_dir` / `ssh_cat` (Medium → Critical via escalation) —
   read-side remote tools still require consent because they reveal
   remote state to the prompt.

System prompt renders ONLY servers with `ai_access != None`. When a
server's `ai_access` is `ReadOnly`, the runtime strips mutating tools
from the spec list entirely (the LLM never sees them) — a defence-in-
depth that the modal alone cannot provide.

AI-written descriptions and remote stdout are treated as untrusted;
length-capped to 4000 bytes and control-char stripped before they
reach the prompt.

`ServerStore` trait object on `ToolContext` exposes:

```rust
trait ServerStore {
    fn list_visible(&self, ai_filter: SshAiAccess) -> Vec<SshServer>;
    fn get(&self, id: Uuid) -> Option<SshServer>;
    fn require_for_tool(&self, call: &ToolCall) -> Result<SshServer, ToolError>;
}
```

`require_for_tool` ENFORCES the per-server `ai_access` gate: a call to
`ssh_run_command` referencing a server with `ai_access == None` returns
a `PermissionDenied` *before* the modal gets a chance to open. The
modal then appears with the `hard_blocked` flag set so the user
understands why the action failed.

---

## 6. Frontend

`App.tsx` view state `"chat"|"store"`; TopBar 🧩 toggle. `StorePanel.tsx`
(Discover / Installed sub-tabs). `SshServerForm.tsx` modal (fields, auth
method reveal, jump host, streaming Test, host-key confirmation, Save
gated on passing test + confirmed fingerprint). `core.ts` bindings +
demo fakes. CSS in house style (no glass/blur).

### 6.5 Tool Approval Modal (new)

> This section supersedes any per-tool-card inline approval pattern used
> before. Going forward, every tool call whose `ApprovalDecision`
> requires consent opens **this** modal, regardless of risk tier.

#### Props

```ts
type Props = {
  approval: ToolApprovalRequest;   // the SSE-decoded payload
  onResolve: (decision: ToolApprovalDecision) => void;
  onCancel:  () => void;            // closes the modal AND denies the call
};
```

`ToolApprovalRequest` mirrors the `StreamEvent::ToolApproval` variant
from `lochor-events` plus a `suggested_min_scope` derived from the rule
table (see §5.4). `ToolApprovalDecision` mirrors the Rust struct in
`lochor-shared-types` exactly; the frontend never invents extra fields.

#### Layout (must follow this order)

```
┌─────────────────────────────────────────────────────────────────────┐
│  [SEVERITY BANNER]                                                  │
│  Low: muted, no extra rules                                         │
│  Medium: amber border, "Reason required"                             │
│  High: orange border, "Diff mandatory"                              │
│  Critical: red border + pulsing dot, "Type project path to confirm" │
├─────────────────────────────────────────────────────────────────────┤
│  Tool: write_file            Risk: Medium 🟠                        │
│  Why: "refactor auth module"   ← decision.reason                   │
│  Remote: — (no escalation)                                          │
├─────────────────────────────────────────────────────────────────────┤
│  Preview:           (decision.diff, monospace, max-height 240px)    │
│  WRITE src/auth.rs                                                   │
│    size: 1284 bytes                                                  │
│    --- head ---                                                     │
│    use axum::{...};                                                 │
│    --- tail ---                                                     │
│    pub async fn login(...) { ... }                                  │
├─────────────────────────────────────────────────────────────────────┤
│  Apply to:   [ Once ] [ Session ] [ Project ] [ Always ]            │
│              ▲ suggested_min is highlighted as default; lower       │
│               scopes are disabled when rule forbids them.          │
├─────────────────────────────────────────────────────────────────────┤
│  [ Critical only ── Type to confirm ────────────────────────────── ] │
│  [ ] I understand the action runs on <target>.     [Cancel disable] │
├─────────────────────────────────────────────────────────────────────┤
│  [ Deny ]                                       [ Allow ◀ ]         │
└─────────────────────────────────────────────────────────────────────┘
```

**Critical banner specifics.** A pulsing red dot animates at 0.8 Hz. The
Allow button is disabled until BOTH (a) the type-to-confirm input
contains the project path or server name AND (b) the user clicks the
"I understand…" checkbox. There is no way to bypass via Enter, no way
to use `Always` without first clicking `Project` then expanding. Pressing
**Escape** is treated as **Deny** for Medium and HIGH but is **disabled**
for Critical (must click Deny explicitly) — the modal traps focus on
Critical calls.

**Accessibility.** `role="dialog"`, `aria-modal="true"`,
`aria-labelledby="lochor-approval-title"`. Focus is moved to the
type-to-confirm input on Critical; to the Allow button otherwise. The
modal restores focus to the chat composer when closed.

**Persistence.** When the user picks `Project` or `Always`, the frontend
POSTs the decision through `core.approveToolCall(...)` and the backend
mirrors it in `approval_decisions` (V1.1 migration 0004). For now (V1)
`Session` scope is in-memory only and is reset every daemon restart —
clearly labelled in the chip tooltip ("lost when you quit Lochor").

#### Wire events

The chat panel subscribes to `StreamEvent::ToolApproval` over the same
`Channel<StreamEvent>` that delivers tokens. When one arrives:
1. Pause the token stream (Tauri Channel queues events while modal is open).
2. Mount the modal with the decoded payload.
3. On resolve, send `approve_tool_call` IPC, then resume the stream.
4. The next event is expected to be `tool_result`; if Deny, it's
   `tool_result { ok: false, output: "denied by user" }`.

If the modal is cancelled (Esc, X button, focus loss — except Critical)
the agent receives a denial and continues from there.

---

## 7. Security decisions

No secret in SQLite/plaintext; Keychain only. Refuse password/pasted-key
saves under NullKeychain. Host-key TOFU + explicit confirm + hard fail
on change. Save gated server-side on a verified test_token.
Least-privilege self-cleaning probe. Per-server `ai_access` default
`none`, only user widens. **Approval gate moved from name-based to
risk-based** (§5). **Interactive consent is mandatory for any tool
classified Medium or above** (§5). **Remote tools are auto-escalated to
Critical regardless of declared tier** (§5.1). Real interactive consent
before AI remote mutation. Untrusted-data handling. Secrets in
`Zeroizing`, never on `ToolContext`/`StreamEvent`/`tracing`.

### 7.1 Audit chain

Every `ApprovalDecision` is written to `audit_logs` (existing table from
migration 0002) with event name `tool.approval.{allow|deny}` and an
opaque diff SHA-256 (the diff text itself is NOT logged — only its hash,
for replay debugging without leaking secrets). The decision struct's
`decided_by` field carries the local user id (`"local"` for now;
remote-server users get their actual ID in V1.1).

## 8. Build order (green gate each step)

1. Migration + shared types (`RiskScope`, `Decision`, etc.).
2. Storage repo (`SshServerRepo`) + register + re-export.
3. Auth helper `ssh_key(Uuid)`.
4. `packages/ssh` crate (connect/run/probe + host-key handler).
5. Tauri commands + Core wiring (keychain + pending_tests).
6. core.ts bindings + demo fakes.
7. Store view + form + CSS.
8. Agent description loop (`ServerStore` + `update_server_description` + injection).
9. **Risk-based approval rule table** (§5) + `Risk::Critical` enum value + `approval_decision` API in `lochor-agent-runtime`.
10. **Tool Approval Modal** (§6.5) — React component with severity banner, diff preview, scope chips, Critical type-to-confirm.
11. Wire `StreamEvent::ToolApproval` → modal → `approve_tool_call` IPC → agent resume (Critical paths tested first).
12. **`ssh_run_command` tool behind Critical + ai_access allowlist** (the first tool that pays for the new gate).

## 9. MVP cuts (defer)

`ssh_command_log` audit table (only `audit_logs` is written in V1);
generic connector spine; known_hosts import; pasting raw private-key text;
password auth on jump host; pooled connections; `Project`/`Always` scopes
beyond `Session` (V1 keeps allowlist in-memory; persistence ships with
migration 0004 in V1.1); MCP `ServerStore`-style connectors; "Open in
browser" preview override (preview locked to sandbox origin).
