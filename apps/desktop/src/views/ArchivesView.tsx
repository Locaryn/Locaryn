import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useMemo, useState } from "react";
import { type Project, type Session, core } from "../lib/core";

type Props = {
  /** Rouvrir une conversation sortie des archives. */
  onOpenSession: (s: Session) => void;
};

/** Une conversation archivée, avec le projet d'où elle vient. */
type Rangee = { session: Session; projet: Project };

/**
 * Les archives.
 *
 * Retirer une conversation d'une liste n'est presque jamais vouloir la perdre.
 * La corbeille archive donc au lieu de supprimer, et il fallait bien un endroit
 * où les retrouver : le voici. On y ressort une conversation d'un geste, et la
 * suppression définitive est un second geste, pris ici, confirmé — pas une
 * étourderie possible depuis la liste principale.
 */
export function ArchivesView({ onOpenSession }: Props) {
  const [rangees, setRangees] = useState<Rangee[] | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [occupe, setOccupe] = useState<string | null>(null);
  const [aSupprimer, setASupprimer] = useState<string | null>(null);
  const [recherche, setRecherche] = useState("");

  const recharger = useCallback(async () => {
    try {
      const projets = await core.listProjects();
      // Les archives sont rangées par projet côté serveur : on parcourt les
      // projets, et on remet tout à plat par date, parce que ce qu'on cherche
      // ici c'est « la conversation d'avant-hier », pas « le projet trois ».
      const listes = await Promise.all(
        projets.map(async (p) => {
          const sessions = await core.archivedSessions(p.id).catch(() => [] as Session[]);
          return sessions.map((session) => ({ session, projet: p }));
        }),
      );
      const tout = listes.flat();
      tout.sort((a, b) => ((a.session.archived_at ?? "") < (b.session.archived_at ?? "") ? 1 : -1));
      setRangees(tout);
      setErreur(null);
    } catch (e) {
      setErreur(String(e));
      setRangees([]);
    }
  }, []);

  useEffect(() => {
    void recharger();
  }, [recharger]);

  const rangeesFiltrees = useMemo(() => {
    if (!rangees) return [];
    const terme = normaliser(recherche.trim());
    if (!terme) return rangees;
    return rangees.filter((r) => {
      const texte = normaliser(
        [
          r.session.title ?? "",
          r.projet.name,
          r.session.archived_at ? dateCourte(r.session.archived_at) : "",
        ].join(" "),
      );
      return texte.includes(terme);
    });
  }, [rangees, recherche]);

  const totalArchives = rangees?.length ?? 0;
  const rechercheActive = recherche.trim().length > 0;

  async function ressortir(r: Rangee) {
    setOccupe(r.session.id);
    try {
      await core.archiveSession(r.session.id, false);
      setRangees((prev) => prev?.filter((x) => x.session.id !== r.session.id) ?? null);
      onOpenSession(r.session);
    } catch (e) {
      setErreur(String(e));
    } finally {
      setOccupe(null);
    }
  }

  async function supprimer(r: Rangee) {
    setOccupe(r.session.id);
    try {
      await core.deleteSession(r.session.id);
      setRangees((prev) => prev?.filter((x) => x.session.id !== r.session.id) ?? null);
      setASupprimer(null);
    } catch (e) {
      setErreur(String(e));
    } finally {
      setOccupe(null);
    }
  }

  return (
    <div className="locaryn-archives">
      <header className="locaryn-archives-head">
        <h1>
          <Icon name="archive" size={20} /> Archives
        </h1>
        <p className="locaryn-archives-hint">
          Rien n'est perdu ici. Une conversation en ressort telle qu'elle était ; la supprimer est
          un geste séparé, et définitif.
        </p>
      </header>

      {erreur && <p className="locaryn-error">{erreur}</p>}

      {rangees !== null && (
        <div className="locaryn-archives-toolbar">
          <label className="locaryn-archives-search" htmlFor="locaryn-archives-search">
            <span className="locaryn-archives-search-icon" aria-hidden="true">
              <Icon name="search" size={15} />
            </span>
            <span className="sr-only">Rechercher dans les archives</span>
            <input
              id="locaryn-archives-search"
              className="locaryn-input"
              type="search"
              value={recherche}
              placeholder="Rechercher une conversation ou un projet…"
              onChange={(e) => setRecherche(e.target.value)}
            />
            {recherche && (
              <button
                type="button"
                className="locaryn-archives-search-clear"
                aria-label="Effacer la recherche"
                onClick={() => setRecherche("")}
              >
                <Icon name="close" size={14} />
              </button>
            )}
          </label>
          <span className="locaryn-archives-counter" aria-live="polite">
            {rechercheActive
              ? `${rangeesFiltrees.length} résultat${rangeesFiltrees.length === 1 ? "" : "s"} sur ${totalArchives} archive${totalArchives === 1 ? "" : "s"}`
              : `${totalArchives} archive${totalArchives === 1 ? "" : "s"}`}
          </span>
        </div>
      )}

      {rangees === null && <p className="locaryn-archives-empty">Lecture des archives…</p>}

      {rangees?.length === 0 && (
        <p className="locaryn-archives-empty">
          {rechercheActive
            ? "Aucune archive à rechercher pour le moment."
            : "Aucune conversation archivée. Glissez-en une sur la corbeille pour la ranger ici."}
        </p>
      )}

      {rangees && rangees.length > 0 && rangeesFiltrees.length === 0 && (
        <p className="locaryn-archives-empty">
          Aucune archive ne correspond à « {recherche.trim()} ».
        </p>
      )}

      <ul className="locaryn-archives-list">
        {rangeesFiltrees.map((r) => (
          <li key={r.session.id} className="locaryn-archives-row">
            <div className="locaryn-archives-text">
              <span className="locaryn-archives-title">{r.session.title || "Sans titre"}</span>
              <span className="locaryn-archives-meta">
                {r.projet.name}
                {r.session.archived_at && ` · archivée le ${dateCourte(r.session.archived_at)}`}
              </span>
            </div>

            {aSupprimer === r.session.id ? (
              <div className="locaryn-archives-actions">
                <span className="locaryn-archives-warn">Supprimer définitivement ?</span>
                <button
                  type="button"
                  className="locaryn-btn-danger"
                  disabled={occupe === r.session.id}
                  onClick={() => supprimer(r)}
                >
                  Supprimer
                </button>
                <button type="button" onClick={() => setASupprimer(null)}>
                  Annuler
                </button>
              </div>
            ) : (
              <div className="locaryn-archives-actions">
                <button
                  type="button"
                  disabled={occupe === r.session.id}
                  onClick={() => ressortir(r)}
                  title="Remettre la conversation dans sa liste"
                >
                  <Icon name="refresh" size={14} /> Ressortir
                </button>
                <button
                  type="button"
                  className="locaryn-archives-del"
                  disabled={occupe === r.session.id}
                  onClick={() => setASupprimer(r.session.id)}
                  title="Supprimer définitivement"
                >
                  <Icon name="trash" size={14} />
                </button>
              </div>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** « 14 mars » plutôt qu'un horodatage : on cherche un souvenir, pas une trace. */
function normaliser(value: string): string {
  return value.normalize("NFD").replace(/\p{M}/gu, "").toLocaleLowerCase();
}

function dateCourte(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { day: "numeric", month: "long" });
}
