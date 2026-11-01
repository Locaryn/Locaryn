import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type PermissionWebRetenue, core } from "../lib/core";

/** Les noms des autorisations, tels que la fenêtre de demande les dit. */
const NOMS: Record<string, string> = {
  microphone: "Micro",
  camera: "Caméra",
  geolocation: "Position",
  notifications: "Notifications",
  clipboard: "Presse-papiers",
  sensors: "Capteurs",
  downloads: "Téléchargements multiples",
  files: "Fichiers",
  fonts: "Polices",
  midi: "Appareils MIDI",
  windows: "Placement des fenêtres",
};

/** Les réponses retenues aux demandes des pages (micro, caméra…). */
export function WebPermissionsSettings() {
  const [liste, setListe] = useState<PermissionWebRetenue[] | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);

  const relire = useCallback(() => {
    core
      .permissionsWebRetenues()
      .then((l) => {
        setListe(l);
        setErreur(null);
      })
      .catch((e: unknown) => setErreur(String(e)));
  }, []);

  useEffect(() => {
    relire();
  }, [relire]);

  async function oublier(origin?: string, kind?: string) {
    try {
      await core.permissionsWebOublier(origin, kind);
    } catch (e: unknown) {
      setErreur(String(e));
    }
    relire();
  }

  return (
    <section className="locaryn-model-preference-card">
      <div className="locaryn-model-preference-heading">
        <Icon name="microphone" size={17} />
        <div>
          <h4>Micro, caméra et autres accès</h4>
          <p>
            Les réponses que vous avez demandé de retenir. Oubliez-en une pour que la question soit
            reposée la prochaine fois.
          </p>
        </div>
      </div>
      {erreur && <p className="locaryn-field-hint">{erreur}</p>}
      {liste && liste.length === 0 && (
        <p className="locaryn-field-hint">Aucune réponse retenue pour l'instant.</p>
      )}
      {liste && liste.length > 0 && (
        <>
          <ul className="locaryn-web-perm-list">
            {liste.map((d) => (
              <li key={`${d.origin}|${d.kind}`}>
                <span className="locaryn-web-perm-qui">{d.origin || "Locaryn"}</span>
                <span>{NOMS[d.kind] ?? d.kind}</span>
                <span className={d.allow ? "locaryn-web-perm-oui" : "locaryn-web-perm-non"}>
                  {d.allow ? "Autorisé" : "Refusé"}
                </span>
                <button
                  type="button"
                  className="locaryn-btn-ghost"
                  onClick={() => void oublier(d.origin, d.kind)}
                >
                  Oublier
                </button>
              </li>
            ))}
          </ul>
          <button type="button" className="locaryn-btn-ghost" onClick={() => void oublier()}>
            Tout oublier
          </button>
        </>
      )}
    </section>
  );
}
