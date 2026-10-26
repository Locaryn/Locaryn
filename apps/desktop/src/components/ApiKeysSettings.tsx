import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type ApiKeyCreated, type ApiKeyInfo, type ProviderApiStatus, core } from "../lib/core";

import { confirmer } from "../lib/dialog";
const DUREES: { jours: number | null; libelle: string }[] = [
  { jours: 30, libelle: "30 jours" },
  { jours: 90, libelle: "90 jours" },
  { jours: null, libelle: "Sans expiration" },
];

function date(iso: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleDateString();
}

/**
 * Les clés de l'API fournisseur, une par application (« Cursor du portable »,
 * « Claude Code »…). La clé n'est montrée qu'à sa création : seul son
 * empreinte est gardée. La révoquer coupe aussitôt l'application qui l'utilise.
 */
export function ApiKeysSettings() {
  const [etat, setEtat] = useState<ProviderApiStatus | null>(null);
  const [cles, setCles] = useState<ApiKeyInfo[]>([]);
  const [nom, setNom] = useState("");
  const [duree, setDuree] = useState<number | null>(90);
  const [creee, setCreee] = useState<ApiKeyCreated | null>(null);
  const [copie, setCopie] = useState(false);
  const [occupe, setOccupe] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);

  const lire = useCallback(async () => {
    try {
      const [e, c] = await Promise.all([core.providerApiStatus(), core.listApiKeys()]);
      setEtat(e);
      setCles(c);
    } catch (e) {
      setErreur(String(e));
    }
  }, []);

  useEffect(() => {
    void lire();
  }, [lire]);

  async function creer(e: React.FormEvent) {
    e.preventDefault();
    setOccupe(true);
    setErreur(null);
    try {
      const c = await core.createApiKey(nom.trim(), duree);
      setCreee(c);
      setCopie(false);
      setNom("");
      await lire();
    } catch (err) {
      setErreur(String(err));
    } finally {
      setOccupe(false);
    }
  }

  async function revoquer(c: ApiKeyInfo) {
    const quoi = c.label ? `« ${c.label} »` : `${c.hint}…`;
    if (!(await confirmer(`Révoquer la clé ${quoi} ? L'application qui l'utilise perd l'accès.`))) {
      return;
    }
    try {
      await core.revokeApiKey(c.id);
      await lire();
    } catch (err) {
      setErreur(String(err));
    }
  }

  return (
    <div className="locaryn-field">
      <p className="locaryn-field-hint">
        Une clé par application qui utilise les modèles de ce Locaryn (Cursor, VS Code, Claude Code,
        OpenCode…). Elle ne donne accès qu'aux modèles. Les adresses à donner sont dans Réglages →
        Serveur → API fournisseur
        {etat?.admin
          ? ` ; les clés appartiennent au compte administrateur « ${etat.admin} ».`
          : "."}
      </p>

      {etat && !etat.enabled && (
        <p className="locaryn-vp-warn">
          L'API fournisseur est désactivée : allumez-la dans Réglages → Serveur pour créer une clé.
        </p>
      )}

      {creee && (
        <div className="locaryn-api-bloc locaryn-api-nouvelle">
          <strong>Copiez cette clé maintenant : elle ne sera plus affichée.</strong>
          <span className="locaryn-api-copiable">
            <code>{creee.token}</code>
            <button
              type="button"
              className="locaryn-btn-ghost"
              title="Copier"
              onClick={() => {
                void navigator.clipboard.writeText(creee.token).then(() => setCopie(true));
              }}
            >
              <Icon name={copie ? "check" : "copy"} size={14} />
            </button>
          </span>
          <button
            type="button"
            className="locaryn-btn-ghost"
            style={{ alignSelf: "flex-start" }}
            onClick={() => setCreee(null)}
          >
            J'ai copié la clé
          </button>
        </div>
      )}

      {etat?.enabled && (
        <form className="locaryn-api-creation" onSubmit={creer}>
          <input
            className="locaryn-input"
            placeholder="Nom de l'application (ex : Cursor du portable)"
            value={nom}
            onChange={(e) => setNom(e.target.value)}
            maxLength={80}
          />
          <select
            className="locaryn-input"
            value={duree ?? ""}
            onChange={(e) => setDuree(e.target.value ? Number(e.target.value) : null)}
          >
            {DUREES.map((d) => (
              <option key={d.libelle} value={d.jours ?? ""}>
                {d.libelle}
              </option>
            ))}
          </select>
          <button type="submit" className="locaryn-btn" disabled={occupe}>
            <Icon name="plus" size={14} /> {occupe ? "Création…" : "Créer une clé"}
          </button>
        </form>
      )}

      {erreur && <div className="locaryn-vp-error">{erreur}</div>}

      <div className="locaryn-api-liste">
        {cles.length === 0 ? (
          <p className="locaryn-field-hint">Aucune clé pour l'instant.</p>
        ) : (
          cles.map((c) => (
            <div key={c.id} className="locaryn-api-cle">
              <Icon name="key" size={15} />
              <div className="locaryn-api-cle-texte">
                <span className="locaryn-api-cle-nom">{c.label || "Clé sans nom"}</span>
                <span className="locaryn-field-hint">
                  {c.hint}… · créée le {date(c.created_at)} · dernière utilisation{" "}
                  {c.last_used_at ? date(c.last_used_at) : "jamais"} · expire{" "}
                  {c.expires_at ? `le ${date(c.expires_at)}` : "jamais"}
                </span>
              </div>
              <button
                type="button"
                className="locaryn-btn-ghost"
                title="Révoquer"
                onClick={() => void revoquer(c)}
              >
                <Icon name="trash" size={14} />
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
