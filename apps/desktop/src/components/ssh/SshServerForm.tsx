import { Icon } from "@locaryn/ui-core";
import { useState } from "react";
import {
  type SshAuthMethod,
  type SshProbeResult,
  type SshServerDraft,
  type SshTestEvent,
  core,
} from "../../lib/core";

type Props = {
  onClose: () => void;
  onSaved: () => void;
};

type TestState = "idle" | "testing" | "tested" | "error";

export function SshServerForm({ onClose, onSaved }: Props) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [host, setHost] = useState("");
  const [port, setPort] = useState("22");
  const [username, setUsername] = useState("");
  const [authMethod, setAuthMethod] = useState<SshAuthMethod>("agent");
  const [keyPath, setKeyPath] = useState("");
  const [password, setPassword] = useState("");
  const [passphrase, setPassphrase] = useState("");

  const [useJump, setUseJump] = useState(false);
  const [jHost, setJHost] = useState("");
  const [jPort, setJPort] = useState("22");
  const [jUser, setJUser] = useState("");
  const [jAuth, setJAuth] = useState<Exclude<SshAuthMethod, "password">>("agent");
  const [jKeyPath, setJKeyPath] = useState("");

  const [testState, setTestState] = useState<TestState>("idle");
  const [statusLine, setStatusLine] = useState("");
  const [probe, setProbe] = useState<SshProbeResult | null>(null);
  const [hostKeyConfirmed, setHostKeyConfirmed] = useState(false);
  const [saving, setSaving] = useState(false);

  // Any change to connection-identifying fields invalidates the test.
  function resetTest() {
    setTestState("idle");
    setProbe(null);
    setHostKeyConfirmed(false);
    setStatusLine("");
  }

  function buildDraft(): SshServerDraft {
    return {
      name,
      description,
      host: host.trim(),
      port: Number(port) || 22,
      username: username.trim(),
      auth_method: authMethod,
      key_path: authMethod === "key" ? keyPath.trim() || null : null,
      scope: "user",
      jump: useJump
        ? {
            host: jHost.trim(),
            port: Number(jPort) || 22,
            username: jUser.trim(),
            auth_method: jAuth,
            key_path: jAuth === "key" ? jKeyPath.trim() || null : null,
          }
        : null,
    };
  }

  function secretValue(): string | null {
    if (authMethod === "password") return password;
    if (authMethod === "key") return passphrase || null;
    return null;
  }

  function statusFromEvent(ev: SshTestEvent): string {
    switch (ev.type) {
      case "connecting":
        return "connecting…";
      case "authenticating":
        return "authenticating…";
      case "probing":
        return `probing ${ev.step}…`;
      case "done":
        return "done";
      case "error":
        return ev.message;
    }
  }

  async function runTest() {
    if (!host.trim() || !username.trim()) {
      setTestState("error");
      setStatusLine("host and username are required");
      return;
    }
    setTestState("testing");
    setProbe(null);
    setHostKeyConfirmed(false);
    setStatusLine("connecting…");
    try {
      const res = await core.testSshConnection(buildDraft(), secretValue(), (ev) =>
        setStatusLine(statusFromEvent(ev)),
      );
      setProbe(res);
      setTestState("tested");
      if (!description.trim()) setDescription(res.suggested_description);
    } catch (e) {
      setTestState("error");
      setStatusLine(String(e).replace(/^Error:\s*/, ""));
    }
  }

  async function confirmKey() {
    if (!probe) return;
    try {
      await core.confirmSshHostKey(probe.test_token);
      setHostKeyConfirmed(true);
    } catch (e) {
      setStatusLine(String(e).replace(/^Error:\s*/, ""));
    }
  }

  async function save() {
    if (!probe || !hostKeyConfirmed || !name.trim()) return;
    setSaving(true);
    try {
      await core.saveSshServer(buildDraft(), secretValue(), probe.test_token);
      onSaved();
    } catch (e) {
      setTestState("error");
      setStatusLine(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setSaving(false);
    }
  }

  const canSave = testState === "tested" && hostKeyConfirmed && !!name.trim() && !saving;

  return (
    <>
      <div
        className="locaryn-settings-backdrop"
        onClick={onClose}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
        }}
      />
      <div
        // biome-ignore lint/a11y/useSemanticElements: <dialog> apporte le padding, la marge auto et le positionnement par défaut de l'agent utilisateur, que .locaryn-form-modal (fixe, centré par transform, sans padding) ne réinitialise pas ; la modale est montée et démontée par React, jamais par showModal().
        className="locaryn-form-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Add SSH server"
      >
        <div className="locaryn-settings-header">
          <span className="locaryn-settings-title">Add SSH server</span>
          <button
            type="button"
            className="locaryn-settings-close"
            onClick={onClose}
            aria-label="Close"
          >
            <Icon name="close" size={16} />
          </button>
        </div>

        <div className="locaryn-form-body">
          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor="ssh-name">
              Name
            </label>
            <input
              id="ssh-name"
              className="locaryn-input locaryn-input-text"
              value={name}
              placeholder="web-prod"
              onChange={(e) => setName(e.target.value)}
            />
          </div>

          <div className="locaryn-field-grid">
            <div className="locaryn-field locaryn-field-grow">
              <label className="locaryn-field-label" htmlFor="ssh-host">
                Host / IP
              </label>
              <input
                id="ssh-host"
                className="locaryn-input"
                value={host}
                placeholder="10.0.0.4"
                spellCheck={false}
                onChange={(e) => {
                  setHost(e.target.value);
                  resetTest();
                }}
              />
            </div>
            <div className="locaryn-field locaryn-field-port">
              <label className="locaryn-field-label" htmlFor="ssh-port">
                Port
              </label>
              <input
                id="ssh-port"
                className="locaryn-input"
                value={port}
                onChange={(e) => {
                  setPort(e.target.value.replace(/\D/g, ""));
                  resetTest();
                }}
              />
            </div>
          </div>

          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor="ssh-username">
              Username
            </label>
            <input
              id="ssh-username"
              className="locaryn-input"
              value={username}
              placeholder="deploy"
              spellCheck={false}
              onChange={(e) => {
                setUsername(e.target.value);
                resetTest();
              }}
            />
          </div>

          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor="ssh-auth-method">
              Authentication
            </label>
            <select
              id="ssh-auth-method"
              className="locaryn-select"
              value={authMethod}
              onChange={(e) => {
                setAuthMethod(e.target.value as SshAuthMethod);
                resetTest();
              }}
            >
              <option value="agent">SSH agent (recommended)</option>
              <option value="key">Private key file</option>
              <option value="password">Password</option>
            </select>
            <p className="locaryn-field-hint">
              {authMethod === "agent"
                ? "Uses keys already loaded in your system's SSH agent (ssh-add on macOS/Linux, Pageant/OpenSSH agent on Windows). Nothing to enter here — pick this if `ssh you@host` already works in your terminal."
                : authMethod === "key"
                  ? "Point to a private-key file on disk (e.g. ~/.ssh/id_ed25519). Add a passphrase only if the key is encrypted."
                  : "The password is stored in your OS keychain, never in the database."}
            </p>
          </div>

          {authMethod === "key" && (
            <div className="locaryn-field">
              <label className="locaryn-field-label" htmlFor="ssh-key-path">
                Private key path
              </label>
              <input
                id="ssh-key-path"
                className="locaryn-input"
                value={keyPath}
                placeholder="~/.ssh/id_ed25519"
                spellCheck={false}
                onChange={(e) => {
                  setKeyPath(e.target.value);
                  resetTest();
                }}
              />
              <input
                className="locaryn-input locaryn-input-text"
                type="password"
                value={passphrase}
                placeholder="key passphrase (optional)"
                onChange={(e) => setPassphrase(e.target.value)}
              />
            </div>
          )}

          {authMethod === "password" && (
            <div className="locaryn-field">
              <label className="locaryn-field-label" htmlFor="ssh-password">
                Password
              </label>
              <input
                id="ssh-password"
                className="locaryn-input locaryn-input-text"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            </div>
          )}

          <div className="locaryn-field">
            <label className="locaryn-toggle-row">
              <span className="locaryn-field-label">Connect through a jump host</span>
              <input
                type="checkbox"
                checked={useJump}
                onChange={(e) => {
                  setUseJump(e.target.checked);
                  resetTest();
                }}
              />
            </label>
          </div>

          {useJump && (
            <div className="locaryn-jump-box">
              <div className="locaryn-field-grid">
                <div className="locaryn-field locaryn-field-grow">
                  <label className="locaryn-field-label" htmlFor="ssh-jump-host">
                    Jump host
                  </label>
                  <input
                    id="ssh-jump-host"
                    className="locaryn-input"
                    value={jHost}
                    spellCheck={false}
                    onChange={(e) => {
                      setJHost(e.target.value);
                      resetTest();
                    }}
                  />
                </div>
                <div className="locaryn-field locaryn-field-port">
                  <label className="locaryn-field-label" htmlFor="ssh-jump-port">
                    Port
                  </label>
                  <input
                    id="ssh-jump-port"
                    className="locaryn-input"
                    value={jPort}
                    onChange={(e) => {
                      setJPort(e.target.value.replace(/\D/g, ""));
                      resetTest();
                    }}
                  />
                </div>
              </div>
              <div className="locaryn-field">
                <label className="locaryn-field-label" htmlFor="ssh-jump-username">
                  Jump username
                </label>
                <input
                  id="ssh-jump-username"
                  className="locaryn-input"
                  value={jUser}
                  spellCheck={false}
                  onChange={(e) => {
                    setJUser(e.target.value);
                    resetTest();
                  }}
                />
              </div>
              <div className="locaryn-field">
                <label className="locaryn-field-label" htmlFor="ssh-jump-auth">
                  Jump auth
                </label>
                <select
                  id="ssh-jump-auth"
                  className="locaryn-select"
                  value={jAuth}
                  onChange={(e) => {
                    setJAuth(e.target.value as "agent" | "key");
                    resetTest();
                  }}
                >
                  <option value="agent">SSH agent</option>
                  <option value="key">Private key</option>
                </select>
              </div>
              {jAuth === "key" && (
                <div className="locaryn-field">
                  <label className="locaryn-field-label" htmlFor="ssh-jump-key-path">
                    Jump key path
                  </label>
                  <input
                    id="ssh-jump-key-path"
                    className="locaryn-input"
                    value={jKeyPath}
                    spellCheck={false}
                    onChange={(e) => {
                      setJKeyPath(e.target.value);
                      resetTest();
                    }}
                  />
                </div>
              )}
            </div>
          )}

          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor="ssh-description">
              Description
            </label>
            <textarea
              id="ssh-description"
              className="locaryn-textarea"
              value={description}
              rows={2}
              placeholder="What is this server for? (auto-filled from the connection test)"
              onChange={(e) => setDescription(e.target.value)}
            />
          </div>

          {/* Test + host-key confirmation */}
          <div className="locaryn-field">
            <div className="locaryn-field-row">
              <button
                type="button"
                className="locaryn-btn-ghost"
                onClick={runTest}
                disabled={testState === "testing"}
              >
                {testState === "testing" ? "Testing…" : "Test connection"}
              </button>
              {statusLine && (
                <div
                  className={`locaryn-conn ${
                    testState === "error"
                      ? "locaryn-conn-error"
                      : testState === "tested"
                        ? "locaryn-conn-ok"
                        : "locaryn-conn-testing"
                  }`}
                >
                  <span className="locaryn-conn-dot" />
                  {statusLine}
                </div>
              )}
            </div>

            {probe && (
              <div className="locaryn-probe">
                <div className="locaryn-probe-caps">
                  <span>{probe.os ?? "unknown OS"}</span>
                  <span>read: {probe.can_read ? "yes" : "no"}</span>
                  <span>write: {probe.can_write ? "yes" : "no"}</span>
                  <span>sudo: {probe.is_sudoer ? "yes" : "no"}</span>
                </div>
                <div className="locaryn-hostkey">
                  <div className="locaryn-hostkey-label">
                    Host key ({probe.host_key.algo}) — verify out of band:
                  </div>
                  <code className="locaryn-hostkey-fp">{probe.host_key.sha256}</code>
                  {hostKeyConfirmed ? (
                    <span className="locaryn-hostkey-ok">
                      <Icon name="check" size={15} /> confirmed
                    </span>
                  ) : (
                    <button type="button" className="locaryn-btn-ghost" onClick={confirmKey}>
                      Confirm fingerprint
                    </button>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>

        <div className="locaryn-form-footer">
          <button type="button" className="locaryn-btn-ghost" onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className="locaryn-btn-primary"
            onClick={save}
            disabled={!canSave}
            title={
              !probe
                ? "Test the connection first"
                : !hostKeyConfirmed
                  ? "Confirm the host key first"
                  : ""
            }
          >
            {saving ? "Saving…" : "Save server"}
          </button>
        </div>
      </div>
    </>
  );
}
