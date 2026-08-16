import { useCallback, useEffect, useState } from "react";
import { type MemoryEntry, core } from "../lib/core";

/**
 * Ce que Locaryn retient de vous.
 *
 * L'écran montre le texte exact qui est versé au modèle à chaque message —
 * pas un résumé, pas une reformulation. Une mémoire qu'on ne peut pas lire est
 * une mémoire qu'on ne peut pas corriger, et une mémoire fausse coûte plus
 * cher qu'une mémoire vide.
 */
const CATEGORIES: { id: string; label: string; hint: string }[] = [
  { id: "preference", label: "Préférence", hint: "Réponds-moi en français, va droit au but…" },
  { id: "habitude", label: "Habitude", hint: "Je code le soir, sur Windows avec un GPU NVIDIA…" },
  { id: "projet", label: "Projet", hint: "Je développe Locaryn, une plateforme d'IA locale…" },
  { id: "fait", label: "Fait", hint: "Une chose utile à savoir sur moi…" },
];

export function MemorySettings() {
  const [entries, setEntries] = useState<MemoryEntry[]>([]);
  const [category, setCategory] = useState("preference");
  const [draft, setDraft] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [editDraft, setEditDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setEntries(await core.listMemory());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function add() {
    if (!draft.trim()) return;
    setBusy(true);
    try {
      await core.remember(category, draft.trim());
      setDraft("");
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function saveEdit(entry: MemoryEntry) {
    setBusy(true);
    try {
      await core.editMemory(entry.id, entry.category, editDraft.trim());
      setEditing(null);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const hint = CATEGORIES.find((c) => c.id === category)?.hint ?? "";

  return (
    <section className="locaryn-view-container">
      <div className="locaryn-view-header">
        <h2>Mémoire</h2>
        <p className="locaryn-view-desc">
          Ce que Locaryn retient de vous d'une conversation à l'autre. Ces phrases sont envoyées au
          modèle avec chaque message : il n'y a rien d'autre, et rien de caché. Elles vivent sur le
          service — donc partagées avec votre téléphone, et hébergées par votre serveur quand vous
          en utilisez un.
        </p>
      </div>

      {error && <p className="locaryn-error">{error}</p>}

      <div className="locaryn-card" style={{ maxWidth: "760px" }}>
        <h3>Retenir quelque chose</h3>
        <div className="locaryn-field">
          <label className="locaryn-field-label" htmlFor="memory-category">
            Nature
          </label>
          <select
            id="memory-category"
            className="locaryn-input"
            value={category}
            onChange={(e) => setCategory(e.target.value)}
          >
            {CATEGORIES.map((c) => (
              <option key={c.id} value={c.id}>
                {c.label}
              </option>
            ))}
          </select>
        </div>
        <div className="locaryn-field">
          <label className="locaryn-field-label" htmlFor="memory-content">
            Formulez-le comme vous le diriez
          </label>
          <input
            id="memory-content"
            className="locaryn-input"
            placeholder={hint}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void add();
            }}
          />
        </div>
        <button
          type="button"
          className="locaryn-btn"
          disabled={busy || !draft.trim()}
          onClick={add}
        >
          Retenir
        </button>
      </div>

      <div className="locaryn-card" style={{ maxWidth: "760px" }}>
        <h3>Ce qui est retenu {entries.length > 0 && <span>({entries.length})</span>}</h3>
        {entries.length === 0 ? (
          <p className="locaryn-view-desc">
            Rien pour l'instant. Le modèle ne sait de vous que ce que dit la conversation en cours.
          </p>
        ) : (
          <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
            {entries.map((e) => (
              <li
                key={e.id}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "10px",
                  padding: "10px 0",
                  borderBottom: "1px solid var(--border)",
                }}
              >
                <span className="locaryn-badge" title={`Écrit par : ${e.source}`}>
                  {CATEGORIES.find((c) => c.id === e.category)?.label ?? e.category}
                </span>
                {editing === e.id ? (
                  <>
                    <input
                      className="locaryn-input"
                      style={{ flex: 1 }}
                      value={editDraft}
                      onChange={(ev) => setEditDraft(ev.target.value)}
                    />
                    <button
                      type="button"
                      className="locaryn-btn"
                      disabled={busy}
                      onClick={() => void saveEdit(e)}
                    >
                      Enregistrer
                    </button>
                    <button
                      type="button"
                      className="locaryn-btn-ghost"
                      onClick={() => setEditing(null)}
                    >
                      Annuler
                    </button>
                  </>
                ) : (
                  <>
                    <span style={{ flex: 1 }}>{e.content}</span>
                    <button
                      type="button"
                      className="locaryn-btn-ghost"
                      onClick={() => {
                        setEditing(e.id);
                        setEditDraft(e.content);
                      }}
                    >
                      Corriger
                    </button>
                    <button
                      type="button"
                      className="locaryn-btn-ghost"
                      disabled={busy}
                      onClick={async () => {
                        setBusy(true);
                        try {
                          await core.forgetMemory(e.id);
                          await load();
                        } catch (err) {
                          setError(String(err));
                        } finally {
                          setBusy(false);
                        }
                      }}
                    >
                      Oublier
                    </button>
                  </>
                )}
              </li>
            ))}
          </ul>
        )}
        {entries.length > 0 && (
          <button
            type="button"
            className="locaryn-btn-ghost"
            style={{ marginTop: "14px" }}
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              try {
                await core.forgetAllMemory();
                await load();
              } catch (err) {
                setError(String(err));
              } finally {
                setBusy(false);
              }
            }}
          >
            Tout oublier
          </button>
        )}
      </div>
    </section>
  );
}
