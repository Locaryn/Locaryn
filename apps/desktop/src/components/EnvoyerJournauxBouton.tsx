import { Icon } from "@locaryn/ui-core";
import { useState } from "react";
import { useI18n } from "../lib/i18n";
import { envoyerErreur } from "../lib/rapports";
import "../styles/rapports.css";

type Etat = "repos" | "envoi" | "envoye" | "erreur";

/**
 * « Envoyer les journaux », sous une erreur. Toujours disponible, même quand
 * la personne a refusé l'envoi automatique : c'est elle qui clique.
 */
export function EnvoyerJournauxBouton({
  erreur,
  modele,
}: {
  /** Le texte de l'erreur, joint au rapport (nettoyé côté Rust). */
  erreur: string;
  modele?: string | null;
}) {
  const { t } = useI18n();
  const [etat, setEtat] = useState<Etat>("repos");
  const [detail, setDetail] = useState<string | null>(null);

  async function envoyer() {
    setEtat("envoi");
    setDetail(null);
    try {
      await envoyerErreur(erreur, modele);
      setEtat("envoye");
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      console.warn("[Journaux] envoi impossible :", e);
      setDetail(message);
      setEtat("erreur");
    }
  }

  return (
    <span className="locaryn-envoi-journaux">
      <button
        type="button"
        className="locaryn-envoi-journaux-btn"
        data-state={etat}
        disabled={etat === "envoi" || etat === "envoye"}
        onClick={() => void envoyer()}
      >
        <Icon name={etat === "envoye" ? "check" : "bug"} size={14} />
        {etat === "envoi"
          ? t("Envoi…")
          : etat === "envoye"
            ? t("Journaux envoyés, merci")
            : etat === "erreur"
              ? t("Réessayer l'envoi")
              : t("Envoyer les journaux")}
      </button>
      {etat === "repos" && (
        <span className="locaryn-envoi-journaux-note">{t("Anonyme, sans vos conversations")}</span>
      )}
      {detail && <span className="locaryn-envoi-journaux-erreur">{detail}</span>}
    </span>
  );
}
