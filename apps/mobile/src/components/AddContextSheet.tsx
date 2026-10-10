import { Icon, type IconName, TRUST_LEVELS, trustInfo } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { type SessionTrust, api } from "../lib/core";
import { useCoucheRetour } from "../lib/navigation";

/** Une pièce jointe prête à partir. */
export type PieceJointe = {
  id: string;
  nom: string;
  /** `image` part en base64 vers le modèle de vision ; `texte` dans le message. */
  genre: "image" | "texte";
  /** Image : base64 sans préfixe. Texte : le contenu lu. */
  contenu: string;
  /** Image : l'URL de la miniature. */
  apercu?: string;
};

type Props = {
  ouvert: boolean;
  onFermer: () => void;
  conversationId: string | null;
  onJoindre: (pieces: PieceJointe[]) => void;
  onErreur: (message: string) => void;
  /** Conversation pas encore commencée : le niveau choisi, posé à sa création. */
  niveauAvant: string | null;
  onNiveauAvant: (niveau: string | null) => void;
};

/** Un texte se joint jusqu'à cette taille : au-delà, il étoufferait la fenêtre. */
const TEXTE_MAX = 200_000;

function lireFichier(f: File): Promise<PieceJointe> {
  return new Promise((resolve, reject) => {
    const lecteur = new FileReader();
    const id = `${f.name}-${f.size}-${f.lastModified}`;
    if (f.type.startsWith("image/")) {
      lecteur.onload = () => {
        const url = String(lecteur.result);
        resolve({
          id,
          nom: f.name,
          genre: "image",
          contenu: url.slice(url.indexOf(",") + 1),
          apercu: url,
        });
      };
      lecteur.readAsDataURL(f);
    } else {
      if (f.size > TEXTE_MAX) {
        reject(
          new Error(
            `« ${f.name} » est trop grand pour être joint (${Math.round(f.size / 1024)} Ko).`,
          ),
        );
        return;
      }
      lecteur.onload = () =>
        resolve({ id, nom: f.name, genre: "texte", contenu: String(lecteur.result) });
      lecteur.readAsText(f);
    }
    lecteur.onerror = () => reject(lecteur.error ?? new Error(`« ${f.name} » illisible.`));
  });
}

/**
 * « + » du composeur : joindre une photo prise sur le moment, une image de la
 * galerie, un fichier, ou changer la permission de la conversation — le panneau
 * du bas que l'on attend d'une application de chat sur téléphone.
 */
export function AddContextSheet({
  ouvert,
  onFermer,
  conversationId,
  onJoindre,
  onErreur,
  niveauAvant,
  onNiveauAvant,
}: Props) {
  const camera = useRef<HTMLInputElement>(null);
  const galerie = useRef<HTMLInputElement>(null);
  const fichiers = useRef<HTMLInputElement>(null);
  const [permission, setPermission] = useState<SessionTrust | null>(null);
  const [niveaux, setNiveaux] = useState(false);

  useCoucheRetour(ouvert, onFermer);

  useEffect(() => {
    if (!ouvert || !conversationId) {
      setPermission(null);
      return;
    }
    let annule = false;
    api
      .sessionTrust(conversationId)
      .then((p) => {
        if (!annule) setPermission(p);
      })
      .catch((e) => console.warn("permission illisible :", e));
    return () => {
      annule = true;
    };
  }, [ouvert, conversationId]);

  useEffect(() => {
    if (!ouvert) setNiveaux(false);
  }, [ouvert]);

  async function choisis(liste: FileList | null) {
    if (!liste || liste.length === 0) return;
    const pieces: PieceJointe[] = [];
    for (const f of Array.from(liste)) {
      try {
        pieces.push(await lireFichier(f));
      } catch (e) {
        onErreur(e instanceof Error ? e.message : String(e));
      }
    }
    if (pieces.length) onJoindre(pieces);
    onFermer();
  }

  async function poser(niveau: string) {
    // Pas encore de conversation : le choix attend sa création.
    if (!conversationId) {
      onNiveauAvant(niveau);
      setNiveaux(false);
      return;
    }
    try {
      setPermission(await api.setSessionTrust(conversationId, niveau));
      setNiveaux(false);
    } catch (e) {
      onErreur(String(e));
    }
  }

  if (!ouvert) return null;
  const effectif = conversationId ? (permission?.effective ?? null) : niveauAvant;
  const actuel = effectif ? trustInfo(effectif as never) : null;
  const tuiles: { icone: IconName; libelle: string; cible: React.RefObject<HTMLInputElement> }[] = [
    { icone: "image", libelle: "Appareil photo", cible: camera },
    { icone: "image", libelle: "Photos", cible: galerie },
    { icone: "download", libelle: "Fichiers", cible: fichiers },
  ];

  return (
    <div className="lo-ajout-voile" role="presentation" onClick={onFermer}>
      <div
        className="lo-ajout"
        role="dialog"
        aria-modal="true"
        aria-label="Ajouter du contexte"
        onClick={(e) => e.stopPropagation()}
      >
        <span className="lo-ajout-poignee" aria-hidden />
        <div className="lo-ajout-tete">
          <button type="button" className="lo-ajout-fermer" onClick={onFermer} aria-label="Fermer">
            <Icon name="close" size={20} />
          </button>
          <h2>{niveaux ? "Autorisation" : "Ajouter du contexte"}</h2>
        </div>

        {niveaux ? (
          <ul className="lo-ajout-niveaux">
            {TRUST_LEVELS.map((n) => (
              <li key={n.value}>
                <button
                  type="button"
                  className={`lo-ajout-niveau${effectif === n.value ? " is-on" : ""}`}
                  onClick={() => void poser(n.value)}
                >
                  <span className="lo-ajout-pastille" style={{ background: n.color }} />
                  <span className="lo-ajout-niveau-texte">
                    <strong>{n.label}</strong>
                    <small>{n.hint}</small>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <>
            <div className="lo-ajout-tuiles">
              {tuiles.map((t) => (
                <button
                  key={t.libelle}
                  type="button"
                  className="lo-ajout-tuile"
                  onClick={() => t.cible.current?.click()}
                >
                  <Icon name={t.icone} size={22} />
                  <span>{t.libelle}</span>
                </button>
              ))}
            </div>
            <button type="button" className="lo-ajout-ligne" onClick={() => setNiveaux(true)}>
              <span className="lo-ajout-ligne-icone">
                <Icon name="shield" size={20} />
              </span>
              <span className="lo-ajout-ligne-texte">
                <strong>Autorisation</strong>
                <small>
                  {actuel?.label ??
                    (conversationId ? "…" : "Celle du projet — à choisir avant de commencer")}
                </small>
              </span>
              <Icon name="chevron" size={18} />
            </button>
          </>
        )}

        <input
          ref={camera}
          type="file"
          accept="image/*"
          capture="environment"
          hidden
          onChange={(e) => void choisis(e.target.files)}
        />
        <input
          ref={galerie}
          type="file"
          accept="image/*"
          multiple
          hidden
          onChange={(e) => void choisis(e.target.files)}
        />
        <input
          ref={fichiers}
          type="file"
          multiple
          hidden
          onChange={(e) => void choisis(e.target.files)}
        />
      </div>
    </div>
  );
}
