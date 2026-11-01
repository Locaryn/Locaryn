import { useEffect, useState } from "react";
import {
  type CloudModel,
  type CloudProvider,
  type TacheDistante,
  type TachesDistantes,
  core,
} from "../../lib/core";

const VIDE: TachesDistantes = {
  memoire: { actif: false, modele: null },
  titres: { actif: false, modele: null },
};

/** Le modèle de la conversation : la valeur du sélecteur pour `modele: null`. */
const CELUI_DU_CHAT = "";

function Ligne({
  titre,
  explication,
  siActif,
  valeur,
  modeles,
  onChange,
}: {
  titre: string;
  explication: string;
  /** Ce que coûte l'interrupteur allumé. */
  siActif: string;
  valeur: TacheDistante;
  modeles: CloudModel[];
  onChange: (v: TacheDistante) => void;
}) {
  return (
    <div className="locaryn-bg-task">
      <div className="locaryn-bg-task-head">
        <div className="locaryn-bg-task-text">
          <strong>{titre}</strong>
          <p>{valeur.actif ? siActif : explication}</p>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={valeur.actif}
          aria-label={titre}
          className={`locaryn-switch${valeur.actif ? " locaryn-switch-on" : ""}`}
          onClick={() => onChange({ ...valeur, actif: !valeur.actif })}
        >
          <span className="locaryn-switch-knob" aria-hidden="true" />
        </button>
      </div>
      {valeur.actif && (
        <label className="locaryn-bg-task-model">
          <span>Avec</span>
          <select
            className="locaryn-select"
            value={valeur.modele ?? CELUI_DU_CHAT}
            onChange={(e) =>
              onChange({
                ...valeur,
                modele: e.target.value === CELUI_DU_CHAT ? null : e.target.value,
              })
            }
          >
            <option value={CELUI_DU_CHAT}>Le modèle de la conversation</option>
            {modeles.map((m) => (
              <option key={m.id} value={m.id}>
                {m.name || m.id}
              </option>
            ))}
          </select>
        </label>
      )}
    </div>
  );
}

/**
 * Les tâches de fond qu'un fournisseur a le droit de faire : trier la mémoire,
 * nommer les conversations. Coupées par défaut — elles consommeraient
 * l'abonnement ou les crédits pour des tâches qu'un modèle local fait très
 * bien. Coupées, un modèle local déjà chargé s'en charge ; sinon elles n'ont
 * pas lieu, et une conversation garde sa date pour titre.
 */
export function ProviderBackgroundTasks({ provider }: { provider: CloudProvider }) {
  const [reglages, setReglages] = useState<TachesDistantes | null>(null);
  const [modeles, setModeles] = useState<CloudModel[]>([]);
  const [erreur, setErreur] = useState<string | null>(null);

  useEffect(() => {
    let annule = false;
    core
      .tachesDistantes(provider.id)
      .then((r) => {
        if (!annule) setReglages(r);
      })
      .catch((e: unknown) => {
        console.warn("[Tâches de fond] réglages illisibles :", e);
        if (!annule) setReglages(VIDE);
      });
    core
      .cloudProviderModels(provider.id, false)
      .then((m) => {
        if (!annule) setModeles(m);
      })
      .catch((e: unknown) => console.warn("[Tâches de fond] catalogue illisible :", e));
    return () => {
      annule = true;
    };
  }, [provider.id]);

  if (!reglages) return null;

  const poser = (suite: TachesDistantes) => {
    const avant = reglages;
    setReglages(suite);
    setErreur(null);
    core.tachesDistantesEcrire(provider.id, suite).catch((e: unknown) => {
      setReglages(avant);
      setErreur(`Réglage non enregistré : ${String(e)}`);
    });
  };

  return (
    <section className="locaryn-provider-usage locaryn-bg-tasks" aria-label="Tâches de fond">
      <div className="locaryn-provider-usage-head">
        <strong>Tâches de fond</strong>
      </div>
      <Ligne
        titre={`Utilisation de ${provider.label} pour la classification mémoire`}
        explication="Coupé : la mémoire est triée par un modèle local déjà chargé, ou pas du tout. Rien n'est consommé sur ce compte."
        siActif="Activé : le tri de la mémoire passe par ce compte pendant les temps morts, et consomme son quota ou ses crédits."
        valeur={reglages.memoire}
        modeles={modeles}
        onChange={(memoire) => poser({ ...reglages, memoire })}
      />
      <Ligne
        titre={`Utilisation de ${provider.label} pour le nommage des conversations`}
        explication="Coupé : les conversations sont nommées par un modèle local déjà chargé ; sinon elles gardent leur date et leur heure de début."
        siActif="Activé : chaque nouvelle conversation est nommée par ce compte, une courte requête qui consomme son quota ou ses crédits."
        valeur={reglages.titres}
        modeles={modeles}
        onChange={(titres) => poser({ ...reglages, titres })}
      />
      {erreur && <p className="locaryn-usage-note">{erreur}</p>}
    </section>
  );
}
