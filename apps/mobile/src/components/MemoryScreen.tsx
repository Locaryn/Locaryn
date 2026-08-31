import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useMemo, useState } from "react";
import { type MemoryEntry, type MemoryGroup, api } from "../lib/core";
import { Screen } from "./Screen";

type Props = { onBack: () => void };

/** Le temps que met une fiche à s'effacer avant de quitter la liste. */
const FORGET_MS = 320;

const GROUPS: { value: MemoryGroup; label: string }[] = [
  { value: "vous", label: "Vous" },
  { value: "sujets", label: "Sujets" },
  { value: "zones", label: "Zones" },
  { value: "personnes", label: "Personnes" },
];

function groupLabel(group: MemoryGroup): string {
  return GROUPS.find((g) => g.value === group)?.label ?? "Sujets";
}

/**
 * Ce que le serveur retient de vous — une fiche par sujet, groupée.
 *
 * Au repos, un titre et un résumé d'une ligne. Sur un téléphone il n'y a pas
 * de survol : c'est la touche qui ouvre une fiche, révèle ses détails et
 * propose de l'oublier.
 */
export function MemoryScreen({ onBack }: Props) {
  const [entries, setEntries] = useState<MemoryEntry[] | null>(null);
  const [draft, setDraft] = useState("");
  const [group, setGroup] = useState<MemoryGroup>("sujets");
  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** La fiche ouverte : sur un téléphone, la touche remplace le survol. */
  const [ouvert, setOuvert] = useState<string | null>(null);
  const [forgetting, setForgetting] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setEntries(await api.listMemory());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const grouped = useMemo(() => {
    const parGroupe = new Map<MemoryGroup, MemoryEntry[]>();
    for (const g of GROUPS) parGroupe.set(g.value, []);
    for (const entry of entries ?? []) {
      (parGroupe.get(entry.group) ?? parGroupe.get("sujets"))?.push(entry);
    }
    return parGroupe;
  }, [entries]);

  async function add() {
    const detail = draft.trim();
    const nom = title.trim();
    if (!detail || !nom || busy) return;
    setBusy(true);
    try {
      await api.remember(group, nom, detail);
      setDraft("");
      setTitle("");
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function forget(id: string) {
    setForgetting(id);
    await new Promise((resolve) => setTimeout(resolve, FORGET_MS));
    try {
      await api.forget(id);
      setEntries((prev) => prev?.filter((e) => e.id !== id) ?? null);
      setOuvert(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setForgetting(null);
    }
  }

  const total = entries?.length ?? 0;

  return (
    <Screen title="Mémoire" onBack={onBack}>
      <p className="lo-hint">
        Une fiche par sujet — vous, vos centres d'intérêt, vos projets, les personnes que vous
        mentionnez. Touchez une fiche pour voir ses détails et l'oublier.
      </p>

      {entries === null && !error && <p className="lo-sub">Chargement…</p>}
      {entries?.length === 0 && <p className="lo-sub">Rien pour l'instant.</p>}

      {entries && total > 0 && (
        <div className="lo-memory-document">
          {GROUPS.map((g) => {
            const rows = grouped.get(g.value) ?? [];
            if (rows.length === 0) return null;
            return (
              <div key={g.value}>
                <p className="lo-memory-group-title">{groupLabel(g.value)}</p>
                {rows.map((e) => {
                  const actif = ouvert === e.id;
                  return (
                    <div
                      key={e.id}
                      className={`lo-memory-para${actif ? " is-open" : ""}${forgetting === e.id ? " is-forgetting" : ""}`}
                    >
                      <button
                        type="button"
                        className="lo-memory-text"
                        aria-expanded={actif}
                        onClick={() => setOuvert(actif ? null : e.id)}
                      >
                        <strong>{e.title}</strong>
                        {!actif && <span className="lo-memory-summary"> — {e.summary}</span>}
                      </button>
                      {actif && (
                        <>
                          {e.details.length > 0 && (
                            <ul className="lo-memory-details">
                              {e.details.map((d) => (
                                <li key={d}>{d}</li>
                              ))}
                            </ul>
                          )}
                          <div className="lo-memory-foot">
                            <span className="lo-memory-meta">{groupLabel(e.group)}</span>
                            <button
                              type="button"
                              className="lo-btn-small"
                              onClick={() => void forget(e.id)}
                              aria-label={`Oublier : ${e.title}`}
                            >
                              <Icon name="trash" size={15} /> Oublier
                            </button>
                          </div>
                        </>
                      )}
                    </div>
                  );
                })}
              </div>
            );
          })}
        </div>
      )}

      <label className="lo-label" htmlFor="mem-group">
        Groupe
      </label>
      <select
        id="mem-group"
        className="lo-input"
        value={group}
        onChange={(e) => setGroup(e.target.value as MemoryGroup)}
      >
        {GROUPS.map((g) => (
          <option key={g.value} value={g.value}>
            {g.label}
          </option>
        ))}
      </select>

      <label className="lo-label" htmlFor="mem-title">
        Titre
      </label>
      <input
        id="mem-title"
        className="lo-input"
        placeholder="Bot Bastet"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
      />

      <label className="lo-label" htmlFor="mem-new">
        À retenir
      </label>
      <input
        id="mem-new"
        className="lo-input"
        placeholder="Je préfère les réponses courtes."
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && void add()}
      />
      <button
        type="button"
        className="lo-btn"
        disabled={busy || !draft.trim() || !title.trim()}
        onClick={add}
      >
        {busy ? "Enregistrement…" : "Retenir"}
      </button>

      {error && <p className="lo-error">{error}</p>}
    </Screen>
  );
}
