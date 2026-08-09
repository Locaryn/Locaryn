import { useEffect, useRef, useState } from "react";
import { type RagHit, type RagStatus, core } from "../lib/core";

/** Text-ish files we can index directly (a PDF/DOCX would need extraction). */
const TEXT_EXT = [
  "txt",
  "md",
  "markdown",
  "rst",
  "csv",
  "tsv",
  "json",
  "yaml",
  "yml",
  "toml",
  "ini",
  "cfg",
  "log",
  "html",
  "htm",
  "xml",
  "js",
  "jsx",
  "ts",
  "tsx",
  "py",
  "rs",
  "go",
  "java",
  "c",
  "h",
  "cpp",
  "hpp",
  "cs",
  "rb",
  "php",
  "sh",
  "sql",
  "css",
  "scss",
  "vue",
  "svelte",
];

function isTextFile(name: string): boolean {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  return TEXT_EXT.includes(ext);
}

type Props = {
  projectId: string;
  onClose: () => void;
};

/**
 * Per-project knowledge base (RAG). Index text blocks, then every chat message
 * in this project automatically retrieves the most relevant chunks and feeds
 * them to the model. Embeddings run on a local `--embeddings` server.
 */
export function RagPanel({ projectId, onClose }: Props) {
  const [status, setStatus] = useState<RagStatus | null>(null);
  const [source, setSource] = useState("");
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<RagHit[] | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const [importing, setImporting] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  /** Index dropped/selected files directly — no copy-paste needed. */
  async function importFiles(files: FileList | File[]) {
    const list = Array.from(files);
    if (list.length === 0) return;
    setBusy(true);
    setError(null);
    const skipped: string[] = [];
    try {
      for (const f of list) {
        if (!isTextFile(f.name)) {
          skipped.push(f.name);
          continue;
        }
        setImporting(f.name);
        const text = await f.text();
        if (text.trim()) {
          const s = await core.ragIndexText(projectId, f.name, text);
          setStatus(s);
        }
      }
      if (skipped.length > 0) {
        setError(
          `Ignoré (format non lisible en texte) : ${skipped.join(", ")}. Copiez-collez leur contenu, ou convertissez-les en .txt/.md.`,
        );
      }
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setImporting(null);
      setBusy(false);
    }
  }

  useEffect(() => {
    let cancelled = false;
    core
      .ragStatus(projectId)
      .then((s) => {
        if (!cancelled) setStatus(s);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [projectId]);

  async function index() {
    const src = source.trim() || "note";
    if (!text.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const s = await core.ragIndexText(projectId, src, text);
      setStatus(s);
      setText("");
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }

  async function search() {
    if (!query.trim()) return;
    setBusy(true);
    setError(null);
    try {
      setHits(await core.ragSearch(projectId, query, 5));
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }

  async function clearAll() {
    setBusy(true);
    try {
      await core.ragClear(projectId);
      setStatus(await core.ragStatus(projectId));
      setHits(null);
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }

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
        className="locaryn-settings-modal"
        // biome-ignore lint/a11y/useSemanticElements: <dialog> apporte le padding, la marge auto et le positionnement par défaut de l'agent utilisateur, que .locaryn-settings-modal (fixe, centré par transform, taille imposée) ne réinitialise pas ; la modale est montée/démontée par React, jamais par showModal().
        role="dialog"
        aria-modal="true"
        aria-label="Documents (RAG)"
      >
        <div className="locaryn-settings-header">
          <span className="locaryn-settings-title">📚 Base de connaissances (RAG)</span>
          <button
            type="button"
            className="locaryn-settings-close"
            onClick={onClose}
            aria-label="Fermer"
          >
            ✕
          </button>
        </div>

        <div className="locaryn-settings-pane" style={{ padding: 20, overflowY: "auto" }}>
          <p className="locaryn-rag-what">
            <b>À quoi ça sert ?</b> Donnez vos documents au modèle une bonne fois pour toutes.
            Ensuite, à <i>chaque</i> message de ce projet, Locaryn retrouve automatiquement les
            passages utiles et les glisse dans la question — le modèle répond avec
            <b> vos</b> infos, sans que vous ayez à les recoller à chaque fois.
          </p>
          <p className="locaryn-field-hint">
            Tout reste en local (aucun envoi vers Internet). La première indexation démarre un petit
            serveur d'embeddings : comptez quelques secondes.
          </p>
          <p className="locaryn-field-hint" style={{ marginTop: 6 }}>
            ⓘ Par défaut, c'est votre <strong>modèle de chat actif</strong> qui produit les
            embeddings (zéro configuration). Pour une récupération nettement plus fine, installez un
            vrai modèle d'<em>embedding</em> (nomic-embed, bge, e5) — un modèle de chat sépare mal
            les sens.
          </p>

          <div className="locaryn-conn" style={{ marginTop: 10 }}>
            <span
              className={`locaryn-health-dot ${status && status.chunk_count > 0 ? "locaryn-health-ok" : "locaryn-health-off"}`}
            />
            <span>
              {status
                ? status.chunk_count > 0
                  ? `${status.chunk_count} extraits · ${status.sources.length} source(s) · dim ${status.dim}`
                  : "Aucun document indexé"
                : "…"}
            </span>
          </div>

          {status && status.sources.length > 0 && (
            <ul className="locaryn-lora-list" style={{ marginTop: 10 }}>
              {status.sources.map((s) => (
                <li key={s.source} className="locaryn-lora-row">
                  <span className="locaryn-lora-path" title={s.source}>
                    {s.source}
                  </span>
                  <span className="locaryn-lora-scale">{s.chunks}</span>
                </li>
              ))}
            </ul>
          )}

          {/* Drop zone — import files directly instead of pasting text. */}
          <div className="locaryn-field" style={{ marginTop: 20 }}>
            <div className="locaryn-field-label">Importer des fichiers</div>
            <input
              ref={fileRef}
              type="file"
              multiple
              hidden
              accept={TEXT_EXT.map((e) => `.${e}`).join(",")}
              onChange={(e) => {
                if (e.target.files) importFiles(e.target.files);
                e.target.value = "";
              }}
            />
            <button
              type="button"
              className={`locaryn-dropzone${dragOver ? " over" : ""}`}
              style={{ width: "100%", font: "inherit", color: "inherit" }}
              onClick={() => !busy && fileRef.current?.click()}
              onDragOver={(e) => {
                e.preventDefault();
                setDragOver(true);
              }}
              onDragLeave={() => setDragOver(false)}
              onDrop={(e) => {
                e.preventDefault();
                setDragOver(false);
                if (e.dataTransfer.files?.length) importFiles(e.dataTransfer.files);
              }}
            >
              {importing ? (
                <>
                  ⏳ Indexation de <b>{importing}</b>…
                </>
              ) : (
                <>
                  {/* Des <span> et non des <div> : le contenu d'un <button> doit rester
                      du contenu de phrasé, et la colonne flex les rend à l'identique. */}
                  <span className="locaryn-dropzone-main">
                    📄 Glissez vos documents ici, ou cliquez pour choisir
                  </span>
                  <span className="locaryn-dropzone-sub">
                    .txt .md .csv .json .html, code source… — plusieurs fichiers acceptés
                  </span>
                </>
              )}
            </button>
          </div>

          <div className="locaryn-field" style={{ marginTop: 16 }}>
            <div className="locaryn-field-label">Ou coller du texte</div>
            <input
              className="locaryn-input"
              aria-label="Nom de la source"
              placeholder="Nom de la source (ex: notes-archi.md)"
              value={source}
              onChange={(e) => setSource(e.target.value)}
              style={{ marginBottom: 8 }}
            />
            <textarea
              className="locaryn-input"
              placeholder="Collez ici le texte à indexer…"
              value={text}
              onChange={(e) => setText(e.target.value)}
              rows={6}
              style={{
                resize: "vertical",
                fontFamily: "var(--font-mono)",
                fontSize: "var(--text-xs)",
              }}
            />
            <div className="locaryn-field-actions" style={{ marginTop: 10 }}>
              <button
                type="button"
                className="locaryn-btn-primary"
                onClick={index}
                disabled={busy || !text.trim()}
              >
                {busy ? "Indexation…" : "Indexer"}
              </button>
              {status && status.chunk_count > 0 && (
                <button
                  type="button"
                  className="locaryn-btn-ghost"
                  onClick={clearAll}
                  disabled={busy}
                >
                  Tout effacer
                </button>
              )}
            </div>
          </div>

          <div className="locaryn-field" style={{ marginTop: 20 }}>
            <label htmlFor="rag-query" className="locaryn-field-label">
              Tester la recherche
            </label>
            <div className="locaryn-field-row">
              <input
                id="rag-query"
                className="locaryn-input"
                placeholder="Une question pour voir ce qui remonte…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") search();
                }}
              />
              <button
                type="button"
                className="locaryn-btn-ghost"
                onClick={search}
                disabled={busy || !query.trim()}
              >
                Chercher
              </button>
            </div>
            {hits &&
              (hits.length === 0 ? (
                <p className="locaryn-field-hint" style={{ marginTop: 8 }}>
                  Aucun résultat.
                </p>
              ) : (
                <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 8 }}>
                  {hits.map((h, i) => (
                    <div
                      // biome-ignore lint/suspicious/noArrayIndexKey: les résultats sont remplacés en bloc à chaque recherche, jamais réordonnés ni retirés ; deux extraits peuvent être identiques, donc le contenu ne fournit pas de clé.
                      key={i}
                      className="locaryn-lora-row"
                      style={{ flexDirection: "column", alignItems: "stretch", gap: 4 }}
                    >
                      <div
                        style={{
                          display: "flex",
                          justifyContent: "space-between",
                          fontSize: "var(--text-xs)",
                          color: "var(--text-dim)",
                        }}
                      >
                        <span className="locaryn-kv-mono">{h.source}</span>
                        <span>{h.score.toFixed(3)}</span>
                      </div>
                      <div style={{ fontSize: "var(--text-xs)", lineHeight: 1.5 }}>
                        {h.text.length > 240 ? `${h.text.slice(0, 240)}…` : h.text}
                      </div>
                    </div>
                  ))}
                </div>
              ))}
          </div>

          {error && (
            <p className="locaryn-field-hint" style={{ color: "var(--danger)", marginTop: 8 }}>
              {error}
            </p>
          )}
        </div>
      </div>
    </>
  );
}
