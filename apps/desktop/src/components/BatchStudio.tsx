import { Icon } from "@locaryn/ui-core";
import { useMemo, useState } from "react";

export interface BatchJob {
  id: string;
  name: string;
  provider:
    | "OpenAI Batch (-50%)"
    | "Anthropic Batch (-50%)"
    | "DeepSeek Batch (-50%)"
    | "Ollama Local Batch";
  totalRequests: number;
  completedRequests: number;
  status: "queued" | "validating" | "in_progress" | "completed" | "failed";
  tokensSaved: number;
  costSavedUsd: number;
  createdAt: string;
}

const DEMO_JOBS: BatchJob[] = [
  {
    id: "batch-9841",
    name: "Génération de tests unitaires (42 fichiers)",
    provider: "DeepSeek Batch (-50%)",
    totalRequests: 42,
    completedRequests: 42,
    status: "completed",
    tokensSaved: 145000,
    costSavedUsd: 0.29,
    createdAt: "2026-07-20 18:40",
  },
  {
    id: "batch-9842",
    name: "Documentation automatique TypeScript",
    provider: "OpenAI Batch (-50%)",
    totalRequests: 120,
    completedRequests: 85,
    status: "in_progress",
    tokensSaved: 420000,
    costSavedUsd: 1.25,
    createdAt: "2026-07-20 20:15",
  },
];

export function BatchStudio() {
  const [jobs, setJobs] = useState<BatchJob[]>(DEMO_JOBS);
  const [newJobName, setNewJobName] = useState("");
  const [selectedProvider, setSelectedProvider] =
    useState<BatchJob["provider"]>("DeepSeek Batch (-50%)");
  const [batchFileText, setBatchFileText] = useState(
    '{"custom_id": "req-1", "method": "POST", "url": "/v1/chat/completions", "body": {"model": "deepseek-reasoner", "messages": [{"role": "user", "content": "Refactor src/lib/core.ts"}]}}\n{"custom_id": "req-2", "method": "POST", "url": "/v1/chat/completions", "body": {"model": "deepseek-reasoner", "messages": [{"role": "user", "content": "Generates unit tests"}]}}',
  );

  const totalSavedTokens = useMemo(() => {
    return jobs.reduce((acc, j) => acc + j.tokensSaved, 0);
  }, [jobs]);

  const totalSavedUsd = useMemo(() => {
    return jobs.reduce((acc, j) => acc + j.costSavedUsd, 0);
  }, [jobs]);

  function handleCreateBatch() {
    if (!newJobName.trim()) return;
    const lines = batchFileText
      .trim()
      .split("\n")
      .filter((l) => l.trim().length > 0);
    const count = Math.max(1, lines.length);

    const newJ: BatchJob = {
      id: `batch-${Math.floor(1000 + Math.random() * 9000)}`,
      name: newJobName.trim(),
      provider: selectedProvider,
      totalRequests: count,
      completedRequests: 0,
      status: "in_progress",
      tokensSaved: Math.round(count * 3500),
      costSavedUsd: Math.round(count * 0.012 * 100) / 100,
      createdAt: new Date().toISOString().slice(0, 16).replace("T", " "),
    };

    setJobs((prev) => [newJ, ...prev]);
    setNewJobName("");

    // Simulate progress
    setTimeout(() => {
      setJobs((prev) =>
        prev.map((j) =>
          j.id === newJ.id ? { ...j, completedRequests: Math.round(count * 0.5) } : j,
        ),
      );
    }, 2500);

    setTimeout(() => {
      setJobs((prev) =>
        prev.map((j) =>
          j.id === newJ.id ? { ...j, completedRequests: count, status: "completed" } : j,
        ),
      );
    }, 5000);
  }

  return (
    <div
      className="locaryn-view-container"
      style={{ padding: "var(--space-4)", overflowY: "auto" }}
    >
      <div className="locaryn-view-header">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
          <div>
            <h2>
              <Icon name="speed" size={15} /> Batch API Studio (-50% Coût des Jetons)
            </h2>
            <p className="locaryn-view-desc">
              Traitez vos gros volumes de prompts et d'analyse de codebase par lots asynchrones.
              Économisez 50% sur le tarif des tokens API (OpenAI, Anthropic, DeepSeek) et dépassez
              les limites de quota minute.
            </p>
          </div>

          <div
            style={{
              display: "flex",
              gap: "12px",
              background: "var(--surface)",
              padding: "8px 16px",
              borderRadius: "var(--radius)",
              border: "1px solid var(--border)",
            }}
          >
            <div>
              <span style={{ fontSize: "10px", color: "var(--text-faint)", display: "block" }}>
                ÉCONOMIE JETONS
              </span>
              <strong style={{ color: "#64c878", fontSize: "14px" }}>
                {totalSavedTokens.toLocaleString()} tokens
              </strong>
            </div>
            <div style={{ borderLeft: "1px solid var(--border)", paddingLeft: "12px" }}>
              <span style={{ fontSize: "10px", color: "var(--text-faint)", display: "block" }}>
                RÉDUCTION COÛT
              </span>
              <strong style={{ color: "var(--accent)", fontSize: "14px" }}>
                -${totalSavedUsd.toFixed(2)} USD (-50%)
              </strong>
            </div>
          </div>
        </div>
      </div>

      {/* New Batch Creation Form */}
      <div className="locaryn-box-card" style={{ marginTop: "16px", padding: "16px" }}>
        <h3 style={{ fontSize: "14px", fontWeight: 700, marginBottom: "8px" }}>
          ➕ Créer un nouveau Lot de Traitement (Batch JSONL)
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
            onChange={(e) => setSelectedProvider(e.target.value as BatchJob["provider"])}
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
          <span style={{ fontSize: "11px", color: "#64c878" }}>
            💡 Réduction de 50% appliquée automatiquement sur la facture de tokens.
          </span>
          <button
            type="button"
            className="locaryn-btn-primary"
            disabled={!newJobName.trim()}
            onClick={handleCreateBatch}
          >
            <Icon name="speed" size={15} /> Soumettre le Lot d'API (-50%)
          </button>
        </div>
      </div>

      {/* Existing Batch Jobs Table */}
      <div style={{ marginTop: "24px" }}>
        <h3 style={{ fontSize: "14px", fontWeight: 700, marginBottom: "12px" }}>
          📋 Historique des Lots de Traitement
        </h3>

        <div className="locaryn-model-list">
          {jobs.map((j) => {
            const pct = Math.round((j.completedRequests / j.totalRequests) * 100);
            return (
              <div
                key={j.id}
                className="locaryn-box-card"
                style={{ padding: "12px 16px", marginBottom: "8px" }}
              >
                <div
                  style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}
                >
                  <div>
                    <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
                      <strong style={{ fontSize: "13px" }}>{j.name}</strong>
                      <span
                        className="locaryn-tag"
                        style={{ background: "rgba(100, 200, 120, 0.15)", color: "#64c878" }}
                      >
                        {j.provider}
                      </span>
                      <span className="locaryn-tag locaryn-tag-soft">{j.createdAt}</span>
                    </div>
                    <span
                      style={{
                        fontSize: "11px",
                        color: "var(--text-faint)",
                        marginTop: "4px",
                        display: "block",
                      }}
                    >
                      ID: {j.id} — {j.completedRequests} / {j.totalRequests} requêtes • Economie : ~
                      {j.tokensSaved.toLocaleString()} tokens (${j.costSavedUsd.toFixed(2)})
                    </span>
                  </div>

                  <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
                    {j.status === "completed" ? (
                      <span className="locaryn-tag locaryn-tag-installed">Terminé ✓</span>
                    ) : (
                      <span
                        className="locaryn-tag"
                        style={{ background: "rgba(100, 150, 255, 0.2)", color: "var(--accent)" }}
                      >
                        En cours ({pct}%)
                      </span>
                    )}

                    {j.status === "completed" && (
                      <button
                        type="button"
                        className="locaryn-btn-ghost"
                        style={{ fontSize: "11px", padding: "4px 8px" }}
                        onClick={() => alert(`Téléchargement des résultats JSONL pour ${j.id}`)}
                      >
                        ⬇️ Télécharger Résultats
                      </button>
                    )}
                  </div>
                </div>

                {j.status === "in_progress" && (
                  <div
                    className="locaryn-footer-progress-track"
                    style={{ width: "100%", marginTop: "8px", height: "4px" }}
                  >
                    <div className="locaryn-footer-progress-fill" style={{ width: `${pct}%` }} />
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
