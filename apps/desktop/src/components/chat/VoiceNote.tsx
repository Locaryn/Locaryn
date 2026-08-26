import { Icon, LoMorph } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";

type Props = {
  /** Blob/asset URL prepared by the desktop media bridge. */
  url: string | null;
  status: "loading" | "ready" | "error";
  error?: string;
  /** Raw generated-file path, passed back to the native Save As flow. */
  onSave: () => void | Promise<void>;
};

/**
 * A generated voice message is a thing to listen to first, not an invisible
 * download. The overflow menu deliberately contains the only save action so
 * the user stays in control of putting a copy on disk.
 */
export function VoiceNote({ url, status, error, onSave }: Props) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const close = (event: MouseEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setMenuOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [menuOpen]);

  async function save() {
    setSaving(true);
    setMenuOpen(false);
    try {
      await onSave();
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="locaryn-voice-note" aria-label="Note vocale">
      <div className="locaryn-voice-note-head">
        <span className="locaryn-voice-note-icon" aria-hidden="true">
          <Icon name="mic" size={16} />
        </span>
        <div className="locaryn-voice-note-title">
          <strong>Note vocale</strong>
          <span>
            {status === "loading"
              ? "Préparation de l'écoute…"
              : status === "error"
                ? "Lecture indisponible"
                : "Prête à être écoutée"}
          </span>
        </div>
        <div className="locaryn-voice-note-menu" ref={menuRef}>
          <button
            type="button"
            className="locaryn-voice-note-more"
            aria-label="Options de la note vocale"
            aria-expanded={menuOpen}
            disabled={status !== "ready" || saving}
            onClick={() => setMenuOpen((open) => !open)}
          >
            <span aria-hidden="true">⋮</span>
          </button>
          {menuOpen && (
            <div className="locaryn-voice-note-popover" role="menu">
              <button type="button" role="menuitem" onClick={() => void save()}>
                {saving ? "Préparation…" : "Enregistrer sous…"}
              </button>
            </div>
          )}
        </div>
      </div>

      {status === "ready" && url ? (
        <audio
          className="locaryn-voice-note-player"
          src={url}
          controls
          preload="metadata"
          aria-label="Écouter la note vocale"
        />
      ) : status === "error" ? (
        <p className="locaryn-voice-note-error">{error ?? "Impossible de charger cette note."}</p>
      ) : (
        <div className="locaryn-voice-note-loading" aria-live="polite">
          <LoMorph width={72} />
          <span>La note arrive…</span>
        </div>
      )}
    </div>
  );
}
