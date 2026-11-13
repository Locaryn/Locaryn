import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { type UserLoginQr as LoginQr, core } from "../lib/core";
import { useModeAcces } from "../lib/modeAcces";

type Props = {
  userId: string;
  username: string;
  /** Le nom du mode d'accès choisi quand ce n'est pas le réseau local. */
  distant?: string | null;
  onClose: () => void;
};

/** Un nouveau code toutes les 2 min 30 ; le serveur garde le précédent
 *  3 minutes, le temps de finir un scan commencé juste avant le changement. */
const RENOUVELLEMENT_S = 150;
/** Le code suivant est demandé un peu avant la fin : le service peut mettre
 *  quelques secondes à le produire (adresse publique, box), et l'écran
 *  restait sur l'ancien code, compte à rebours déjà relancé. */
const AVANCE_S = 10;

/**
 * Le QR de connexion d'un compte : le téléphone qui le scanne sait quel
 * serveur joindre, puis demande le code à six caractères affiché ici. Le code
 * n'est pas dans le QR — une photo du QR seule n'ouvre rien. Il change à
 * chaque renouvellement et ne sert qu'une fois.
 *
 * Il porte l'adresse du mode d'accès choisi : celle que le panneau
 * d'appairage vient d'obtenir (Internet, tunnel), ou l'adresse locale. Les
 * deux valent quand le serveur sort du réseau local : un petit choix les
 * départage.
 */
export function UserLoginQr({ userId, username, distant, onClose }: Props) {
  const { transport } = useModeAcces();
  const [qr, setQr] = useState<LoginQr | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [reste, setReste] = useState(RENOUVELLEMENT_S);
  const deuxVoies = Boolean(distant && transport);
  const [voie, setVoie] = useState<"distant" | "local">("distant");
  const cible = deuxVoies && voie === "distant" ? transport : null;
  /** Le code suivant, déjà prêt, affiché d'un coup à la fin du décompte. */
  const prochain = useRef<LoginQr | null>(null);
  /** Le décompte, lu par la minuterie sans passer par un état React. */
  const resteRef = useRef(RENOUVELLEMENT_S);

  const demander = useCallback(
    () => core.userLoginQr(userId, cible?.mode ?? "local", cible?.url),
    [userId, cible?.mode, cible?.url],
  );

  const renouveler = useCallback(async () => {
    prochain.current = null;
    try {
      setQr(await demander());
      setErreur(null);
      resteRef.current = RENOUVELLEMENT_S;
      setReste(RENOUVELLEMENT_S);
    } catch (e) {
      setErreur(String(e).replace(/^Error:\s*/, ""));
    }
  }, [demander]);

  useEffect(() => {
    void renouveler();
  }, [renouveler]);

  // Le décompte : le code suivant est demandé en avance, puis affiché pile à
  // zéro. S'il n'est pas encore arrivé, on le demande et on l'attend.
  useEffect(() => {
    let vivant = true;
    const t = window.setInterval(() => {
      const r = resteRef.current;
      if (r === AVANCE_S + 1) {
        void demander()
          .then((suivant) => {
            if (vivant) prochain.current = suivant;
          })
          .catch((e: unknown) => {
            console.warn("code suivant non préparé, il sera demandé à la fin", e);
          });
      }
      if (r > 1) {
        resteRef.current = r - 1;
        setReste(r - 1);
        return;
      }
      const pret = prochain.current;
      prochain.current = null;
      resteRef.current = RENOUVELLEMENT_S;
      setReste(RENOUVELLEMENT_S);
      if (pret) {
        setQr(pret);
        setErreur(null);
      } else {
        void renouveler();
      }
    }, 1000);
    return () => {
      vivant = false;
      window.clearInterval(t);
    };
  }, [demander, renouveler]);

  useEffect(() => {
    const echap = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", echap);
    return () => window.removeEventListener("keydown", echap);
  }, [onClose]);

  return createPortal(
    <dialog open className="locaryn-qr-overlay" aria-label={`QR de connexion de ${username}`}>
      <button
        type="button"
        className="locaryn-qr-overlay-veil"
        aria-label="Fermer"
        onClick={onClose}
      />
      <div className="locaryn-qr-overlay-card">
        <div className="locaryn-qr-overlay-head">
          <div>
            <h3>Connexion de {username}</h3>
            <span className="locaryn-field-hint">Sans identifiant ni mot de passe</span>
          </div>
          <button type="button" className="locaryn-icon-btn" onClick={onClose} aria-label="Fermer">
            <Icon name="close" size={16} />
          </button>
        </div>
        {deuxVoies && (
          <div className="locaryn-segmented locaryn-login-qr-voies" role="radiogroup">
            {(
              [
                ["distant", distant],
                ["local", "Réseau local"],
              ] as const
            ).map(([id, label]) => (
              <button
                key={id}
                type="button"
                role="radio"
                aria-checked={voie === id}
                className={`locaryn-segment${voie === id ? " locaryn-segment-on" : ""}`}
                onClick={() => {
                  setQr(null);
                  setVoie(id);
                }}
              >
                {label}
              </button>
            ))}
          </div>
        )}
        {distant && !transport && (
          <p className="locaryn-field-hint">
            Le code {distant} n'est pas encore prêt : ce QR porte l'adresse du réseau local.
          </p>
        )}
        {qr?.qr_svg ? (
          <div
            className="locaryn-qr-overlay-code"
            // biome-ignore lint/security/noDangerouslySetInnerHtml: SVG produit par le service local, sans entrée extérieure
            dangerouslySetInnerHTML={{ __html: qr.qr_svg }}
          />
        ) : (
          <div className="locaryn-login-qr-attente">{erreur ?? "Préparation du code…"}</div>
        )}
        {qr && (
          <>
            <div className="locaryn-login-qr-code" aria-label="Code de connexion">
              {qr.code.split("").map((c, i) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: un caractère du code n'a pas d'autre identité que sa place.
                <span key={i}>{c}</span>
              ))}
            </div>
            <div className="locaryn-login-qr-temps" aria-hidden>
              <span style={{ width: `${(reste / RENOUVELLEMENT_S) * 100}%` }} />
            </div>
          </>
        )}
        <p className="locaryn-field-hint">
          Avec l'appareil photo du téléphone ou « Scanner un QR code » dans l'app Locaryn, puis saisir le code. Nouveau code dans{" "}
          {reste} s.
        </p>
        {erreur && qr && <p className="locaryn-vp-error">{erreur}</p>}
      </div>
    </dialog>,
    document.body,
  );
}
