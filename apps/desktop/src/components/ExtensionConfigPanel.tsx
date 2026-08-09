import { useCallback, useEffect, useMemo, useState } from "react";
import {
  type ExtensionConfig,
  type ExtensionField,
  type ExtensionMcpServer,
  type InstalledExtension,
  core,
} from "../lib/core";

/**
 * Le formulaire de réglages d'une extension.
 *
 * Ce composant ne connaît aucune extension en particulier : il reçoit un
 * schéma déclaré par le manifeste et le dessine. C'est ce qui permet à une
 * extension d'ajouter ses propres réglages sans qu'une ligne soit écrite ici,
 * et à l'application de n'en garder aucune trace quand l'extension est
 * désinstallée — les valeurs vivent dans le dossier du plugin, pas ici.
 */

type Props = {
  extension: InstalledExtension;
  onClose: () => void;
};

const GENERAL = "Général";

function fieldLabel(key: string, field: ExtensionField): string {
  return field.title ?? key;
}

/** Une variable d'environnement en cours d'édition.
 *
 *  L'identité est portée par la ligne, pas par sa position : « Retirer »
 *  supprime au milieu de la liste, et une clé fondée sur l'index faisait
 *  réutiliser les champs de saisie de la mauvaise ligne — les valeurs
 *  remontaient d'un cran sous les yeux de l'utilisateur. Le nom de la variable
 *  ne peut pas servir de clé non plus : il est vide tant qu'on n'a rien tapé. */
type EnvRow = { id: string; key: string; value: string };

let envSeq = 0;
function nextEnvId(): string {
  envSeq += 1;
  return `env-${envSeq}`;
}

export function ExtensionConfigPanel({ extension, onClose }: Props) {
  const [config, setConfig] = useState<ExtensionConfig | null>(null);
  const [draft, setDraft] = useState<Record<string, unknown>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  // Les serveurs MCP déclarés par l'extension : env + auto-start éditables,
  // à côté du formulaire de schéma. La commande/URL et le transport viennent
  // du fichier du plugin et ne sont pas modifiables ici.
  const [mcpServers, setMcpServers] = useState<ExtensionMcpServer[] | null>(null);
  const [mcpDrafts, setMcpDrafts] = useState<
    Record<string, { env: EnvRow[]; auto_start: boolean }>
  >({});
  const [mcpBusy, setMcpBusy] = useState(false);
  const [mcpError, setMcpError] = useState<string | null>(null);
  const [mcpSaved, setMcpSaved] = useState(false);

  const load = useCallback(async () => {
    try {
      const cfg = await core.getExtensionConfig(extension.id);
      setConfig(cfg);
      setDraft({ ...cfg.values });
    } catch (e) {
      setError(String(e));
    }
    // Non-fatal : le formulaire de schéma fonctionne même si la lecture des
    // serveurs MCP échoue (extension sans serveur, fichier illisible…).
    try {
      const servers = await core.getExtensionMcpServers(extension.id);
      setMcpServers(servers);
      setMcpDrafts(
        Object.fromEntries(
          servers.map((s) => [
            s.name,
            {
              env: Object.entries(s.env).map(([key, value]) => ({
                id: nextEnvId(),
                key,
                value,
              })),
              auto_start: s.auto_start,
            },
          ]),
        ),
      );
    } catch {
      setMcpServers([]);
    }
  }, [extension.id]);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const dirty = useMemo(() => {
    if (!config) return false;
    return Object.keys(draft).some(
      (k) => JSON.stringify(draft[k]) !== JSON.stringify(config.values[k]),
    );
  }, [draft, config]);

  const groups = useMemo(() => {
    if (!config?.schema) return [];
    const map = new Map<string, [string, ExtensionField][]>();
    for (const [key, field] of Object.entries(config.schema)) {
      const g = field.group?.trim() || GENERAL;
      const list = map.get(g) ?? [];
      list.push([key, field]);
      map.set(g, list);
    }
    return [...map.entries()];
  }, [config]);

  async function save() {
    setBusy(true);
    setError(null);
    try {
      const next = await core.setExtensionConfig(extension.id, draft);
      setConfig(next);
      setDraft({ ...next.values });
      setSaved(true);
      setTimeout(() => setSaved(false), 2500);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function set(key: string, value: unknown) {
    setDraft((prev) => ({ ...prev, [key]: value }));
  }

  // --- Serveurs MCP : éditeur de variables d'environnement + auto-start -----

  function setEnvRow(server: string, index: number, key: string, value: string) {
    setMcpDrafts((prev) => {
      const rows = [...(prev[server]?.env ?? [])];
      rows[index] = { ...rows[index], key, value };
      return {
        ...prev,
        [server]: { env: rows, auto_start: prev[server]?.auto_start ?? false },
      };
    });
  }

  function addEnvRow(server: string) {
    setMcpDrafts((prev) => ({
      ...prev,
      [server]: {
        env: [...(prev[server]?.env ?? []), { id: nextEnvId(), key: "", value: "" }],
        auto_start: prev[server]?.auto_start ?? false,
      },
    }));
  }

  function removeEnvRow(server: string, index: number) {
    setMcpDrafts((prev) => {
      const rows = [...(prev[server]?.env ?? [])];
      rows.splice(index, 1);
      return {
        ...prev,
        [server]: { env: rows, auto_start: prev[server]?.auto_start ?? false },
      };
    });
  }

  function setAutoStart(server: string, value: boolean) {
    setMcpDrafts((prev) => ({
      ...prev,
      [server]: { env: prev[server]?.env ?? [], auto_start: value },
    }));
  }

  async function saveMcp() {
    if (!mcpServers) return;
    setMcpBusy(true);
    setMcpError(null);
    try {
      const servers: ExtensionMcpServer[] = mcpServers.map((s) => {
        const d = mcpDrafts[s.name] ?? { env: [], auto_start: s.auto_start };
        const env: Record<string, string> = {};
        for (const row of d.env) {
          if (row.key.trim()) env[row.key.trim()] = row.value;
        }
        return {
          name: s.name,
          transport: s.transport,
          target: s.target,
          env,
          auto_start: d.auto_start,
        };
      });
      const next = await core.setExtensionMcpServers(extension.id, servers);
      setMcpServers(next);
      setMcpDrafts(
        Object.fromEntries(
          next.map((s) => [
            s.name,
            {
              env: Object.entries(s.env).map(([key, value]) => ({
                id: nextEnvId(),
                key,
                value,
              })),
              auto_start: s.auto_start,
            },
          ]),
        ),
      );
      setMcpSaved(true);
      setTimeout(() => setMcpSaved(false), 2500);
    } catch (e) {
      setMcpError(String(e));
    } finally {
      setMcpBusy(false);
    }
  }

  function renderField(key: string, field: ExtensionField) {
    const value = draft[key];
    const id = `ext-cfg-${key}`;

    switch (field.type) {
      case "boolean":
        return (
          <label
            htmlFor={id}
            style={{ display: "flex", gap: 10, alignItems: "flex-start", cursor: "pointer" }}
          >
            <input
              id={id}
              type="checkbox"
              checked={value === true}
              onChange={(e) => set(key, e.target.checked)}
            />
            <span>
              <strong style={{ fontSize: 13 }}>{fieldLabel(key, field)}</strong>
              {field.description && (
                <span className="locaryn-field-hint" style={{ display: "block" }}>
                  {field.description}
                </span>
              )}
            </span>
          </label>
        );

      case "select":
        return (
          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor={id}>
              {fieldLabel(key, field)}
            </label>
            {field.description && <p className="locaryn-field-hint">{field.description}</p>}
            <select
              id={id}
              className="locaryn-select"
              value={String(value ?? "")}
              onChange={(e) => set(key, e.target.value)}
            >
              {(field.options ?? []).map((opt, i) => (
                <option key={opt} value={opt}>
                  {field.optionLabels?.[i] ?? opt}
                </option>
              ))}
            </select>
          </div>
        );

      case "number":
        return (
          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor={id}>
              {fieldLabel(key, field)}
            </label>
            {field.description && <p className="locaryn-field-hint">{field.description}</p>}
            <input
              id={id}
              type="number"
              className="locaryn-input"
              value={Number(value ?? 0)}
              min={field.min}
              max={field.max}
              step={field.step ?? 1}
              onChange={(e) => set(key, Number(e.target.value))}
            />
          </div>
        );

      case "text":
        return (
          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor={id}>
              {fieldLabel(key, field)}
            </label>
            {field.description && <p className="locaryn-field-hint">{field.description}</p>}
            <textarea
              id={id}
              className="locaryn-input"
              rows={4}
              style={{ resize: "vertical", fontFamily: "inherit" }}
              value={String(value ?? "")}
              onChange={(e) => set(key, e.target.value)}
            />
          </div>
        );

      case "list": {
        const asText = Array.isArray(value) ? (value as string[]).join("\n") : String(value ?? "");
        return (
          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor={id}>
              {fieldLabel(key, field)}
            </label>
            <p className="locaryn-field-hint">
              {field.description ? `${field.description} — une par ligne.` : "Une par ligne."}
            </p>
            <textarea
              id={id}
              className="locaryn-input"
              rows={3}
              style={{ resize: "vertical", fontFamily: "inherit" }}
              value={asText}
              onChange={(e) =>
                set(
                  key,
                  e.target.value
                    .split("\n")
                    .map((s) => s.trim())
                    .filter(Boolean),
                )
              }
            />
          </div>
        );
      }

      case "secret":
        return (
          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor={id}>
              {fieldLabel(key, field)}
            </label>
            {field.description && <p className="locaryn-field-hint">{field.description}</p>}
            <input
              id={id}
              type="password"
              className="locaryn-input"
              autoComplete="off"
              placeholder="••••••"
              value={String(value ?? "")}
              onChange={(e) => set(key, e.target.value)}
            />
          </div>
        );

      case "path":
        return (
          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor={id}>
              {fieldLabel(key, field)}
            </label>
            {field.description && <p className="locaryn-field-hint">{field.description}</p>}
            <div style={{ display: "flex", gap: 8 }}>
              <input
                id={id}
                className="locaryn-input"
                style={{ flex: 1 }}
                value={String(value ?? "")}
                onChange={(e) => set(key, e.target.value)}
              />
              <button
                type="button"
                className="locaryn-btn-ghost"
                onClick={() =>
                  core
                    .pickVoiceReference?.()
                    .then((p) => p && set(key, p))
                    .catch(() => undefined)
                }
              >
                Parcourir
              </button>
            </div>
          </div>
        );

      default:
        return (
          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor={id}>
              {fieldLabel(key, field)}
            </label>
            {field.description && <p className="locaryn-field-hint">{field.description}</p>}
            <input
              id={id}
              className="locaryn-input"
              value={String(value ?? "")}
              onChange={(e) => set(key, e.target.value)}
            />
          </div>
        );
    }
  }

  const hasSchema = !!config?.schema && Object.keys(config.schema).length > 0;

  return (
    <div className="locaryn-settings-backdrop">
      <button
        type="button"
        className="locaryn-backdrop-dismiss"
        aria-label="Fermer"
        onClick={onClose}
      />
      <dialog
        open
        className="locaryn-card locaryn-modal-card"
        aria-modal="true"
        aria-label={`Réglages de ${extension.name}`}
        style={{
          width: "min(680px, 92vw)",
          maxHeight: "82vh",
          overflowY: "auto",
          margin: "60px auto",
          padding: 20,
        }}
      >
        <h3 style={{ marginBottom: 2 }}>{extension.name}</h3>
        <p className="locaryn-field-hint" style={{ marginBottom: 16 }}>
          v{extension.version}
          {extension.author ? ` · ${extension.author}` : ""} — réglages fournis par l'extension
          elle-même.
        </p>

        {error && (
          <p className="locaryn-field-hint" style={{ color: "var(--danger)", marginBottom: 12 }}>
            {error}
          </p>
        )}

        {!config ? (
          <p className="locaryn-field-hint">Chargement…</p>
        ) : !hasSchema ? (
          <p className="locaryn-field-hint">
            Cette extension ne déclare aucun réglage. Rien à configurer ici.
          </p>
        ) : (
          groups.map(([group, fields]) => (
            <section key={group} style={{ marginBottom: 22 }}>
              <h4
                style={{
                  fontSize: "var(--text-md)",
                  marginBottom: 10,
                  paddingBottom: 6,
                  borderBottom: "1px solid var(--border)",
                }}
              >
                {group}
              </h4>
              <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
                {fields.map(([key, field]) => (
                  <div key={key}>{renderField(key, field)}</div>
                ))}
              </div>
            </section>
          ))
        )}

        {mcpServers && (
          <section style={{ marginBottom: 22, marginTop: 22 }}>
            <h4
              style={{
                fontSize: "var(--text-md)",
                marginBottom: 10,
                paddingBottom: 6,
                borderBottom: "1px solid var(--border)",
              }}
            >
              Serveurs MCP
            </h4>
            <p className="locaryn-field-hint" style={{ marginBottom: 12 }}>
              Variables d'environnement et démarrage automatique de chaque serveur déclaré par
              l'extension. La commande, l'URL et le transport viennent du fichier du plugin et ne
              sont pas modifiables ici.
            </p>
            {mcpError && (
              <p
                className="locaryn-field-hint"
                style={{ color: "var(--danger)", marginBottom: 12 }}
              >
                {mcpError}
              </p>
            )}
            {mcpServers.length === 0 ? (
              <p className="locaryn-field-hint">
                Cette extension ne déclare aucun serveur MCP modifiable.
              </p>
            ) : (
              mcpServers.map((s) => {
                const draft = mcpDrafts[s.name] ?? { env: [], auto_start: false };
                return (
                  <div
                    key={s.name}
                    className="locaryn-card"
                    style={{ padding: 12, marginBottom: 10 }}
                  >
                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 8,
                        marginBottom: 8,
                        flexWrap: "wrap",
                      }}
                    >
                      <strong style={{ fontSize: 13 }}>{s.name}</strong>
                      <span className="locaryn-tag">{s.transport}</span>
                      <code
                        className="locaryn-connector-cmd"
                        style={{ flex: "1 1 200px", fontSize: 11 }}
                      >
                        {s.target}
                      </code>
                    </div>
                    <label
                      htmlFor={`mcp-auto-${s.name}`}
                      style={{
                        display: "flex",
                        gap: 10,
                        alignItems: "center",
                        cursor: "pointer",
                        marginBottom: 10,
                      }}
                    >
                      <input
                        id={`mcp-auto-${s.name}`}
                        type="checkbox"
                        checked={draft.auto_start}
                        onChange={(e) => setAutoStart(s.name, e.target.checked)}
                      />
                      <span style={{ fontSize: 13 }}>
                        Démarrer automatiquement avec l'application
                      </span>
                    </label>
                    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                      {draft.env.map((row, i) => (
                        <div key={row.id} style={{ display: "flex", gap: 6, alignItems: "center" }}>
                          <input
                            className="locaryn-input"
                            style={{ flex: "0 0 200px", fontFamily: "monospace" }}
                            placeholder="NOM_VARIABLE"
                            value={row.key}
                            onChange={(e) => setEnvRow(s.name, i, e.target.value, row.value)}
                          />
                          <input
                            className="locaryn-input"
                            style={{ flex: 1, fontFamily: "monospace" }}
                            placeholder="valeur"
                            value={row.value}
                            onChange={(e) => setEnvRow(s.name, i, row.key, e.target.value)}
                          />
                          <button
                            type="button"
                            className="locaryn-btn-ghost"
                            style={{ fontSize: 12, color: "var(--danger)" }}
                            onClick={() => removeEnvRow(s.name, i)}
                          >
                            Retirer
                          </button>
                        </div>
                      ))}
                      <div>
                        <button
                          type="button"
                          className="locaryn-btn-ghost"
                          style={{ fontSize: 12 }}
                          onClick={() => addEnvRow(s.name)}
                        >
                          + Ajouter une variable
                        </button>
                      </div>
                    </div>
                  </div>
                );
              })
            )}
            {mcpServers.length > 0 && (
              <div
                style={{
                  display: "flex",
                  gap: 8,
                  justifyContent: "flex-end",
                  alignItems: "center",
                  marginTop: 10,
                }}
              >
                <span className="locaryn-field-hint">{mcpSaved ? "Enregistré." : ""}</span>
                <button
                  type="button"
                  className="locaryn-btn-primary"
                  disabled={mcpBusy}
                  onClick={saveMcp}
                >
                  {mcpBusy ? "…" : "Enregistrer les serveurs MCP"}
                </button>
              </div>
            )}
          </section>
        )}

        <div
          style={{
            display: "flex",
            gap: 8,
            justifyContent: "space-between",
            alignItems: "center",
            marginTop: 8,
            paddingTop: 14,
            borderTop: "1px solid var(--border)",
          }}
        >
          <span className="locaryn-field-hint">
            {saved ? "Enregistré." : dirty ? "Modifications non enregistrées." : ""}
          </span>
          <div style={{ display: "flex", gap: 8 }}>
            <button type="button" className="locaryn-btn-ghost" onClick={onClose}>
              Fermer
            </button>
            {hasSchema && (
              <button
                type="button"
                className="locaryn-btn-primary"
                disabled={busy || !dirty}
                onClick={save}
              >
                {busy ? "…" : "Enregistrer"}
              </button>
            )}
          </div>
        </div>
      </dialog>
    </div>
  );
}
