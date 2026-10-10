import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { type UserLoginQr as LoginQr, core } from "../lib/core";

type Props = {
  userId: string;
  username: string;
  onClose: () => void;
};

/** Un nouveau code toutes les 60 s ; le serveur garde le précédent 90 s. */
const RENOUVELLEMENT_S = 60;

/**
 * Le QR de connexion d'un compte, à la manière d'une télé qu'on relie à son
 * téléphone : on scanne, la session de ce compte s'ouvre, sans identifiant ni
 * mot de passe. Le code change toutes les minutes et ne sert qu'une fois ; il
 * s'affiche aussi en clair pour une saisie à la main.
 */
export function UserLoginQr({ userId, username, onClose }: Props) {
  const [qr, setQr] = useState<LoginQr | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [reste, setReste] = useState(RENOUVELLEMENT_S);

  const renouveler = useCallback(async () => {
    try {
      setQr(await core.userLoginQr(userId));
      setErreur(null);
      setReste(RENOUVELLEMENT_S);
    } catch (e) {
      setErreur(String(e).replace(/^Error:\s*/, ""));
    }
  }, [userId]);

  useEffect(() => {
    void renouveler();
  }, [renouveler]);

  useEffect(() => {
    const t = window.setInterval(() => {
      setReste((r) => {
        if (r <= 1) {
          void renouveler();
          return RENOUVELLEMENT_S;
        }
        return r - 1;
      });
    }, 1000);
    return () => window.clearInterval(t);
  }, [renouveler]);

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
          Sur le téléphone, ouvrez Locaryn et touchez « Scanner un QR code » : la session de{" "}
          <strong>{username}</strong> s'ouvre directement. Nouveau code dans {reste} s ; chaque code
          ne sert qu'une fois.
        </p>
        {erreur && qr && <p className="locaryn-vp-error">{erreur}</p>}
      </div>
    </dialog>,
    document.body,
  );
}
