import { useEffect, useState } from "react";
import { type ArchivedConversation, api } from "../lib/core";
import { Screen } from "./Screen";

type Props = {
  onBack: () => void;
  /** Rouvrir une conversation sortie des archives dans le chat. */
  onOpenChat: (sessionId: string) => void;
};

/**
 * Les archives.
 *
 * Rien n'est perdu ici : une conversation en ressort telle qu'elle était.
 * C'est un endroit qu'on ne visite presque jamais — c'est pour ça qu'il est
 * dans les réglages, pas dans l'historique. On y vient pour retrouver « la
 * conversation d'avant-hier », et on la ressort d'un geste.
 */
export function ArchivesScreen({ onBack, onOpenChat }: Props) {
  const [rangees, setRangees] = useState<ArchivedConversation[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  useEffect(() => {
    void api
      .archivedConversations()
      .then(setRangees)
      .catch((e) => {
        setError(String(e));
        setRangees([]);
      });
  }, []);

  async function ressortir(r: ArchivedConversation) {
    setBusyId(r.id);
    setError(null);
    try {
      await api.archiveConversation(r.id, false);
      setRangees((prev) => prev?.filter((x) => x.id !== r.id) ?? null);
      onOpenChat(r.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
    }
  }

  return (
    <Screen title="Archives" onBack={onBack}>
      {error && <p className="lo-error">{error}</p>}

      {rangees === null && (
        <div className="lo-loading-row" role="status">
          <span className="lo-spinner" aria-hidden />
          <span>Lecture des archives…</span>
        </div>
      )}

      {rangees?.length === 0 && (
        <p className="lo-sub">
          Aucune conversation archivée. Une conversation archivée depuis l'historique se retrouve
          ici, rien n'est jamais perdu.
        </p>
      )}

      <ul className="lo-cards">
        {rangees?.map((r) => (
          <li key={r.id} className="lo-card">
            <div className="lo-card-text">
              <span className="lo-card-title">{r.title}</span>
              <span className="lo-hint">
                {r.project}
                {r.archived_at ? ` · archivée le ${dateCourte(r.archived_at)}` : ""}
              </span>
            </div>
            <div className="lo-card-actions">
              <button
                type="button"
                className="lo-btn-small lo-btn-small-on"
                disabled={busyId === r.id}
                onClick={() => void ressortir(r)}
              >
                {busyId === r.id ? "…" : "Restaurer"}
              </button>
            </div>
          </li>
        ))}
      </ul>
    </Screen>
  );
}

/** « 14 mars » plutôt qu'un horodatage : on cherche un souvenir, pas une trace. */
function dateCourte(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString("fr-FR", { day: "numeric", month: "long" });
}
