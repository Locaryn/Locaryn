import { useCallback, useEffect, useMemo, useState } from "react";
import { type MemoryEntry, core } from "../lib/core";

type MemoryGroup = {
  id: string;
  label: string;
  entries: MemoryEntry[];
};

const CATEGORY_LABELS: Record<string, string> = {
  preference: "Préférences",
  préférence: "Préférences",
  habit: "Habitudes",
  habitude: "Habitudes",
  project: "Projets",
  projet: "Projets",
  fact: "Informations personnelles",
  fait: "Informations personnelles",
};

function labelFor(category: string): string {
  const key = category.trim().toLowerCase();
  return CATEGORY_LABELS[key] ?? (category.trim() || "Autres informations");
}

function groupMemories(entries: MemoryEntry[]): MemoryGroup[] {
  const groups = new Map<string, MemoryGroup>();
  for (const entry of entries) {
    const id = entry.category.trim().toLowerCase() || "autres";
    const current = groups.get(id);
    if (current) current.entries.push(entry);
    else groups.set(id, { id, label: labelFor(entry.category), entries: [entry] });
  }
  return [...groups.values()];
}

/**
 * La mémoire est un document, pas une liste de formulaires.
 *
 * Les entrées restent séparées dans la base pour pouvoir être oubliées
 * proprement, mais l'interface les rassemble en quelques zones de texte. Un
 * survol révèle le détail du groupe sans transformer mille souvenirs en mille
 * lignes visibles.
 */
export function MemorySettings() {
  const [entries, setEntries] = useState<MemoryEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [hoveredGroup, setHoveredGroup] = useState<string | null>(null);

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

  const groups = useMemo(() => groupMemories(entries), [entries]);
  const documentText = useMemo(
    () =>
      groups
        .map((group) => `${group.label}\n${group.entries.map((entry) => entry.content).join(" ")}`)
        .join("\n\n"),
    [groups],
  );

  async function forgetAll() {
    setBusy(true);
    try {
      await core.forgetAllMemory();
      setEntries([]);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="locaryn-memory-settings">
      <div className="locaryn-memory-intro">
        <div>
          <span className="locaryn-account-eyebrow">MÉMOIRE DU COMPTE</span>
          <h3>Ce que Locaryn retient de vous</h3>
          <p>
            Une vue compacte de tout ce qui est envoyé au modèle avec vos messages. Les souvenirs
            sont regroupés par zone pour rester lisibles, même quand la mémoire grandit.
          </p>
        </div>
        <span className="locaryn-memory-count">
          {entries.length} élément{entries.length === 1 ? "" : "s"}
        </span>
      </div>

      {error && <div className="locaryn-vp-error">{error}</div>}

      {groups.length === 0 ? (
        <div className="locaryn-memory-empty">
          <strong>La mémoire est vide.</strong>
          <span>Locaryn ne conserve encore aucune information durable sur vous.</span>
        </div>
      ) : (
        <>
          <div className="locaryn-memory-document-shell">
            <div className="locaryn-memory-document-toolbar">
              <span>Mémoire globale</span>
              <span>
                {groups.length} zone{groups.length === 1 ? "" : "s"}
              </span>
            </div>
            <article className="locaryn-memory-document" aria-label="Résumé de la mémoire">
              {groups.map((group) => (
                <section
                  key={group.id}
                  className={`locaryn-memory-block${hoveredGroup === group.id ? " is-hovered" : ""}`}
                  onMouseEnter={() => setHoveredGroup(group.id)}
                  onMouseLeave={() => setHoveredGroup(null)}
                >
                  <h4>{group.label}</h4>
                  <p>{group.entries.map((entry) => entry.content).join(" ")}</p>
                  {hoveredGroup === group.id && (
                    <div className="locaryn-memory-popover" role="tooltip">
                      <strong>{group.label}</strong>
                      <span>
                        {group.entries.length} souvenir{group.entries.length === 1 ? "" : "s"}
                      </span>
                      <div>
                        {group.entries.map((entry) => (
                          <p key={entry.id}>{entry.content}</p>
                        ))}
                      </div>
                    </div>
                  )}
                </section>
              ))}
            </article>
          </div>

          <details className="locaryn-memory-technical">
            <summary>Voir le texte exact envoyé au modèle</summary>
            <pre>{documentText}</pre>
          </details>

          <div className="locaryn-memory-actions">
            <span>
              Pour modifier une information, demandez à Locaryn de corriger ou d'oublier ce point
              dans le chat.
            </span>
            <button
              type="button"
              className="locaryn-btn-ghost"
              disabled={busy}
              onClick={() => void forgetAll()}
            >
              Tout oublier
            </button>
          </div>
        </>
      )}
    </div>
  );
}
