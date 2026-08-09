import { useEffect, useRef, useState } from "react";
import { type Message, type MobileStatus, api } from "../lib/core";

type Props = {
  status: MobileStatus;
  onScan: () => void;
};

/**
 * The conversation.
 *
 * Everything heavy runs on the machine at the other end; this is a thread and
 * a text field. The bar at the top says which server and whether the phone is
 * away from it — the only two things about the connection worth a person's
 * attention.
 */
export function Chat({ status, onScan }: Props) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const endRef = useRef<HTMLDivElement>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: ni `messages` ni `busy` ne sont lus ici — ils déclenchent. Les retirer immobiliserait la vue au premier message au lieu de suivre la conversation.
  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, busy]);

  async function send() {
    const text = draft.trim();
    if (!text || busy) return;
    setDraft("");
    setError(null);
    setMessages((m) => [...m, { id: `u${m.length}`, role: "user", content: text }]);
    setBusy(true);
    try {
      const reply = await api.send(text);
      setMessages((m) => [...m, { id: `a${m.length}`, role: "assistant", content: reply }]);
    } catch (e) {
      setError(String(e));
      // Put the text back rather than losing it to a failed send.
      setDraft(text);
      setMessages((m) => m.slice(0, -1));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="lo-screen">
      <div className="lo-bar">
        <span className="lo-dot" />
        <span>{status.server_name ?? "Locaryn"}</span>
        {status.travelling && <span className="lo-bar-away">à distance</span>}
        <button
          type="button"
          className="lo-bar-away"
          style={{ marginLeft: status.travelling ? 8 : "auto", cursor: "pointer" }}
          onClick={onScan}
        >
          Scanner
        </button>
      </div>

      <div className="lo-thread">
        {messages.length === 0 && (
          <p className="lo-sub" style={{ marginTop: "var(--space-6)", textAlign: "center" }}>
            Posez une question. Le modèle tourne sur {status.server_name ?? "votre serveur"}.
          </p>
        )}
        {messages.map((m) => (
          <div key={m.id} className={`lo-msg ${m.role === "user" ? "lo-msg-me" : "lo-msg-ai"}`}>
            {m.content}
          </div>
        ))}
        {busy && (
          <div className="lo-msg lo-msg-ai" style={{ color: "var(--text-faint)" }}>
            …
          </div>
        )}
        {error && <p className="lo-error">{error}</p>}
        <div ref={endRef} />
      </div>

      <div className="lo-compose">
        <input
          className="lo-input"
          placeholder="Votre message"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void send()}
        />
        <button
          type="button"
          className="lo-send"
          disabled={busy || !draft.trim()}
          onClick={send}
          aria-label="Envoyer"
        >
          ↑
        </button>
      </div>
    </div>
  );
}
