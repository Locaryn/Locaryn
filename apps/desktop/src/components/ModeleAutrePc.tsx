import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { ACTIVE_MODEL_CHANGED, type CloudModel, type ServeurModeles, core } from "../lib/core";

/** Ce qu'on sait d'un PC ajouté une fois sa liste lue. */
interface EtatServeur {
  modeles: CloudModel[];
  erreur?: string;
  /** Le modèle de ce PC qui sert la conversation, s'il la sert. */
  actif: string | null;
}

/**
 * Emprunter le modèle d'un autre PC Locaryn, sans rien lui confier d'autre.
 *
 * « Se connecter à un serveur » envoie tout là-bas : conversations, projets,
 * fichiers, et les outils. Un MCP qui n'existe que sur ce poste — Roblox Studio
 * ouvert ici — y aurait été appelé pour rien. Ici, seul le calcul du modèle
 * part sur l'autre PC ; la boucle d'agent et ses outils restent ici.
 */
export function ModeleAutrePc() {
  const [serveurs, setServeurs] = useState<ServeurModeles[]>([]);
  const [etats, setEtats] = useState<Record<string, EtatServeur>>({});
  const [choix, setChoix] = useState<Record<string, string>>({});
  const [adresse, setAdresse] = useState("");
  const [identifiant, setIdentifiant] = useState("");
  const [motDePasse, setMotDePasse] = useState("");
  const [occupe, setOccupe] = useState<string | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);

  const recharger = useCallback(async () => {
    const liste = await core.serveursModeles().catch(() => []);
    setServeurs(liste);
    const catalogues = await core.cloudProviders().catch(() => []);
    const suivants: Record<string, EtatServeur> = {};
    for (const s of liste) {
      const actif = catalogues.find((c) => c.id === s.id)?.active_model ?? null;
      try {
        suivants[s.id] = { modeles: await core.cloudProviderModels(s.id, true), actif };
      } catch (e) {
        suivants[s.id] = { modeles: [], actif, erreur: String(e) };
      }
    }
    setEtats(suivants);
  }, []);

  useEffect(() => {
    void recharger();
  }, [recharger]);

  async function ajouter() {
    setOccupe("ajout");
    setErreur(null);
    setInfo(null);
    try {
      const s = await core.serveurModelesAjouter(adresse, identifiant, motDePasse);
      setAdresse("");
      setIdentifiant("");
      setMotDePasse("");
      setInfo(`${s.nom} est ajouté. Choisissez son modèle ci-dessous.`);
      await recharger();
    } catch (e) {
      setErreur(String(e));
    } finally {
      setOccupe(null);
    }
  }

  async function utiliser(s: ServeurModeles) {
    const modele = choix[s.id] ?? etats[s.id]?.modeles[0]?.id;
    if (!modele) return;
    setOccupe(s.id);
    setErreur(null);
    try {
      await core.cloudProviderSelect(s.id, modele);
      window.dispatchEvent(new Event(ACTIVE_MODEL_CHANGED));
      setInfo(`La conversation utilise maintenant ${modele} sur ${s.nom}.`);
      await recharger();
    } catch (e) {
      setErreur(String(e));
    } finally {
      setOccupe(null);
    }
  }

  async function revenirIci() {
    setOccupe("local");
    setErreur(null);
    try {
      const fournisseurs = await core.listProviders();
      const local = fournisseurs.find((p) => p.kind === "local");
      if (!local) throw new Error("Aucun modèle n'est installé sur ce PC.");
      await core.setActiveProvider(local.id);
      window.dispatchEvent(new Event(ACTIVE_MODEL_CHANGED));
      setInfo("La conversation utilise de nouveau le modèle de ce PC.");
      await recharger();
    } catch (e) {
      setErreur(String(e));
    } finally {
      setOccupe(null);
    }
  }

  async function retirer(s: ServeurModeles) {
    setOccupe(s.id);
    setErreur(null);
    try {
      await core.serveurModelesRetirer(s.id);
      window.dispatchEvent(new Event(ACTIVE_MODEL_CHANGED));
      await recharger();
    } catch (e) {
      setErreur(String(e));
    } finally {
      setOccupe(null);
    }
  }

  const unActif = serveurs.some((s) => etats[s.id]?.actif);

  return (
    <div className="locaryn-field" style={{ marginTop: 28 }}>
      <div className="locaryn-field-label">Modèle d'un autre PC</div>
      <p className="locaryn-field-hint">
        Faites calculer le modèle par un PC plus puissant de votre réseau, en gardant tout le reste
        ici.
      </p>

      <div className="lo-autre-pc-schema">
        <div className="lo-autre-pc-cote">
          <span className="lo-autre-pc-titre">
            <Icon name="monitor" size={14} /> Sur ce PC
          </span>
          <span>La conversation, vos fichiers, le terminal, les MCP (Roblox Studio…)</span>
        </div>
        <Icon name="arrow-right" size={14} />
        <div className="lo-autre-pc-cote">
          <span className="lo-autre-pc-titre">
            <Icon name="cpu" size={14} /> Sur l'autre PC
          </span>
          <span>Seulement le calcul du modèle</span>
        </div>
      </div>

      {serveurs.map((s) => {
        const etat = etats[s.id];
        const valeur = choix[s.id] ?? etat?.actif ?? etat?.modeles[0]?.id ?? "";
        return (
          <div key={s.id} className="lo-autre-pc-carte">
            <div className="lo-autre-pc-entete">
              <div>
                <div className="lo-autre-pc-nom">
                  {s.nom}
                  {etat?.actif && <span className="lo-autre-pc-badge">utilisé</span>}
                </div>
                <div className="lo-autre-pc-url">{s.url}</div>
              </div>
              <button
                type="button"
                className="locaryn-btn-ghost"
                disabled={occupe !== null}
                onClick={() => void retirer(s)}
              >
                Retirer
              </button>
            </div>
            {!etat ? (
              <p className="locaryn-field-hint">Lecture de ses modèles…</p>
            ) : etat.erreur ? (
              <p className="locaryn-vp-error">{etat.erreur}</p>
            ) : etat.modeles.length === 0 ? (
              <p className="locaryn-field-hint">Aucun modèle installé sur ce PC.</p>
            ) : (
              <div className="lo-autre-pc-choix">
                <select
                  className="locaryn-select"
                  value={valeur}
                  disabled={occupe !== null}
                  onChange={(e) => setChoix((c) => ({ ...c, [s.id]: e.target.value }))}
                  aria-label={`Modèle de ${s.nom}`}
                >
                  {etat.modeles.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.name || m.id}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  className="locaryn-btn-primary"
                  disabled={occupe !== null || (etat.actif !== null && valeur === etat.actif)}
                  onClick={() => void utiliser(s)}
                >
                  {occupe === s.id
                    ? "…"
                    : etat.actif !== null && valeur === etat.actif
                      ? "En cours d'utilisation"
                      : "Utiliser pour la conversation"}
                </button>
              </div>
            )}
          </div>
        );
      })}

      {unActif && (
        <button
          type="button"
          className="locaryn-btn-ghost"
          style={{ marginTop: 8 }}
          disabled={occupe !== null}
          onClick={() => void revenirIci()}
        >
          Revenir au modèle de ce PC
        </button>
      )}

      {serveurs.length > 0 && (
        <p className="locaryn-field-hint" style={{ marginTop: 12 }}>
          <strong>Les deux modèles à la fois :</strong> gardez le modèle de ce PC pour la
          conversation, et demandez-lui de lancer un sous-agent sur l'autre (« lance un sous-agent
          sur {serveurs[0].nom} pour… »). Le sous-agent calcule là-bas, mais ses outils, et donc les
          MCP, travaillent ici.
        </p>
      )}

      <div className="lo-autre-pc-ajout">
        <div className="lo-autre-pc-titre">Ajouter un PC</div>
        <p className="locaryn-field-hint" style={{ margin: "4px 0 10px" }}>
          Sur l'autre PC : activez le mode serveur et créez un identifiant (Réglages → Serveur &
          fonctions). Son adresse s'y affiche.
        </p>
        <input
          className="locaryn-input"
          style={{ width: "100%" }}
          placeholder="Adresse, par ex. 192.168.0.57:7474"
          value={adresse}
          disabled={occupe !== null}
          onChange={(e) => setAdresse(e.target.value)}
          aria-label="Adresse de l'autre PC"
        />
        <div className="lo-autre-pc-identifiants">
          <input
            className="locaryn-input"
            placeholder="Identifiant"
            value={identifiant}
            disabled={occupe !== null}
            onChange={(e) => setIdentifiant(e.target.value)}
            aria-label="Identifiant sur l'autre PC"
          />
          <input
            className="locaryn-input"
            type="password"
            placeholder="Mot de passe"
            value={motDePasse}
            disabled={occupe !== null}
            onChange={(e) => setMotDePasse(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void ajouter()}
            aria-label="Mot de passe sur l'autre PC"
          />
        </div>
        <button
          type="button"
          className="locaryn-btn-primary"
          style={{ marginTop: 10 }}
          disabled={occupe !== null || !adresse.trim() || !identifiant.trim() || !motDePasse}
          onClick={() => void ajouter()}
        >
          {occupe === "ajout" ? "Connexion à l'autre PC…" : "Ajouter ce PC"}
        </button>
      </div>

      {info && <p className="locaryn-field-hint lo-autre-pc-info">{info}</p>}
      {erreur && <div className="locaryn-vp-error">{erreur}</div>}
    </div>
  );
}
