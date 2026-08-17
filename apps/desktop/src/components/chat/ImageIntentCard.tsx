import { Icon } from "@locaryn/ui-core";
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
    <div className={`locaryn-intent${decided ? " locaryn-intent-decided" : ""}`}>
      <div className="locaryn-intent-head">
        <span className="locaryn-intent-icon">
          <Icon name={intent.is_edit ? "edit" : "image"} size={15} />
        </span>
        <strong>{intent.is_edit ? "Modifier une image ?" : "Générer une image ?"}</strong>
        {decided && (
          <span className="locaryn-intent-badge">
            {decided === "accepted" ? "accepté" : "refusé"}
          </span>
        )}
      </div>

      {intent.reason && <p className="locaryn-intent-reason">{intent.reason}</p>}

      <dl className="locaryn-intent-rows">
        <div>
          <dt>Prompt (anglais)</dt>
          <dd className="locaryn-intent-prompt">{intent.english_prompt}</dd>
        </div>
        <div>
          <dt>Modèle</dt>
          <dd className="locaryn-kv-mono">{shortModel}</dd>
        </div>
        <div>
          <dt>Qualité</dt>
          <dd>
            {quality.label} · {quality.px}px
          </dd>
        </div>
      </dl>

      {!decided && (
        <div className="locaryn-intent-actions">
          <button type="button" className="locaryn-btn-ghost" onClick={onRefuse}>
            Non, répondre normalement
          </button>
          <div className="locaryn-intent-qualities">
            {IMAGE_QUALITIES.map((q) => (
              <button
                key={q.id}
                type="button"
                className={`locaryn-intent-q${q.id === quality.id ? " locaryn-active" : ""}`}
                onClick={() => onAccept(q.id)}
                title={`${q.hint} (${q.px}px)`}
              >
                {q.px}
              </button>
            ))}
          </div>
          <button
            type="button"
            className="locaryn-btn-primary"
            onClick={() => onAccept(quality.id)}
          >
            Générer
          </button>
        </div>
      )}
    </div>
  );
}
