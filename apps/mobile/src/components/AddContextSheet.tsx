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

/** Un texte lu sur place garde au plus ce nombre de caractères (comme le serveur). */
const TEXTE_MAX = 60_000;
/** Un document envoyé au serveur pour en tirer le texte pèse au plus ceci. */
const DOCUMENT_MAX = 40 * 1024 * 1024;

function lireUrl(f: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const lecteur = new FileReader();
    lecteur.onload = () => resolve(String(lecteur.result));
    lecteur.onerror = () => reject(lecteur.error ?? new Error(`« ${f.name} » illisible.`));
    lecteur.readAsDataURL(f);
  });
}

/** Les premiers octets disent si le fichier est du texte : un binaire porte des zéros. */
function porteDuTexte(octets: Uint8Array): boolean {
  const debut = octets.subarray(0, 8192);
  if (debut.includes(0)) return false;
  try {
    new TextDecoder("utf-8", { fatal: true }).decode(debut);
    return true;
  } catch {
    // Coupé au milieu d'un caractère, ou vraiment binaire : on retente sans la
    // fin, qui peut tomber au milieu d'une séquence UTF-8.
    try {
      new TextDecoder("utf-8", { fatal: true }).decode(debut.subarray(0, debut.length - 4));
      return true;
    } catch {
      return false;
    }
  }
}

function tronquer(texte: string): string {
  if (texte.length <= TEXTE_MAX) return texte;
  return `${texte.slice(0, TEXTE_MAX)}

[Document tronqué : seuls les ${TEXTE_MAX} premiers caractères ont été joints.]`;
}

/**
 * Lit un fichier joint. Une image part telle quelle vers le modèle ; un texte
 * est lu sur place ; un PDF (ou tout binaire) part au serveur, qui en tire le
 * texte — lu ici comme du texte, il ne donnait que des octets illisibles.
 */
async function lireFichier(f: File): Promise<PieceJointe> {
  const id = `${f.name}-${f.size}-${f.lastModified}`;
  if (f.type.startsWith("image/")) {
    const url = await lireUrl(f);
    return { id, nom: f.name, genre: "image", contenu: url.slice(url.indexOf(",") + 1), apercu: url };
  }
  const octets = new Uint8Array(await f.arrayBuffer());
  if (porteDuTexte(octets)) {
    return { id, nom: f.name, genre: "texte", contenu: tronquer(new TextDecoder("utf-8").decode(octets)) };
  }
  if (f.size > DOCUMENT_MAX) {
    throw new Error(
      `« ${f.name} » est trop lourd pour être lu (${Math.round(f.size / (1024 * 1024))} Mo, 40 Mo au plus).`,
    );
  }
  const url = await lireUrl(f);
  const { texte } = await api.extractDocument(f.name, url.slice(url.indexOf(",") + 1));
  return { id, nom: f.name, genre: "texte", contenu: texte };
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
  /** Le fichier en cours de lecture (un PDF part au serveur : ça prend un instant). */
  const [lecture, setLecture] = useState<string | null>(null);

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
      setLecture(f.name);
      try {
        pieces.push(await lireFichier(f));
      } catch (e) {
        onErreur(e instanceof Error ? e.message : String(e));
      }
    }
    setLecture(null);
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
            {lecture && (
              <p className="lo-ajout-lecture" role="status">
                <span className="lo-spinner" aria-hidden /> Lecture de « {lecture} »…
              </p>
            )}
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
