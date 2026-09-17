import { Icon } from "@locaryn/ui-core";
import { useState } from "react";

type BatchProvider =
  | "OpenAI Batch (-50%)"
  | "Anthropic Batch (-50%)"
  | "DeepSeek Batch (-50%)"
  | "Ollama Local Batch";

/**
 * Traitement par lots.
 *
 * Aucune soumission réelle vers une API Batch n'est câblée ici — ni appel à
 * `/v1/batches` (OpenAI, Anthropic, DeepSeek), ni file d'attente locale vers
 * le moteur d'inférence. Un historique préchargé de lots jamais soumis, avec
 * des économies et une progression fabriquées, laissait croire à un
 * traitement déjà en cours dès l'installation de l'extension qui déclenche
 * cette vue (`text-analysis`, morph-text-analysis) — Locaryn/Locaryn#1
 * (déposée sur ce dépôt-là, la capacité qu'il déclare étant ce qui fait
 * apparaître cette vue native). L'échec est maintenant explicite.
 */
export function BatchStudio() {
  const [newJobName, setNewJobName] = useState("");
  const [selectedProvider, setSelectedProvider] = useState<BatchProvider>("DeepSeek Batch (-50%)");
  const [batchFileText, setBatchFileText] = useState(
    '{"custom_id": "req-1", "method": "POST", "url": "/v1/chat/completions", "body": {"model": "deepseek-reasoner", "messages": [{"role": "user", "content": "Refactor src/lib/core.ts"}]}}\n{"custom_id": "req-2", "method": "POST", "url": "/v1/chat/completions", "body": {"model": "deepseek-reasoner", "messages": [{"role": "user", "content": "Generates unit tests"}]}}',
  );
  const [submitError, setSubmitError] = useState<string | null>(null);

  function handleCreateBatch() {
    if (!newJobName.trim()) return;
    setSubmitError(
      `La soumission réelle vers ${selectedProvider} n'est pas encore implémentée. Aucune requête n'a été envoyée, aucun lot n'a été créé.`,
    );
  }

  return (
    <div
      className="locaryn-view-container"
      style={{ padding: "var(--space-4)", overflowY: "auto" }}
    >
      <div className="locaryn-view-header">
        <h2>
          <Icon name="speed" size={15} /> Batch API Studio
        </h2>
        <p className="locaryn-view-desc">
          Prépare un fichier de requêtes au format JSONL pour un traitement par lots. La soumission
          réelle vers un fournisseur (OpenAI, Anthropic, DeepSeek) ou vers le moteur local n'est pas
          encore implémentée.
        </p>
      </div>

      {/* New Batch Creation Form */}
      <div className="locaryn-box-card" style={{ marginTop: "16px", padding: "16px" }}>
        <h3 style={{ fontSize: "14px", fontWeight: 700, marginBottom: "8px" }}>
          Préparer un lot de traitement (JSONL)
        </h3>

        <div
          style={{
            display: "grid",
            gridTemplateColumns: "1fr 220px",
            gap: "12px",
            marginBottom: "12px",
          }}
        >
          <input
            className="locaryn-input"
            placeholder="Nom du lot (ex: Indexation de 80 fichiers / Audit de sécurité)..."
            value={newJobName}
            onChange={(e) => setNewJobName(e.target.value)}
          />

          <select
            className="locaryn-select"
            value={selectedProvider}
            onChange={(e) => setSelectedProvider(e.target.value as BatchProvider)}
          >
            <option value="DeepSeek Batch (-50%)">DeepSeek Batch (-50%)</option>
            <option value="OpenAI Batch (-50%)">OpenAI Batch (-50%)</option>
            <option value="Anthropic Batch (-50%)">Anthropic Batch (-50%)</option>
            <option value="Ollama Local Batch">File d'attente locale (llama-server)</option>
          </select>
        </div>

        <div style={{ marginBottom: "12px" }}>
          <div
            style={{
              fontSize: "11px",
              color: "var(--text-faint)",
              marginBottom: "4px",
              display: "block",
            }}
          >
            Fichier Batch au format JSONL (1 requête par ligne avec custom_id) :
          </div>
          <textarea
            className="locaryn-input"
            rows={4}
            style={{
              fontFamily: "var(--font-mono)",
              fontSize: "11px",
              width: "100%",
              resize: "vertical",
            }}
            value={batchFileText}
            onChange={(e) => setBatchFileText(e.target.value)}
          />
        </div>

        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", color: "var(--text-faint)" }}>
            La soumission réelle vers un fournisseur n'est pas encore implémentée.
          </span>
          <button
            type="button"
            className="locaryn-btn-primary"
            disabled={!newJobName.trim()}
            onClick={handleCreateBatch}
          >
            <Icon name="speed" size={15} /> Soumettre le lot
          </button>
        </div>

        {submitError && (
          <div className="locaryn-store-error" style={{ marginTop: "12px" }} role="alert">
            {submitError}
          </div>
        )}
      </div>

      {/* Batch job history — vide tant que rien n'est réellement soumis. */}
      <div style={{ marginTop: "24px" }}>
        <h3 style={{ fontSize: "14px", fontWeight: 700, marginBottom: "12px" }}>
          Historique des lots
        </h3>
        <p className="locaryn-field-hint">
          Aucun lot pour l'instant — aucune soumission réelle n'est encore câblée.
        </p>
      </div>
    </div>
  );
}
