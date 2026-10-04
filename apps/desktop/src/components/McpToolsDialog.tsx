import { useCallback, useEffect, useMemo, useState } from "react";
import { type McpServerInfo, type McpToolInfo, core } from "../lib/core";
import { ModalShell } from "./ModalShell";

type Props = {
  server: McpServerInfo;
  onClose: () => void;
  /** Après un démarrage ou un changement : l'écran parent rafraîchit sa liste. */
  onChanged: () => void;
};

function messageOf(e: unknown): string {
  return String(e).replace(/^Error:\s*/, "");
}

/**
 * Les outils qu'un connecteur MCP offre au modèle, avec une case par outil.
 *
 * Décocher un outil l'interdit au modèle : il ne lui est pas proposé et un appel
 * qui le viserait quand même est refusé. C'est le moyen de garder un connecteur
 * utile (lire, créer) tout en fermant ce qui détruit (tout effacer, écraser).
 * Le choix s'enregistre à chaque case, sans bouton de validation.
 */
export function McpToolsDialog({ server, onClose, onChanged }: Props) {
  const [tools, setTools] = useState<McpToolInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [filter, setFilter] = useState("");

  const load = useCallback(async () => {
    setError(null);
    try {
      setTools(await core.listMcpTools(server.name));
    } catch (e) {
      setTools(null);
      setError(messageOf(e));
    }
  }, [server.name]);

  useEffect(() => {
    if (server.running) void load();
  }, [server.running, load]);

  async function startAndLoad() {
    setBusy(true);
    setError(null);
    try {
      await core.startMcpServer(server.name);
      onChanged();
      await load();
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }

  /** Enregistre la nouvelle liste ; en cas d'échec, remet l'état d'avant. */
  async function apply(next: McpToolInfo[]) {
    const previous = tools;
    setTools(next);
    try {
      await core.setMcpDisabledTools(
        server.name,
        next.filter((t) => !t.enabled).map((t) => t.name),
      );
      onChanged();
    } catch (e) {
      setTools(previous);
      setError(messageOf(e));
    }
  }

  const visible = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!tools) return [];
    if (!q) return tools;
    return tools.filter(
      (t) => t.name.toLowerCase().includes(q) || (t.description ?? "").toLowerCase().includes(q),
    );
  }, [tools, filter]);

  function toggle(name: string) {
    if (!tools) return;
    void apply(tools.map((t) => (t.name === name ? { ...t, enabled: !t.enabled } : t)));
  }

  function setAll(enabled: boolean) {
    if (!tools) return;
    const shown = new Set(visible.map((t) => t.name));
    void apply(tools.map((t) => (shown.has(t.name) ? { ...t, enabled } : t)));
  }

  const enabledCount = tools?.filter((t) => t.enabled).length ?? 0;

  return (
    <ModalShell label={`Outils de ${server.name}`} onClose={onClose} style={{ maxWidth: 680 }}>
      <div className="locaryn-mcp-tools">
        <div>
          <h3 className="locaryn-mcp-tools-title">Outils de « {server.name} »</h3>
          <p className="locaryn-field-hint">
            Le modèle ne voit et n'appelle que les outils cochés. Décochez ceux qu'il ne doit pas
            pouvoir utiliser, par exemple ceux qui suppriment ou écrasent.
          </p>
        </div>

        {error && <div className="locaryn-vp-error">{error}</div>}

        {!server.running && !tools && (
          <div className="locaryn-mcp-tools-empty">
            <p className="locaryn-field-hint">
              Ce connecteur est arrêté : la liste de ses outils vient de lui, il faut le démarrer
              pour la voir.
            </p>
            <button
              type="button"
              className="locaryn-btn-primary"
              disabled={busy}
              onClick={() => void startAndLoad()}
            >
              {busy ? "Démarrage…" : "Démarrer et afficher les outils"}
            </button>
          </div>
        )}

        {server.running && !tools && !error && (
          <p className="locaryn-field-hint">Lecture des outils…</p>
        )}

        {tools && (
          <>
            <div className="locaryn-mcp-tools-bar">
              <span className="locaryn-mcp-tools-count" aria-live="polite">
                {enabledCount} sur {tools.length} activés
              </span>
              <input
                className="locaryn-input locaryn-mcp-tools-search"
                aria-label="Filtrer les outils"
                placeholder="Filtrer…"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
              />
              <button type="button" className="locaryn-btn-ghost" onClick={() => setAll(true)}>
                Tout activer
              </button>
              <button type="button" className="locaryn-btn-ghost" onClick={() => setAll(false)}>
                Tout désactiver
              </button>
            </div>
            <ul className="locaryn-mcp-tools-list">
              {visible.map((t) => (
                <li key={t.name}>
                  <label className={`locaryn-mcp-tool${t.enabled ? "" : " locaryn-mcp-tool-off"}`}>
                    <input
                      type="checkbox"
                      checked={t.enabled}
                      onChange={() => toggle(t.name)}
                      aria-label={`Autoriser l'outil ${t.name}`}
                    />
                    <span className="locaryn-mcp-tool-body">
                      <span className="locaryn-mcp-tool-head">
                        <code className="locaryn-mcp-tool-name">{t.name}</code>
                        {t.destructive && <span className="locaryn-mcp-tool-flag">destructif</span>}
                        {t.read_only && (
                          <span className="locaryn-mcp-tool-flag locaryn-mcp-tool-flag-ro">
                            lecture seule
                          </span>
                        )}
                      </span>
                      {t.description && (
                        <span className="locaryn-mcp-tool-desc">{t.description}</span>
                      )}
                    </span>
                  </label>
                </li>
              ))}
              {visible.length === 0 && (
                <li className="locaryn-field-hint">Aucun outil ne correspond.</li>
              )}
            </ul>
          </>
        )}

        <div className="locaryn-mcp-actions">
          <button type="button" className="locaryn-btn-primary" onClick={onClose}>
            Fermer
          </button>
        </div>
      </div>
    </ModalShell>
  );
}
