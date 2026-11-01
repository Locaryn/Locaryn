// Les demandes d'autorisation des pages (micro, caméra, position…), posées
// par l'app plutôt que par la bulle de Windows. Une extension (le dictaphone)
// comme un site ouvert dans le navigateur intégré passent par ici.

import { Icon, type IconName } from "@locaryn/ui-core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { type DemandePermissionWeb, core, coreMode } from "../lib/core";
import { ModalShell } from "./ModalShell";

const EVENEMENT = "web-permission-request";

type Texte = { icone: IconName; quoi: string; pourquoi: string };

/** Ce que chaque autorisation permet, sans jargon. */
const TEXTES: Record<string, Texte> = {
  microphone: {
    icone: "microphone",
    quoi: "utiliser votre micro",
    pourquoi: "Le son n'est capté que pendant un enregistrement, et il reste sur cet ordinateur.",
  },
  camera: {
    icone: "video-camera",
    quoi: "utiliser votre caméra",
    pourquoi: "L'image n'est prise que tant que la page s'en sert.",
  },
  geolocation: {
    icone: "crosshair",
    quoi: "connaître votre position",
    pourquoi: "La page reçoit l'endroit approximatif où se trouve cet ordinateur.",
  },
  notifications: {
    icone: "bell",
    quoi: "vous envoyer des notifications",
    pourquoi: "Des messages pourront s'afficher dans le coin de l'écran.",
  },
  clipboard: {
    icone: "copy",
    quoi: "lire ce que vous avez copié",
    pourquoi: "La page verra le dernier texte ou la dernière image copiés.",
  },
  sensors: {
    icone: "gauge",
    quoi: "lire les capteurs de l'appareil",
    pourquoi: "Mouvement, orientation ou lumière, selon l'appareil.",
  },
  downloads: {
    icone: "download-simple",
    quoi: "télécharger plusieurs fichiers d'affilée",
    pourquoi: "Les fichiers arrivent dans votre dossier de téléchargements.",
  },
  files: {
    icone: "folder-open",
    quoi: "modifier des fichiers que vous choisissez",
    pourquoi: "Seuls les fichiers ou dossiers que vous lui ouvrez sont concernés.",
  },
  fonts: {
    icone: "text-aa",
    quoi: "voir les polices installées",
    pourquoi: "La liste des polices de cet ordinateur.",
  },
  midi: {
    icone: "music-notes",
    quoi: "piloter vos appareils MIDI",
    pourquoi: "Claviers et contrôleurs de musique branchés à cet ordinateur.",
  },
  windows: {
    icone: "monitor",
    quoi: "placer des fenêtres sur vos écrans",
    pourquoi: "La page connaîtra la disposition de vos écrans.",
  },
};

const AUTRE: Texte = {
  icone: "shield-check",
  quoi: "accéder à une fonction de l'appareil",
  pourquoi: "Refusez si vous ne savez pas pourquoi elle le demande.",
};

/** `https://www.exemple.fr` → `www.exemple.fr`. */
function nomDuSite(origine: string): string {
  try {
    return new URL(origine).host;
  } catch {
    return origine;
  }
}

export function WebPermissionPrompt() {
  const [file, setFile] = useState<DemandePermissionWeb[]>([]);
  const [retenir, setRetenir] = useState(true);
  const [envoi, setEnvoi] = useState(false);

  useEffect(() => {
    if (coreMode !== "tauri") return;
    let fini = false;
    let arret: (() => void) | undefined;
    void listen<DemandePermissionWeb>(EVENEMENT, (e) => {
      setFile((f) => (f.some((d) => d.id === e.payload.id) ? f : [...f, e.payload]));
    })
      .then((stop) => {
        if (fini) stop();
        else arret = stop;
      })
      .catch((err: unknown) => {
        console.error("demandes d'autorisation non écoutées", err);
      });
    return () => {
      fini = true;
      arret?.();
    };
  }, []);

  const demande = file[0] ?? null;

  const repondre = useCallback(
    async (autoriser: boolean, garder: boolean) => {
      if (!demande) return;
      setEnvoi(true);
      try {
        await core.repondrePermissionWeb(demande.id, autoriser, garder);
      } catch (err: unknown) {
        console.error("réponse d'autorisation non transmise", err);
      } finally {
        setEnvoi(false);
        setRetenir(true);
        setFile((f) => f.filter((d) => d.id !== demande.id));
      }
    },
    [demande],
  );

  if (!demande) return null;
  const texte = TEXTES[demande.kind] ?? AUTRE;
  const qui = demande.origin ? nomDuSite(demande.origin) : "Une extension de Locaryn";
  const portee = demande.origin ? "pour ce site" : "pour Locaryn";

  return (
    <ModalShell
      onClose={() => void repondre(false, false)}
      className="locaryn-card locaryn-permission-web"
      label="Demande d'autorisation"
      role="alertdialog"
    >
      <div className="locaryn-gate-head">
        <Icon name={texte.icone} size={18} />
        <h3>
          <span className="locaryn-permission-web-qui">{qui}</span> veut {texte.quoi}
        </h3>
      </div>
      <p className="locaryn-permission-web-pourquoi">{texte.pourquoi}</p>
      <label className="locaryn-permission-web-retenir">
        <input
          type="checkbox"
          checked={retenir}
          disabled={envoi}
          onChange={(e) => setRetenir(e.target.checked)}
        />
        Retenir ma réponse {portee}
      </label>
      <div className="locaryn-gate-actions">
        <button
          type="button"
          className="locaryn-btn-ghost"
          disabled={envoi}
          onClick={() => void repondre(false, retenir)}
        >
          Refuser
        </button>
        <button
          type="button"
          className="locaryn-btn-primary"
          disabled={envoi}
          autoFocus
          onClick={() => void repondre(true, retenir)}
        >
          Autoriser
        </button>
      </div>
      {file.length > 1 && (
        <p className="locaryn-field-hint">
          {file.length - 1} autre{file.length > 2 ? "s" : ""} demande
          {file.length > 2 ? "s" : ""} en attente
        </p>
      )}
    </ModalShell>
  );
}
