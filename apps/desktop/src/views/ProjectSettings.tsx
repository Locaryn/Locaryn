import { useEffect, useState } from "react";
import { core, type Project, type RagStatus, type TrustLevel } from "../lib/core";

type Props = {
  projects: Project[];
  /** Called after a project is archived so the caller can refresh its list. */
  onArchived?: (p: Project) => void;
};

const TRUST_LABELS: Record<TrustLevel, { label: string; hint: string; color: string }> = {
  trusted: {
    label: "Confiance",
    hint: "Les outils peu et moyennement risqués s'exécutent sans confirmation.",
    color: "#5aa86a",
  },
  untrusted: {
    label: "Prudent",
    hint: "Seuls les outils en lecture s'exécutent sans confirmation.",
    color: "#d4a03a",
  },
  sandbox: {
    label: "Bac à sable",
    hint: "Chaque outil demande une confirmation explicite.",
    color: "#cc7d72",
  },
};

/**
 * Per-project settings: what the agent is allowed to do in this project, its
 * knowledge base, and lifecycle actions. Rendered inside the general settings
 * view so everything about a project lives in one place.
 */
export function ProjectSettings({ projects, onArchived }: Props) {
  const [selectedId, setSelectedId] = useState<string | null>(projects[0]?.id ?? null);
  const [rag, setRag] = useState<RagStatus | null>(null);
  const [busy, setBusy] = useState(false);

  const selected = projects.find((p) => p.id === selectedId) ?? null;

  useEffect(() => {
    if (!selected) {
      setRag(null);
      return;
    }
    let cancelled = false;
    core.ragStatus(selected.id)
      .then((s) => { if (!cancelled) setRag(s); })
      .catch(() => { if (!cancelled) setRag(null); });
    return () => { cancelled = true; };
  }, [selected]);

  async function archive() {
    if (!selected) return;
    const ok = window.confirm(
      `Archiver « ${selected.name} » ?\n\nLe projet disparaît de la liste. Ses conversations restent sur le disque.`,
    );
    if (!ok) return;
    setBusy(true);
    try {
      await core.archiveProject(selected.id);
      onArchived?.(selected);
      setSelectedId(projects.find((p) => p.id !== selected.id)?.id ?? null);
    } catch (e) {
      window.alert(`Archivage impossible : ${String(e).replace(/^Error:\s*/, "")}`);
    } finally {
      setBusy(false);
    }
  }

  if (projects.length === 0) {
    return (
      <div className="lochor-field">
        <p className="lochor-field-hint">
          Aucun projet. Ajoutez-en un depuis la barre latérale pour configurer ses autorisations.
        </p>
      </div>
    );
  }

  return (
    <div className="lochor-projset">
      <div className="lochor-projset-list">
        {projects.map((p) => (
          <button
            key={p.id}
            type="button"
            className={`lochor-projset-item${p.id === selectedId ? " lochor-active" : ""}`}
            onClick={() => setSelectedId(p.id)}
            title={p.path}
          >
            📁 {p.name}
          </button>
        ))}
      </div>

      {selected && (
        <div className="lochor-projset-detail">
          <div className="lochor-field">
            <label className="lochor-field-label">{selected.name}</label>
            <p className="lochor-field-hint lochor-kv-mono">{selected.path}</p>
          </div>

          <div className="lochor-field" style={{ marginTop: 18 }}>
            <label className="lochor-field-label">Autorisations des outils</label>
            <p className="lochor-field-hint">
              Détermine ce que l'agent peut faire seul dans ce projet (lire, écrire, exécuter).
            </p>
            <div className="lochor-conn" style={{ marginTop: 8 }}>
              <span
                className="lochor-health-dot"
                style={{ background: TRUST_LABELS[selected.trust_level].color }}
              />
              <span>
                <strong>{TRUST_LABELS[selected.trust_level].label}</strong> —{" "}
                {TRUST_LABELS[selected.trust_level].hint}
              </span>
            </div>
            <p className="lochor-field-hint" style={{ marginTop: 8, fontStyle: "italic" }}>
              Le changement de niveau depuis l'interface arrive avec le journal d'approbations ;
              les demandes d'outils restent confirmées au cas par cas d'ici là.
            </p>
          </div>

          <div className="lochor-field" style={{ marginTop: 18 }}>
            <label className="lochor-field-label">Base de connaissances (RAG)</label>
            <p className="lochor-field-hint">
              Documents indexés utilisés automatiquement dans les conversations de ce projet.
            </p>
            <div className="lochor-conn" style={{ marginTop: 8 }}>
              <span className={`lochor-health-dot ${rag && rag.chunk_count > 0 ? "lochor-health-ok" : "lochor-health-off"}`} />
              <span>
                {rag == null
                  ? "…"
                  : rag.chunk_count > 0
                    ? `${rag.chunk_count} extraits · ${rag.sources.length} source(s)`
                    : "Aucun document indexé"}
              </span>
            </div>
            {rag && rag.chunk_count > 0 && (
              <div className="lochor-field-actions" style={{ marginTop: 10 }}>
                <button
                  type="button"
                  className="lochor-btn-ghost"
                  disabled={busy}
                  onClick={async () => {
                    if (!window.confirm("Effacer l'index de ce projet ?")) return;
                    setBusy(true);
                    try {
                      await core.ragClear(selected.id);
                      setRag(await core.ragStatus(selected.id));
                    } finally {
                      setBusy(false);
                    }
                  }}
                >
                  Effacer l'index
                </button>
              </div>
            )}
          </div>

          <div className="lochor-field" style={{ marginTop: 18 }}>
            <label className="lochor-field-label">Actions</label>
            <div className="lochor-field-actions" style={{ marginTop: 8 }}>
              <button
                type="button"
                className="lochor-btn-ghost"
                onClick={() => core.openModelsFolder(selected.path).catch(() => {})}
              >
                📂 Ouvrir le dossier
              </button>
              <button
                type="button"
                className="lochor-btn-ghost"
                style={{ color: "var(--danger)", borderColor: "var(--danger)" }}
                disabled={busy}
                onClick={archive}
              >
                🗄️ Archiver le projet
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
