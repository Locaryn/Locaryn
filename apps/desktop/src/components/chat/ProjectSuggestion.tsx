import { Icon } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { core } from "../../lib/core";

type Props = {
  sessionId: string | null;
  /** Le projet actuel : une conversation déjà rangée n'a rien à se voir proposer. */
  projectId?: string | null;
  /** Combien de messages compte la conversation — la question ne vaut d'être
   *  posée qu'une fois qu'il y a de quoi juger. */
  messageCount: number;
  /** Conversation éphémère : rien n'en sera gardé, donc rien n'est à ranger. */
  ephemeral?: boolean;
  /** Après le déplacement, pour que la liste suive. */
  onMoved?: (projectId: string) => void;
};

/**
 * « Ranger cette conversation dans <projet> ? »
 *
 * Un petit modèle relit l'échange et dit s'il relève d'un projet existant. Il
 * répond « nulle part » la plupart du temps, et c'est voulu : la ligne
 * n'apparaît que quand il reconnaît vraiment quelque chose.
 *
 * Elle se pose au-dessus du composeur, là où le regard revient entre deux
 * messages, et pas au milieu du fil : une conversation en cours ne doit pas
 * être coupée par une question d'intendance. Refuser la fait taire pour cette
 * conversation — reposer la même question est le meilleur moyen qu'elle soit
 * ignorée pour de bon.
 */
export function ProjectSuggestion({
  sessionId,
  projectId,
  messageCount,
  ephemeral,
  onMoved,
}: Props) {
  const [suggestion, setSuggestion] = useState<{ id: string; name: string } | null>(null);
  const [busy, setBusy] = useState(false);
  /** Conversations où la question a déjà été posée, ou écartée. */
  const vues = useRef<Set<string>>(new Set());

  useEffect(() => {
    if (!sessionId || ephemeral) return;
    // Deux messages, c'est un aller-retour : de quoi reconnaître un sujet.
    // En dessous, la question porterait sur une phrase.
    if (messageCount < 2) return;
    if (vues.current.has(sessionId)) return;
    vues.current.add(sessionId);

    let annule = false;
    void (async () => {
      try {
        const r = await core.suggestProject(sessionId);
        if (annule || !r.project_id || r.project_id === projectId) return;
        setSuggestion({ id: r.project_id, name: r.project_name ?? "ce projet" });
      } catch {
        // Une aide qui échoue ne s'annonce pas : elle ne s'affiche pas.
      }
    })();
    return () => {
      annule = true;
    };
  }, [sessionId, projectId, messageCount, ephemeral]);

  // Changer de conversation efface la proposition de la précédente.
  useEffect(() => {
    setSuggestion(null);
  }, []);

  if (!suggestion || !sessionId) return null;

  async function ranger() {
    if (!sessionId || !suggestion) return;
    setBusy(true);
    try {
      await core.moveSession(sessionId, suggestion.id);
      onMoved?.(suggestion.id);
      setSuggestion(null);
    } catch {
      setBusy(false);
    }
  }

  return (
    <div className="locaryn-suggest">
      <Icon name="project" size={14} />
      <span className="locaryn-suggest-text">
        Ranger cette conversation dans <strong>{suggestion.name}</strong> ?
      </span>
      <button type="button" className="locaryn-suggest-yes" disabled={busy} onClick={ranger}>
        Ranger
      </button>
      <button
        type="button"
        className="locaryn-suggest-no"
        onClick={() => setSuggestion(null)}
        aria-label="Ne pas ranger"
        title="Ne pas ranger"
      >
        <Icon name="close" size={13} />
      </button>
    </div>
  );
}
