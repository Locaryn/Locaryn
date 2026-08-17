import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";
import { type ProgressionTelechargement, type UpdateStatus, api, coreMode } from "../lib/core";

/**
 * La version installée, et la mise à jour quand il y en a une.
 *
 * Cet écran doit être atteignable **avant** toute connexion. Le cas est
 * concret : le serveur passe à une version que le téléphone ne sait pas encore
 * parler, la connexion échoue, et si la mise à jour n'était accessible qu'une
 * fois connecté, il n'y aurait aucune façon d'en sortir.
 *
 * Rien ici ne renvoie vers une page web. L'application dit d'où l'on part et
 * où l'on va, ce que la version apporte, puis télécharge et ouvre
 * l'installateur. Android installe et vérifie la signature — cette part-là
 * n'est pas négociable, et c'est très bien ainsi.
 *
 * Un seul geste suffit. Si le droit d'installer manque, l'écran qui l'accorde
 * s'ouvre tout seul, et le retour au premier plan relance l'installation sans
 * retélécharger : la personne appuie une fois, pas trois.
 */
export function VersionSection() {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [phase, setPhase] = useState<"repos" | "verifie" | "telecharge" | "installe">("repos");
  const [error, setError] = useState<string | null>(null);
  /** L'avancement du téléchargement en cours, pour la barre. */
  const [progression, setProgression] = useState<ProgressionTelechargement | null>(null);
  /** Vrai dès qu'une installation a été demandée dans cette session. */
  const demande = useRef(false);

  const verifier = useCallback(async () => {
    setBusy(true);
    setPhase("verifie");
    setError(null);
    try {
      setStatus(await api.checkUpdate());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setPhase("repos");
    }
  }, []);

  useEffect(() => {
    void verifier();
  }, [verifier]);

  /**
   * Un seul geste, quel que soit l'état.
   *
   * Paquet absent : on télécharge puis on ouvre l'installateur. Paquet déjà
   * complet — le cas d'une installation refusée faute d'autorisation — on
   * relance l'installateur sans reprendre les trente mégaoctets.
   */
  const installer = useCallback(async (etat: UpdateStatus) => {
    if (!etat.download_url) return;
    demande.current = true;
    setBusy(true);
    setError(null);
    try {
      if (etat.downloaded) {
        setPhase("installe");
        await api.resumeInstall(etat.download_url);
      } else {
        setPhase("telecharge");
        setProgression(null);
        await api.installUpdate(etat.download_url, etat.size, setProgression);
      }
      setStatus({ ...etat, downloaded: true });
    } catch (e) {
      setError(String(e));
      // Le téléchargement a pu réussir alors que seul le lancement a
      // échoué : on relit l'état du disque plutôt que de le supposer, pour
      // que le bouton propose de reprendre et non de tout retélécharger.
      try {
        setStatus(await api.checkUpdate());
      } catch {
        /* l'erreur affichée reste la vraie, celle de l'installation */
      }
    } finally {
      setBusy(false);
      setPhase("repos");
      setProgression(null);
    }
  }, []);

  /**
   * Revenir au premier plan relance l'installation.
   *
   * C'est le retour de l'écran d'autorisation : la personne vient d'accorder
   * le droit d'installer, et lui redemander d'appuyer sur un bouton ne sert à
   * rien. Le paquet est déjà là, donc rien n'est retéléchargé — et cela ne se
   * déclenche que si une installation a été demandée avant.
   */
  useEffect(() => {
    if (coreMode !== "tauri") return;
    function auRetour() {
      if (document.visibilityState !== "visible") return;
      if (!demande.current || busy) return;
      void (async () => {
        const frais = await api.checkUpdate().catch(() => null);
        if (!frais?.available || !frais.downloaded) return;
        setStatus(frais);
        void installer(frais);
      })();
    }
    document.addEventListener("visibilitychange", auRetour);
    // La vue web d'Android ne signale pas toujours son retour au premier plan.
    // La fenêtre, elle, le sait : c'est elle qui reçoit le focus.
    const fenetre = getCurrentWindow()
      .onFocusChanged(({ payload }) => {
        if (payload) auRetour();
      })
      .catch(() => null);
    return () => {
      document.removeEventListener("visibilitychange", auRetour);
      void fenetre.then((stop) => stop?.());
    };
  }, [busy, installer]);

  const etat = (() => {
    if (phase === "telecharge") {
      return progression?.percentage != null
        ? `Téléchargement… ${progression.percentage} %`
        : "Téléchargement…";
    }
    if (phase === "installe") return "Ouverture de l'installateur…";
    if (busy && !status) return "Vérification…";
    if (!status) return "Version inconnue";
    if (status.error) return status.error;
    if (status.available) return `Nouvelle version : ${status.latest}`;
    if (status.latest) return "À jour";
    return "Impossible de joindre le serveur de mises à jour";
  })();

  const poids =
    status?.size && status.size > 0 ? `${Math.round(status.size / (1024 * 1024))} Mo` : null;

  return (
    <section className="lo-section">
      <h2 className="lo-section-title">Version</h2>

      <div className="lo-card">
        <div className="lo-card-text">
          <span className="lo-card-title">Locaryn {status?.current ?? ""}</span>
          <span className="lo-hint">{etat}</span>
        </div>
        <div className="lo-card-actions">
          <button type="button" className="lo-btn-small" disabled={busy} onClick={verifier}>
            {busy ? "…" : "Vérifier"}
          </button>
        </div>
      </div>

      {status?.available && status.download_url && (
        <>
          <div className="lo-update">
            <p className="lo-update-jump">
              {status.current} <span className="lo-update-arrow">→</span> {status.latest}
              {poids && <span className="lo-hint"> · {poids}</span>}
            </p>
            {status.notes && <p className="lo-update-notes">{status.notes}</p>}
            {phase === "telecharge" && progression && (
              <>
                {/* Le pourcentage vit déjà dans le texte (« Téléchargement…
                    45 % ») et dans la ligne des tailles : la barre est
                    décorative, rien à annoncer de plus. */}
                <div
                  className={`lo-progress${progression.percentage == null ? " lo-progress-indeterminate" : ""}`}
                  aria-hidden="true"
                >
                  <div
                    className="lo-progress-fill"
                    style={
                      progression.percentage == null
                        ? undefined
                        : { width: `${progression.percentage}%` }
                    }
                  />
                </div>
                {progression.total != null && progression.total > 0 && (
                  <p className="lo-hint lo-progress-sizes">
                    {Math.round(progression.downloaded / (1024 * 1024))} Mo sur{" "}
                    {Math.round(progression.total / (1024 * 1024))} Mo
                  </p>
                )}
              </>
            )}
          </div>

          <button
            type="button"
            className="lo-btn"
            disabled={busy}
            onClick={() => void installer(status)}
          >
            {busy
              ? phase === "telecharge"
                ? "Téléchargement…"
                : "Ouverture…"
              : status.downloaded
                ? "Reprendre l'installation"
                : `Installer la version ${status.latest}`}
          </button>

          <p className="lo-hint">
            {status.downloaded
              ? "Le paquet est déjà téléchargé : reprendre n'en télécharge pas un second."
              : "Android affichera son écran d'installation et vérifiera la signature."}
          </p>
        </>
      )}

      {error && <p className="lo-error">{error}</p>}
    </section>
  );
}
