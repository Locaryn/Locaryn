import { useState } from "react";
import { type ImportMcpResult, type McpJsonPreview, core } from "../lib/core";

const EXEMPLE = `{
  "mcpServers": {
    "mon-connecteur": { "url": "https://exemple.com/mcp" }
  }
}`;

type Props = {
  onDone: (result: ImportMcpResult) => void;
  onCancel: () => void;
};

function messageOf(e: unknown): string {
  return String(e).replace(/^Error:\s*/, "");
}

/**
 * Ajouter des serveurs MCP en collant le bloc `mcpServers` des instructions d'un
 * serveur.
 *
 * Deux temps, parce qu'un bloc copié depuis une page web contient une commande
 * qui s'exécutera sur cette machine : on la relit telle qu'elle sera lancée, et
 * rien n'est enregistré ni démarré avant l'accord explicite de la personne.
 */
export function McpJsonImport({ onDone, onCancel }: Props) {
  const [text, setText] = useState("");
  const [name, setName] = useState("");
  const [review, setReview] = useState<McpJsonPreview[] | null>(null);
  const [start, setStart] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function verify() {
    setBusy(true);
    setError(null);
    try {
      setReview(await core.previewMcpJson(text, name));
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }

  async function approve() {
    setBusy(true);
    setError(null);
    try {
      onDone(await core.importMcpJson(text, name, start));
    } catch (e) {
      setError(messageOf(e));
      setBusy(false);
    }
  }

  if (review) {
    const blocked = review.some((r) => r.exists);
    return (
      <div className="locaryn-mcp-json">
        {error && <div className="locaryn-vp-error">{error}</div>}
        <p className="locaryn-field-hint">
          {review.length > 1
            ? `${review.length} serveurs seront ajoutés.`
            : "1 serveur sera ajouté."}{" "}
          Relisez ce qui sera lancé sur cet ordinateur : rien ne tourne avant votre accord.
        </p>
        <ul className="locaryn-mcp-review">
          {review.map((r) => (
            <li key={r.name} className="locaryn-mcp-review-item">
              <div className="locaryn-mcp-review-head">
                <strong>{r.name}</strong>
                <span className="locaryn-box-brand">
                  {r.transport === "stdio" ? "commande locale" : "adresse HTTP"}
                </span>
                {r.exists && <span className="locaryn-mcp-exists">existe déjà</span>}
              </div>
              <code className="locaryn-connector-cmd">{r.target}</code>
              {(r.env_keys.length > 0 || r.header_keys.length > 0) && (
                <p className="locaryn-field-hint">
                  {r.env_keys.length > 0 && `Variables : ${r.env_keys.join(", ")}. `}
                  {r.header_keys.length > 0 && `En-têtes : ${r.header_keys.join(", ")}.`}
                </p>
              )}
            </li>
          ))}
        </ul>
        <label className="locaryn-mcp-start">
          <input type="checkbox" checked={start} onChange={(e) => setStart(e.target.checked)} />
          <span>Démarrer maintenant après l'ajout</span>
        </label>
        {blocked && (
          <p className="locaryn-field-hint">
            Un serveur porte déjà ce nom : retirez-le d'abord, ou renommez-le dans le JSON.
          </p>
        )}
        <div className="locaryn-mcp-actions">
          <button type="button" className="locaryn-btn-ghost" onClick={() => setReview(null)}>
            Retour
          </button>
          <button
            type="button"
            className="locaryn-btn-primary"
            disabled={busy || blocked}
            onClick={() => void approve()}
          >
            {busy ? "Ajout…" : "Approuver et ajouter"}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="locaryn-mcp-json">
      {error && <div className="locaryn-vp-error">{error}</div>}
      <p className="locaryn-field-hint">
        Collez le bloc <code>mcpServers</code> des instructions du serveur. Il est enregistré dans{" "}
        <code>~/.locaryn/mcp.json</code>. Rien ne tourne avant que vous l'ayez relu et approuvé.
      </p>
      <textarea
        className="locaryn-textarea locaryn-mcp-json-input"
        aria-label="JSON des serveurs MCP"
        placeholder={EXEMPLE}
        value={text}
        spellCheck={false}
        onChange={(e) => setText(e.target.value)}
      />
      <input
        className="locaryn-input"
        aria-label="Nom du serveur"
        placeholder="Nom (seulement si le JSON n'en contient pas)"
        value={name}
        onChange={(e) => setName(e.target.value)}
      />
      <div className="locaryn-mcp-actions">
        <button type="button" className="locaryn-btn-ghost" onClick={onCancel}>
          Annuler
        </button>
        <button
          type="button"
          className="locaryn-btn-primary"
          disabled={busy || !text.trim()}
          onClick={() => void verify()}
        >
          {busy ? "Lecture…" : "Vérifier"}
        </button>
      </div>
    </div>
  );
}
