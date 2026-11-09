import { Icon } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { useI18n } from "../lib/i18n";
import { type GraviteNote, envoyerProbleme, lireDernierEchange } from "../lib/rapports";
import "../styles/rapports.css";

type Etat = "saisie" | "envoi" | "envoye";

/** Le formulaire : description, gravité, échange joint ou non. */
function Formulaire({ onFermer }: { onFermer: () => void }) {
  const { t } = useI18n();
  const [description, setDescription] = useState("");
  const [gravite, setGravite] = useState<GraviteNote>("info");
  const echange = lireDernierEchange();
  const [joindre, setJoindre] = useState(echange !== null);
  const [etat, setEtat] = useState<Etat>("saisie");
  const [erreur, setErreur] = useState<string | null>(null);

  async function envoyer() {
    setEtat("envoi");
    setErreur(null);
    try {
      await envoyerProbleme(description.trim(), gravite, joindre ? echange : null);
      setEtat("envoye");
    } catch (e) {
      console.warn("[Problème] envoi impossible :", e);
      setErreur(e instanceof Error ? e.message : String(e));
      setEtat("saisie");
    }
  }

  if (etat === "envoye") {
    return (
      <div className="locaryn-probleme-corps">
        <p className="locaryn-probleme-merci">
          <Icon name="check" size={16} /> {t("Envoyé, merci. On s'en occupe.")}
        </p>
        <button type="button" className="locaryn-probleme-btn" onClick={onFermer}>
          {t("Fermer")}
        </button>
      </div>
    );
  }

  return (
    <div className="locaryn-probleme-corps">
      <label className="locaryn-probleme-label" htmlFor="locaryn-probleme-texte">
        {t("Décrivez ce qui s'est passé")}
      </label>
      <textarea
        id="locaryn-probleme-texte"
        className="locaryn-probleme-texte"
        rows={4}
        maxLength={4000}
        value={description}
        placeholder={t("Ce que vous faisiez, ce que vous attendiez, ce qui est arrivé…")}
        onChange={(e) => setDescription(e.target.value)}
      />
      <div className="locaryn-probleme-gravite" role="radiogroup" aria-label={t("Gravité")}>
        {(["critique", "info"] as const).map((g) => (
          <button
            key={g}
            type="button"
            role="radio"
            aria-checked={gravite === g}
            className="locaryn-probleme-choix"
            data-selected={gravite === g}
            onClick={() => setGravite(g)}
          >
            {g === "critique" ? t("Bug critique") : t("Gêne ou mauvaise réponse")}
          </button>
        ))}
      </div>
      {echange && (
        <label className="locaryn-probleme-case">
          <input type="checkbox" checked={joindre} onChange={(e) => setJoindre(e.target.checked)} />
          <span>{t("Joindre le dernier échange de la conversation")}</span>
        </label>
      )}
      <p className="locaryn-probleme-info">
        {t(
          "Envoi anonyme : votre description, les journaux du service et du modèle, la version, les morphs et le modèle utilisé. Chemins, e-mails, clés et adresses sont retirés.",
        )}
      </p>
      {erreur && <p className="locaryn-probleme-erreur">{erreur}</p>}
      <div className="locaryn-probleme-actions">
        <button type="button" className="locaryn-probleme-btn" onClick={onFermer}>
          {t("Annuler")}
        </button>
        <button
          type="button"
          className="locaryn-probleme-btn locaryn-probleme-btn-primaire"
          disabled={etat === "envoi" || description.trim().length < 3}
          onClick={() => void envoyer()}
        >
          {etat === "envoi" ? t("Envoi…") : t("Envoyer")}
        </button>
      </div>
    </div>
  );
}

/**
 * « J'ai un problème », dans la barre du bas à gauche de la cloche : tout ne
 * se voit pas comme une erreur. La personne décrit, tout part avec.
 */
export function SignalerProbleme() {
  const { t } = useI18n();
  const [ouvert, setOuvert] = useState(false);
  const panneau = useRef<HTMLDialogElement | null>(null);
  const bouton = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!ouvert) return;
    panneau.current?.querySelector("textarea")?.focus();
    const clic = (e: MouseEvent) => {
      const cible = e.target as Node;
      if (panneau.current?.contains(cible) || bouton.current?.contains(cible)) return;
      setOuvert(false);
    };
    const touche = (e: KeyboardEvent) => e.key === "Escape" && setOuvert(false);
    document.addEventListener("mousedown", clic);
    document.addEventListener("keydown", touche);
    return () => {
      document.removeEventListener("mousedown", clic);
      document.removeEventListener("keydown", touche);
    };
  }, [ouvert]);

  return (
    <>
      <button
        ref={bouton}
        type="button"
        className="locaryn-probleme-declencheur"
        aria-expanded={ouvert}
        onClick={() => setOuvert((v) => !v)}
      >
        <Icon name="question" size={14} />
        <span>{t("J'ai un problème")}</span>
      </button>
      {ouvert && (
        <dialog
          open
          ref={panneau}
          className="locaryn-notif-panel locaryn-probleme-panel"
          aria-label={t("Signaler un problème")}
        >
          <div className="locaryn-notif-head">
            <strong>{t("Signaler un problème")}</strong>
          </div>
          <Formulaire onFermer={() => setOuvert(false)} />
        </dialog>
      )}
    </>
  );
}
