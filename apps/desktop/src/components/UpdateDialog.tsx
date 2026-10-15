import { Icon, LoProgress } from "@locaryn/ui-core";
import { open } from "@tauri-apps/plugin-shell";
import { type Update, check } from "@tauri-apps/plugin-updater";
import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { core, coreMode } from "../lib/core";
import { notifierSysteme } from "../lib/osNotify";

const RELEASES_URL = "https://github.com/Locaryn/Locaryn/releases";

/** L'evenement qu'un autre ecran envoie pour demander une verification. */
export const CHECK_UPDATE_EVENT = "locaryn:verifier-mise-a-jour";

/** La dernière version annoncée par une notification du système. */
const VERSION_ANNONCEE = "locaryn:maj-annoncee";

/** Une notification Windows (ou macOS) quand une version sort pendant que
 *  l'application tourne, souvent réduite dans la zone de notification : la
 *  fenêtre de mise à jour, seule, n'était vue que de qui l'avait ouverte.
 *  Une seule fois par version. */
function annoncerMiseAJour(version: string) {
  try {
    if (localStorage.getItem(VERSION_ANNONCEE) === version) return;
    localStorage.setItem(VERSION_ANNONCEE, version);
  } catch {
    // Sans stockage local, on annonce quand même : au pire deux fois.
  }
  void notifierSysteme(
    "Mise à jour de Locaryn disponible",
    `La version ${version} est prête. Ouvrez Locaryn pour l'installer.`,
    "maintenance",
  );
}

/** Demander une verification depuis n'importe quel ecran. */
export function demanderVerification() {
  window.dispatchEvent(new CustomEvent(CHECK_UPDATE_EVENT));
}

type Etat =
  | { sorte: "ferme" }
  | { sorte: "recherche" }
  | { sorte: "ajour"; version: string | null }
  | { sorte: "trouvee"; update: Update }
  | { sorte: "telecharge"; update: Update; recu: number; total: number | null }
  | { sorte: "posee"; update: Update }
  | { sorte: "erreur"; message: string };

function formatOctets(n: number): string {
  if (n < 1024) return `${n} o`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} ko`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} Mo`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} Go`;
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}

/**
 * Le plugin renvoie parfois une phrase très courte (« request failed ») alors
 * que la cause est utile pour la personne : GitHub bloqué, manifeste absent ou
 * signature invalide. On garde le détail, mais on donne d'abord une action.
 */
function updateErrorMessage(e: unknown): string {
  const raw = errorMessage(e)
    .replace(/^Error:\s*/i, "")
    .trim();
  // L'URL est retirée avant tout test : elle finit par « latest.json », donc
  // le motif qui cherchait « json » ou « manifest » l'y trouvait toujours. Une
  // panne réseau — « error sending request for url (…/latest.json) » — était
  // ainsi annoncée comme un manifeste absent, et envoyait corriger une chaîne
  // de publication qui n'avait rien.
  const cause = raw.replace(/https?:\/\/\S+/g, "").toLowerCase();
  if (/signature|public key|pubkey|verify|verification/.test(cause)) {
    return `Le manifeste GitHub est accessible, mais sa signature n'est pas acceptée. Installez la dernière version depuis GitHub, puis réessayez. (${raw})`;
  }
  // Le transport d'abord : une requête qui ne part pas ne dit rien du contenu.
  if (
    /network|connect|dns|timeout|timed out|tls|certificate|request failed|sending request|offline|unreachable|refused|proxy/.test(
      cause,
    )
  ) {
    return `GitHub est inaccessible depuis cette machine. Vérifiez la connexion, le proxy ou le pare-feu, puis réessayez. (${raw})`;
  }
  if (/404|not found|no such|missing/.test(cause)) {
    return `Le manifeste de mise à jour est introuvable sur GitHub. La release doit publier « latest.json ». (${raw})`;
  }
  return raw || "La vérification a échoué sans détail fourni par le système.";
}

/**
 * Les notes de version, decoupees en lignes.
 *
 * Ce que GitHub publie est du texte libre. On en fait des entrees de liste
 * plutot que d'inventer une structure : chaque ligne non vide devient une
 * ligne, les puces de tete sont retirees, et rien n'est ajoute.
 *
 * Deux choses que GitHub ajoute lui-même (`generate_release_notes: true`) ne
 * sont pas des notes de version au même sens que le reste, et ne doivent pas
 * recevoir la même rangée à puce qu'un correctif ou une fonctionnalité : la
 * ligne « **Full Changelog** » (extraite à part, voir `lienJournalComplet`)
 * et les mentions « a fait sa première contribution », qui n'apportent rien
 * dans une fenêtre déjà compacte.
 */
function notes(corps: string | undefined): string[] {
  if (!corps) return [];
  return corps
    .split(/\r?\n/)
    .map((l) => l.replace(/^\s*[-*•]\s*/, "").trim())
    .filter(
      (l) =>
        l.length > 0 &&
        !/^#{1,6}\s/.test(l) &&
        !/^\*\*full changelog\*\*/i.test(l) &&
        !/made their first contribution/i.test(l),
    )
    .slice(0, 12);
}

/**
 * L'adresse de comparaison que GitHub ajoute en pied de note. Extraite à
 * part pour devenir un lien discret plutôt qu'une rangée à puce parmi les
 * vraies notes de version.
 */
function lienJournalComplet(corps: string | undefined): string | null {
  const m = corps?.match(/\*\*Full Changelog\*\*:\s*(\S+)/i);
  return m ? m[1] : null;
}

/** Le pictogramme d'une note, devine par son premier mot. */
function pictoNote(ligne: string) {
  const l = ligne.toLowerCase();
  if (/^(fix|correctif|corrige|répare|repare|bug)/.test(l)) return "bug" as const;
  if (/^(feat|ajout|nouveau|nouvelle)/.test(l)) return "plus" as const;
  return "wrench" as const;
}

function dateCourte(brut: string | undefined): string | null {
  if (!brut) return null;
  const d = new Date(brut.replace(/\s\+\d{4}$/, ""));
  if (Number.isNaN(d.getTime())) return null;
  return d.toLocaleDateString("fr-FR");
}

/**
 * La fenetre de mise a jour.
 *
 * Elle ne s'affiche que lorsqu'il y a quelque chose a dire : une version
 * trouvee au demarrage, ou une verification demandee depuis les reglages. Une
 * recherche silencieuse qui ne trouve rien ne doit rien ouvrir.
 */
export function UpdateDialog() {
  const [etat, setEtat] = useState<Etat>({ sorte: "ferme" });
  const [version, setVersion] = useState<string | null>(null);
  const [supporte, setSupporte] = useState(false);
  const verifieRef = useRef(false);
  // Version x64 émulée sur un PC ARM (Snapdragon X) : la mise à jour vise
  // l'application ARM64 native, plutôt que de reconduire l'émulation à
  // chaque version.
  const emuleRef = useRef(false);

  const fermer = useCallback(() => setEtat({ sorte: "ferme" }), []);

  const verifier = useCallback(async (visible: boolean) => {
    // Hors Tauri il n'y a pas de binaire a remplacer, et le plugin updater
    // n'a pas de pont : l'appeler quand meme faisait remonter un
    // « Cannot read properties of undefined » a l'ecran, qui ne dit rien a
    // personne.
    if (coreMode !== "tauri") {
      if (visible) {
        setEtat({
          sorte: "erreur",
          message: "Mode aperçu navigateur : la mise à jour automatique est désactivée.",
        });
      }
      return;
    }
    if (visible) setEtat({ sorte: "recherche" });
    try {
      let update: Update | null = null;
      if (emuleRef.current) {
        try {
          update = await check({ target: "windows-aarch64" });
        } catch {
          // Pas de version ARM64 publiée pour cette version : on reste sur
          // la mise à jour habituelle.
          update = null;
        }
      }
      update ??= await check();
      if (!update) {
        setEtat(visible ? { sorte: "ajour", version: null } : { sorte: "ferme" });
        return;
      }
      setEtat({ sorte: "trouvee", update });
      if (!visible) annoncerMiseAJour(update.version);
    } catch (e) {
      setEtat(visible ? { sorte: "erreur", message: updateErrorMessage(e) } : { sorte: "ferme" });
    }
  }, []);

  // La plateforme decide si l'updater natif existe : Tauri ne le fournit que
  // sur Windows et macOS, et le mode navigateur n'a pas de binaire a remplacer.
  useEffect(() => {
    let annule = false;
    core
      .appInfo()
      .then((info) => {
        if (annule) return;
        setVersion(info.version ?? null);
        emuleRef.current = info.platform === "windows" && info.emulated === true;
        const ok = coreMode === "tauri" && info.platform !== "linux";
        setSupporte(ok);
        if (ok && !verifieRef.current) {
          verifieRef.current = true;
          void verifier(false);
        }
      })
      .catch(() => {
        // Sans information de plateforme, on ne lance rien de soi-meme : le
        // bouton des reglages reste la porte d'entree.
      });
    return () => {
      annule = true;
    };
  }, [verifier]);

  // Une application qui reste ouverte des jours apprend aussi qu'une version
  // est sortie : nouvelle vérification discrète toutes les trois heures, et
  // une notification du système si une version est trouvée.
  const etatRef = useRef(etat);
  etatRef.current = etat;
  useEffect(() => {
    if (!supporte) return;
    const t = setInterval(
      () => {
        // Jamais par-dessus un téléchargement ou une fenêtre déjà ouverte.
        if (etatRef.current.sorte === "ferme") void verifier(false);
      },
      3 * 60 * 60 * 1000,
    );
    return () => clearInterval(t);
  }, [supporte, verifier]);

  useEffect(() => {
    const surDemande = () => void verifier(true);
    window.addEventListener(CHECK_UPDATE_EVENT, surDemande);
    return () => window.removeEventListener(CHECK_UPDATE_EVENT, surDemande);
  }, [verifier]);

  useEffect(() => {
    if (etat.sorte === "ferme" || etat.sorte === "telecharge") return;
    const surTouche = (e: KeyboardEvent) => {
      if (e.key === "Escape") fermer();
    };
    window.addEventListener("keydown", surTouche);
    return () => window.removeEventListener("keydown", surTouche);
  }, [etat.sorte, fermer]);

  async function installer(update: Update) {
    setEtat({ sorte: "telecharge", update, recu: 0, total: null });
    // Le téléchargement annonce chaque morceau reçu — des milliers par
    // seconde. Un rendu par morceau relançait la transition de la barre à
    // chaque fois : elle tremblait, sautait, repartait. On compte ici et on
    // n'affiche que dix fois par seconde, comme sur le téléphone.
    let recu = 0;
    let total: number | null = null;
    let dernier = 0;
    const publier = (force = false) => {
      const maintenant = performance.now();
      if (!force && maintenant - dernier < 100) return;
      dernier = maintenant;
      setEtat((s) => (s.sorte === "telecharge" ? { ...s, recu, total: total ?? s.total } : s));
    };
    try {
      await update.downloadAndInstall((ev) => {
        if (ev.event === "Started") {
          total = ev.data.contentLength ?? null;
          publier(true);
        } else if (ev.event === "Progress") {
          recu += ev.data.chunkLength;
          publier();
        } else if (ev.event === "Finished") {
          if (total) recu = total;
          publier(true);
        }
      });
      setEtat({ sorte: "posee", update });
    } catch (e) {
      setEtat({ sorte: "erreur", message: updateErrorMessage(e) });
    }
  }

  if (etat.sorte === "ferme") return null;

  const update =
    etat.sorte === "trouvee" || etat.sorte === "telecharge" || etat.sorte === "posee"
      ? etat.update
      : null;
  const lignes = update ? notes(update.body) : [];
  const journalComplet = update ? lienJournalComplet(update.body) : null;
  const enCours = etat.sorte === "telecharge";
  const fraction =
    etat.sorte === "telecharge" && etat.total ? Math.min(1, etat.recu / etat.total) : null;

  return createPortal(
    <div className="locaryn-maj-layer">
      <button
        type="button"
        className="locaryn-maj-scrim"
        aria-label="Fermer"
        onClick={enCours ? undefined : fermer}
        disabled={enCours}
      />
      <div className="locaryn-maj" role="dialog" aria-modal="true" aria-label="Mise à jour">
        <header className="locaryn-maj-head">
          <span className="locaryn-maj-icon" aria-hidden="true">
            <Icon name="refresh" size={21} />
          </span>
          <div className="locaryn-maj-ident">
            <h2>Mise à jour</h2>
            {version && (
              <span className="locaryn-maj-hint">Vous utilisez la version {version}</span>
            )}
          </div>
          {!enCours && (
            <button type="button" className="locaryn-maj-close" title="Fermer" onClick={fermer}>
              <Icon name="close" size={15} />
            </button>
          )}
        </header>

        <div className="locaryn-maj-body">
          {etat.sorte === "recherche" && <p className="locaryn-maj-texte">Recherche en cours…</p>}

          {etat.sorte === "ajour" && (
            <p className="locaryn-maj-texte">
              Locaryn est à jour{version ? ` en version ${version}` : ""}.
            </p>
          )}

          {etat.sorte === "erreur" && (
            <p className="locaryn-maj-texte locaryn-maj-erreur">{etat.message}</p>
          )}

          {etat.sorte === "posee" && (
            <p className="locaryn-maj-texte">
              La version {etat.update.version} est prête. Redémarrez pour l'appliquer.
            </p>
          )}

          {etat.sorte === "trouvee" && (
            <p className="locaryn-maj-texte">
              La version {etat.update.version} est disponible.
              {lignes.length === 0 ? " Aucune note de version publiée." : ""}
            </p>
          )}

          {enCours && (
            <div className="locaryn-maj-progres">
              <LoProgress value={fraction} />
              <span className="locaryn-maj-progres-label">
                {etat.total
                  ? `${formatOctets(etat.recu)} / ${formatOctets(etat.total)}`
                  : formatOctets(etat.recu)}
              </span>
            </div>
          )}

          {update && lignes.length > 0 && (
            <div className="locaryn-maj-groupe">
              <div className="locaryn-maj-groupe-head">
                <span>{update.version}</span>
                {dateCourte(update.date) && <span>{dateCourte(update.date)}</span>}
              </div>
              <div className="locaryn-maj-notes">
                {lignes.map((l) => (
                  <div key={l} className="locaryn-maj-note">
                    <Icon name={pictoNote(l)} size={15} />
                    <span>{l}</span>
                  </div>
                ))}
              </div>
              {journalComplet && (
                <button
                  type="button"
                  className="locaryn-maj-changelog"
                  onClick={() => void open(journalComplet)}
                >
                  Voir le journal complet des modifications
                  <Icon name="chevron" size={13} />
                </button>
              )}
            </div>
          )}
        </div>

        <footer className="locaryn-maj-foot">
          {etat.sorte === "trouvee" ? (
            <>
              <button type="button" className="locaryn-btn-ghost" onClick={fermer}>
                <Icon name="clock" size={15} /> Plus tard
              </button>
              <button
                type="button"
                className="locaryn-btn-primary"
                onClick={() => void installer(etat.update)}
              >
                <Icon name="download" size={15} /> Télécharger
              </button>
            </>
          ) : enCours ? (
            <span className="locaryn-maj-attente">Téléchargement en cours…</span>
          ) : (
            <>
              {!supporte && (
                <button
                  type="button"
                  className="locaryn-btn-ghost"
                  onClick={() => void open(RELEASES_URL)}
                >
                  <Icon name="marketplace" size={15} /> Voir les versions
                </button>
              )}
              <button type="button" className="locaryn-btn-primary" onClick={fermer}>
                <Icon name="check" size={15} /> Fermer
              </button>
            </>
          )}
        </footer>
      </div>
    </div>,
    document.body,
  );
}
