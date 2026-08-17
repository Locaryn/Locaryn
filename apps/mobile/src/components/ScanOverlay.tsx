import { Icon } from "@locaryn/ui-core";

type Props = {
  /** Renoncer et refermer la caméra. */
  onCancel: () => void;
};

/**
 * Ce qu'on voit pendant qu'on vise un QR code.
 *
 * Sans cet écran, il n'y avait rien : la page devenait transparente pour
 * laisser voir la caméra dessinée derrière elle, et c'est tout. Pas de cadre,
 * pas de consigne, pas de bouton pour renoncer — un écran qui semblait vide,
 * sur lequel la seule sortie était le bouton retour du téléphone.
 *
 * Le cadre n'est pas décoratif : c'est lui qui dit où placer le code, et sans
 * repère on tient le téléphone trop près ou de travers, ce qui donne
 * l'impression que la lecture ne marche pas.
 */
export function ScanOverlay({ onCancel }: Props) {
  return (
    <div className="lo-scan-overlay">
      <div className="lo-scan-frame">
        <span className="lo-scan-corner lo-scan-tl" />
        <span className="lo-scan-corner lo-scan-tr" />
        <span className="lo-scan-corner lo-scan-bl" />
        <span className="lo-scan-corner lo-scan-br" />
      </div>

      <p className="lo-scan-hint">
        Placez le QR code affiché par l'application de bureau dans le cadre.
      </p>

      <button type="button" className="lo-scan-cancel" onClick={onCancel}>
        <Icon name="close" size={18} /> Annuler
      </button>
    </div>
  );
}
