import { IMAGE_QUALITIES, type ImageIntent } from "../../lib/core";

type Props = {
  intent: ImageIntent;
  /** Model that will render it (already resolved by the panel). */
  model: string;
  onAccept: (quality: string) => void;
  onRefuse: () => void;
  /** Set once the user answered, so the card freezes. */
  decided?: "accepted" | "refused";
};

/**
 * Confirmation shown inline in the conversation when the assistant thinks a
 * plain message ("je veux une image d'un chat") should be routed to the image
 * generator. Nothing is generated until the user accepts — the model can
 * misread a request, and generation costs real time on the GPU.
 */
export function ImageIntentCard({ intent, model, onAccept, onRefuse, decided }: Props) {
  const quality = IMAGE_QUALITIES.find((q) => q.id === intent.quality) ?? IMAGE_QUALITIES[1];
  const shortModel = model.split(/[\\/]/).pop() || model;

  return (
    <div className={`lochor-intent${decided ? " lochor-intent-decided" : ""}`}>
      <div className="lochor-intent-head">
        <span className="lochor-intent-icon">{intent.is_edit ? "🖼️" : "🎨"}</span>
        <strong>
          {intent.is_edit ? "Modifier une image ?" : "Générer une image ?"}
        </strong>
        {decided && (
          <span className="lochor-intent-badge">
            {decided === "accepted" ? "accepté" : "refusé"}
          </span>
        )}
      </div>

      {intent.reason && <p className="lochor-intent-reason">{intent.reason}</p>}

      <dl className="lochor-intent-rows">
        <div>
          <dt>Prompt (anglais)</dt>
          <dd className="lochor-intent-prompt">{intent.english_prompt}</dd>
        </div>
        <div>
          <dt>Modèle</dt>
          <dd className="lochor-kv-mono">{shortModel}</dd>
        </div>
        <div>
          <dt>Qualité</dt>
          <dd>
            {quality.label} · {quality.px}px
          </dd>
        </div>
      </dl>

      {!decided && (
        <div className="lochor-intent-actions">
          <button type="button" className="lochor-btn-ghost" onClick={onRefuse}>
            Non, répondre normalement
          </button>
          <div className="lochor-intent-qualities">
            {IMAGE_QUALITIES.map((q) => (
              <button
                key={q.id}
                type="button"
                className={`lochor-intent-q${q.id === quality.id ? " lochor-active" : ""}`}
                onClick={() => onAccept(q.id)}
                title={`${q.hint} (${q.px}px)`}
              >
                {q.px}
              </button>
            ))}
          </div>
          <button type="button" className="lochor-btn-primary" onClick={() => onAccept(quality.id)}>
            Générer
          </button>
        </div>
      )}
    </div>
  );
}
