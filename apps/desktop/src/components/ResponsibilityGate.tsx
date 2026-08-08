type Props = {
  open: boolean;
  /** What the user is enabling, e.g. "la génération d'images sans garde-fous". */
  what: string;
  onAccept: () => void;
  onCancel: () => void;
};

/**
 * Red consent gate shown before enabling an uncensored / abliterated model.
 * The user must explicitly accept sole responsibility. This is harm-reduction:
 * the tool stays capable, but the person owns what they do with it.
 */
export function ResponsibilityGate({ open, what, onAccept, onCancel }: Props) {
  if (!open) return null;
  return (
    <div className="locaryn-settings-backdrop" onClick={onCancel}>
      <div className="locaryn-gate" role="alertdialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        <div className="locaryn-gate-head">
          <span className="locaryn-gate-icon">⚠️</span>
          <h3>Mode sans limite — responsabilité</h3>
        </div>
        <p>
          Vous activez <strong>{what}</strong> avec un modèle <strong>sans garde-fous</strong>
          {" "}(abliteré). Les filtres de sécurité sont retirés.
        </p>
        <ul className="locaryn-gate-list">
          <li>Vous êtes <strong>seul responsable</strong> du contenu généré et de son usage.</li>
          <li>Vous vous engagez à respecter <strong>les lois applicables</strong> et les droits des tiers.</li>
          <li>L'application n'est qu'un outil et <strong>décline toute responsabilité</strong> pour les usages qui en sont faits.</li>
        </ul>
        <div className="locaryn-gate-actions">
          <button type="button" className="locaryn-btn-ghost" onClick={onCancel}>Annuler</button>
          <button type="button" className="locaryn-gate-accept" onClick={onAccept}>
            Je suis conscient·e et j'accepte la responsabilité
          </button>
        </div>
      </div>
    </div>
  );
}
