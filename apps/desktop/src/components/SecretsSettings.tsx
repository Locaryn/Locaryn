import { useCallback, useEffect, useState } from "react";
import { type SecretInfo, core } from "../lib/core";

/**
 * Le coffre : les mots de passe et jetons que le modèle utilise sans jamais
 * les voir. Ils vivent dans le trousseau du système ; on n'affiche ici que
 * leurs noms. Le modèle les demande lui-même (champ masqué dans la
 * conversation) ; on peut aussi les ranger ou les effacer d'ici.
 */
export function SecretsSettings() {
  const [liste, setListe] = useState<SecretInfo[] | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [nom, setNom] = useState("");
  const [libelle, setLibelle] = useState("");
  const [valeur, setValeur] = useState("");

  const charger = useCallback(() => {
    core
      .secretsLister()
      .then(setListe)
      .catch((e) => setErreur(String(e)));
  }, []);
  useEffect(charger, []);

  async function ajouter() {
    setErreur(null);
    try {
      await core.secretDefinir(nom.trim(), libelle.trim() || nom.trim(), valeur);
      setNom("");
      setLibelle("");
      setValeur("");
      charger();
    } catch (e) {
      setErreur(String(e));
    }
  }

  async function effacer(n: string) {
    setErreur(null);
    try {
      await core.secretEffacer(n);
      charger();
    } catch (e) {
      setErreur(String(e));
    }
  }

  return (
    <div className="locaryn-field locaryn-coffre">
      <div className="locaryn-field-label">Coffre des secrets</div>
      <p className="locaryn-field-hint">
        Mots de passe et jetons rangés dans le trousseau du système. Le modèle s'en sert par leur
        nom — <code>{"{{secret:nom}}"}</code> — sans jamais lire la valeur, et toute réponse qui la
        contiendrait est masquée avant de lui parvenir.
      </p>

      {liste && liste.length === 0 && (
        <p className="locaryn-field-hint locaryn-coffre-vide">Aucun secret enregistré.</p>
      )}
      {liste && liste.length > 0 && (
        <ul className="locaryn-coffre-liste">
          {liste.map((s) => (
            <li key={s.nom} className="locaryn-coffre-ligne">
              <span className="locaryn-coffre-texte">
                <span className="locaryn-coffre-libelle">{s.libelle || s.nom}</span>
                <code className="locaryn-coffre-nom">{s.nom}</code>
              </span>
              <span className="locaryn-coffre-valeur" aria-label="valeur masquée">
                ••••••••
              </span>
              <button
                type="button"
                className="locaryn-btn-ghost"
                onClick={() => void effacer(s.nom)}
              >
                Effacer
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className="locaryn-coffre-ajout">
        <input
          className="locaryn-input"
          placeholder="Nom (ex. wsl-sudo-Ubuntu)"
          value={nom}
          onChange={(e) => setNom(e.target.value)}
        />
        <input
          className="locaryn-input"
          placeholder="Libellé"
          value={libelle}
          onChange={(e) => setLibelle(e.target.value)}
        />
        <input
          className="locaryn-input"
          type="password"
          autoComplete="off"
          placeholder="Valeur"
          value={valeur}
          onChange={(e) => setValeur(e.target.value)}
        />
        <button
          type="button"
          className="locaryn-btn-primary"
          disabled={!nom.trim() || !valeur}
          onClick={() => void ajouter()}
        >
          Ranger
        </button>
      </div>
      {erreur && (
        <p className="locaryn-field-error" role="alert">
          {erreur}
        </p>
      )}
    </div>
  );
}
