import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type ProviderApiStatus, core } from "../lib/core";

/** Prévenir les autres écrans (la liste « Compte & Profil ») que l'API a
 *  changé d'état. */
export const API_FOURNISSEUR_EVENT = "locaryn:api-fournisseur";
/** Demander l'ouverture du volet des clés d'API. */
export const OUVRIR_CLES_API_EVENT = "locaryn:ouvrir-cles-api";

function Copiable({ texte }: { texte: string }) {
  const [copie, setCopie] = useState(false);
  return (
    <span className="locaryn-api-copiable">
      <code>{texte}</code>
      <button
        type="button"
        className="locaryn-btn-ghost"
        title="Copier"
        onClick={() => {
          void navigator.clipboard.writeText(texte).then(() => {
            setCopie(true);
            window.setTimeout(() => setCopie(false), 1500);
          });
        }}
      >
        <Icon name={copie ? "check" : "copy"} size={14} />
      </button>
    </span>
  );
}

/**
 * L'API fournisseur : les modèles de cette machine servis aux autres
 * applications (Cursor, VS Code, Claude Code, OpenCode…), chacune avec sa clé
 * d'API. Rien d'autre que les modèles n'est ouvert à ces clés.
 */
export function ProviderApiSettings() {
  const [etat, setEtat] = useState<ProviderApiStatus | null>(null);
  const [occupe, setOccupe] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);

  const lire = useCallback(() => {
    core
      .providerApiStatus()
      .then(setEtat)
      .catch((e) => setErreur(String(e)));
  }, []);

  useEffect(() => {
    lire();
    // Le mode serveur s'allume ou s'éteint juste au-dessus.
    const t = window.setInterval(lire, 5000);
    return () => window.clearInterval(t);
  }, [lire]);

  async function basculer(actif: boolean) {
    setOccupe(true);
    setErreur(null);
    try {
      setEtat(await core.setProviderApi(actif));
      window.dispatchEvent(new CustomEvent(API_FOURNISSEUR_EVENT));
    } catch (e) {
      setErreur(String(e));
    } finally {
      setOccupe(false);
    }
  }

  if (!etat) return null;
  // L'adresse d'Anthropic se donne sans « /v1 » : Claude Code l'ajoute.
  const racineLocale = etat.local_url.replace(/\/v1$/, "");

  return (
    <div className="locaryn-field">
      <div className="locaryn-field-label">API fournisseur</div>
      <p className="locaryn-field-hint">
        Prête vos modèles à d'autres applications (Cursor, VS Code, scripts), chacune avec sa clé.
      </p>

      <div className="locaryn-srv-row">
        <label className="locaryn-srv-toggle">
          <input
            type="checkbox"
            checked={etat.enabled}
            disabled={occupe || (!etat.enabled && Boolean(etat.blocker))}
            onChange={(e) => void basculer(e.target.checked)}
          />
          <span>{occupe ? "…" : etat.enabled ? "API activée" : "API désactivée"}</span>
        </label>
        {etat.enabled && etat.server_running && (
          <span className="locaryn-srv-live">en service</span>
        )}
      </div>

      {etat.blocker && <p className="locaryn-vp-warn">{etat.blocker}</p>}
      {erreur && <div className="locaryn-vp-error">{erreur}</div>}

      {etat.enabled && (
        <div className="locaryn-api-bloc">
          <div className="locaryn-api-ligne">
            <span className="locaryn-api-libelle">Sur cette machine</span>
            <Copiable texte={etat.local_url} />
          </div>
          <div className="locaryn-api-ligne">
            <span className="locaryn-api-libelle">Depuis le réseau</span>
            <Copiable texte={etat.network_url} />
          </div>
          <p className="locaryn-field-hint">
            Sur cette machine, préférez l'adresse locale : pas de certificat à accepter.
          </p>

          <button
            type="button"
            className="locaryn-btn"
            style={{ width: "auto", padding: "6px 14px", fontSize: 13 }}
            onClick={() => window.dispatchEvent(new CustomEvent(OUVRIR_CLES_API_EVENT))}
          >
            <Icon name="key" size={14} /> Gérer les clés d'API
          </button>

          <details className="locaryn-api-exemples">
            <summary>Configurer une application</summary>
            <div className="locaryn-api-exemple">
              <strong>Cursor, VS Code (Continue, Cline), tout client OpenAI</strong>
              <p className="locaryn-field-hint">
                Fournisseur « OpenAI compatible » : adresse de base <code>{etat.local_url}</code>,
                clé : votre clé d'API, modèle : le nom d'un modèle installé (la liste est sur{" "}
                <code>{etat.local_url}/models</code>).
              </p>
            </div>
            <div className="locaryn-api-exemple">
              <strong>Claude Code</strong>
              <pre>{`ANTHROPIC_BASE_URL=${racineLocale}\nANTHROPIC_AUTH_TOKEN=<votre clé d'API>\nANTHROPIC_MODEL=<nom du modèle>`}</pre>
            </div>
            <div className="locaryn-api-exemple">
              <strong>OpenCode (opencode.json)</strong>
              <pre>{`{
  "provider": {
    "locaryn": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "Locaryn",
      "options": {
        "baseURL": "${etat.local_url}",
        "apiKey": "<votre clé d'API>"
      },
      "models": { "<nom du modèle>": {} }
    }
  }
}`}</pre>
            </div>
          </details>
        </div>
      )}
    </div>
  );
}
