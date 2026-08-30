import { Icon, LoProgress } from "@locaryn/ui-core";
import { open } from "@tauri-apps/plugin-shell";
import { type Update, check } from "@tauri-apps/plugin-updater";
import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { core, coreMode } from "../lib/core";

const RELEASES_URL = "https://github.com/Locaryn/Locaryn/releases";

/** L'evenement qu'un autre ecran envoie pour demander une verification. */
export const CHECK_UPDATE_EVENT = "locaryn:verifier-mise-a-jour";

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
 */
function notes(corps: string | undefined): string[] {
  if (!corps) return [];
  return corps
    .split(/\r?\n/)
    .map((l) => l.replace(/^\s*[-*•]\s*/, "").trim())
    .filter((l) => l.length > 0 && !/^#{1,6}\s/.test(l))
    .slice(0, 12);
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
      const update = await check();
      if (!update) {
        setEtat(visible ? { sorte: "ajour", version: null } : { sorte: "ferme" });
        return;
      }
      setEtat({ sorte: "trouvee", update });
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
    try {
      await update.downloadAndInstall((ev) => {
        if (ev.event === "Started") {
          setEtat((s) =>
            s.sorte === "telecharge" ? { ...s, total: ev.data.contentLength ?? s.total } : s,
          );
        } else if (ev.event === "Progress") {
          setEtat((s) =>
            s.sorte === "telecharge" ? { ...s, recu: s.recu + ev.data.chunkLength } : s,
          );
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
