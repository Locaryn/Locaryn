import { useEffect, useState } from "react";
import { type SystemPrompt, core } from "../lib/core";

/**
 * Le caractère que vous donnez au modèle, si vous lui en donnez un.
 *
 * L'application n'en pose aucun. Une version antérieure ouvrait chaque
 * conversation par « You are Locaryn, an AI coding assistant » : le modèle se
 * présentait alors comme conçu pour la programmation et refusait le reste,
 * là où le même modèle lancé hors de l'application répondait. Ce qu'un modèle
 * installé accepte de faire regarde son auteur et la personne qui l'a choisi ;
 * le logiciel qui le lance n'a pas à trancher à leur place.
 *
 * Reste, quand des outils sont offerts, ce qui explique comment s'en servir —
 * lire un fichier plutôt qu'en deviner le contenu. C'est de la mécanique, pas
 * un caractère, et sans elle l'assistant invente le contenu des fichiers.
 */
export function SystemPromptSetting() {
  const [state, setState] = useState<SystemPrompt | null>(null);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    core
      .systemPrompt()
      .then((s) => {
        setState(s);
        setDraft(s.texte ?? "");
      })
      .catch((e) => setError(String(e)));
  }, []);

  async function enregistrer(texte: string | null) {
    setBusy(true);
    setError(null);
    try {
      const suivant = await core.setSystemPrompt(texte);
      setState(suivant);
      setDraft(suivant.texte ?? "");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const donne = state?.texte != null;

  return (
    <div className="locaryn-field">
      <div className="locaryn-field-label">Caractère du modèle</div>
      <p className="locaryn-field-hint">
        Par défaut l'application ne pose rien devant le modèle : il répond avec son propre
        caractère, exactement comme lancé hors d'elle. Écrivez ici ce que vous voulez qu'il soit —
        un compagnon, un correcteur, un spécialiste d'un domaine — et ce texte ouvrira chaque
        conversation.
      </p>

      <textarea
        className="locaryn-textarea"
        rows={8}
        value={draft}
        disabled={busy || !state}
        placeholder="Tu es… (laissez vide pour ne rien poser devant le modèle)"
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => {
          if (draft.trim() !== (state?.texte ?? "")) void enregistrer(draft.trim() || null);
        }}
      />

      <div className="locaryn-field-actions" style={{ justifyContent: "flex-start", gap: 8 }}>
        <button
          type="button"
          className="locaryn-btn-ghost"
          disabled={busy || !donne}
          onClick={() => void enregistrer(null)}
        >
          Ne rien poser
        </button>
      </div>

      <p className="locaryn-field-hint">
        {donne
          ? "Ce texte ouvre chaque conversation — sauf les éphémères, qui repartent toujours de rien."
          : "Rien n'est posé. Quand des outils sont offerts, l'assistant reçoit seulement de quoi s'en servir correctement : aller lire un fichier plutôt qu'en deviner le contenu."}
      </p>

      {error && <div className="locaryn-vp-error">{error}</div>}
    </div>
  );
}
