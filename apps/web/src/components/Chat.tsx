import { useCallback, useEffect, useRef, useState } from "react";
import {
  type Conversation,
  type MediaResult,
  type Message,
  type WebStatus,
  api,
} from "../lib/core";
import { Drawer } from "./Drawer";
import { type Destination, MainMenu } from "./MainMenu";

type Props = {
  status: WebStatus;
  /** Chaque grand espace a son écran ; le menu dit lequel ouvrir. */
  onGo: (d: Destination) => void;
  /** Ce que les extensions actives du serveur apportent, déjà lu par l'app. */
  capabilities: string[];
  /** Une conversation précise à ouvrir au montage — venue de l'écran Figures. */
  initialId?: string | null;
};

/**
 * The conversation. Everything heavy runs on the machine at the other end;
 * this is a thread and a text field — but a thread that resumes: the same
 * conversations as the phone and the desktop, so nothing said elsewhere is
 * lost here.
 *
 * Le tiroir de gauche tient l'historique — les conversations du serveur,
 * puis les projets dépliables. Le menu principal (⋮) mène aux grands
 * espaces : Studio, Figures, Extensions, Modèles, Réglages.
 */
export function Chat({ status, onGo, capabilities, initialId }: Props) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [conversations, setConversations] = useState<Conversation[] | null>(null);
  const [currentId, setCurrentId] = useState<string | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const canCreate = capabilities.some((c) => c.endsWith("-gen") || c === "voice-tts");
  const canFigures = capabilities.includes("figures");
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lightbox, setLightbox] = useState<MediaResult | null>(null);
  const endRef = useRef<HTMLDivElement>(null);

  async function open(id: string) {
    setDrawerOpen(false);
    setError(null);
    setCurrentId(id);
    setMessages([]);
    try {
      setMessages(await api.loadConversation(id).catch(() => [] as Message[]));
    } catch (e) {
      setError(String(e));
    }
  }

  function newConversation() {
    setDrawerOpen(false);
    setError(null);
    setCurrentId(null);
    setMessages([]);
  }

  // Une conversation venue d'ailleurs (l'écran Figures en a ouvert une) se
  // charge au montage. `key` sur le composant force le remontage à chaque
  // figure : le premier rendu suffit.
  // biome-ignore lint/correctness/useExhaustiveDependencies: l'ouverture ne se fait qu'au montage.
  useEffect(() => {
    if (initialId) void open(initialId);
  }, []);

  const refreshConversations = useCallback(async () => {
    try {
      setConversations(await api.listConversations());
    } catch {
      // Une liste muette ne doit pas vider l'écran : on garde ce qu'on a.
    }
  }, []);

  useEffect(() => {
    void refreshConversations();
  }, [refreshConversations]);

  // Reprendre la conversation la plus récente, comme si l'on revenait sur le
  // téléphone : le chat web est le même serveur, le même historique.
  useEffect(() => {
    if (initialId) return;
    let cancelled = false;
    void (async () => {
      const list = await api.listConversations().catch(() => null);
      if (cancelled || !list || list.length === 0) return;
      const dernier = list[0];
      setCurrentId(dernier.id);
      const historique = await api.loadConversation(dernier.id).catch(() => [] as Message[]);
      if (!cancelled) setMessages(historique);
    })();
    return () => {
      cancelled = true;
    };
  }, [initialId]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: ni `messages` ni `busy` ne sont lus ici — ils déclenchent. Les retirer immobiliserait la vue au premier message au lieu de suivre la conversation.
  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, busy]);

  async function send() {
    const text = draft.trim();
    if (!text || busy) return;
    setDraft("");
    setError(null);
    const optimiste: Message = { id: `u${Date.now()}`, role: "user", content: text };
    setMessages((m) => [...m, optimiste]);
    setBusy(true);
    try {
      // La première question d'une conversation neuve crée sa session : la
      // suivante reprend la même, et l'historique reste lisible ailleurs.
      let sessionId = currentId;
      if (!sessionId) {
        sessionId = (await api.newConversation()).id;
        setCurrentId(sessionId);
      }
      const reply = await api.send(text, sessionId);
      setMessages((m) => [
        ...m,
        { id: `a${Date.now()}`, role: "assistant", content: reply.text, images: reply.images },
      ]);
    } catch (e) {
      setError(String(e));
      // Put the text back rather than losing it to a failed send.
      setDraft(text);
      setMessages((m) => m.filter((x) => x.id !== optimiste.id));
    } finally {
      setBusy(false);
    }
  }

  function imageSrc(image: MediaResult): string {
    return `data:${image.mime};base64,${image.data_base64}`;
  }

  async function copyImage(image: MediaResult) {
    try {
      const response = await fetch(imageSrc(image));
      const blob = await response.blob();
      if (!navigator.clipboard?.write || typeof ClipboardItem === "undefined") {
        throw new Error("presse-papier image indisponible");
      }
      await navigator.clipboard.write([new ClipboardItem({ [image.mime]: blob })]);
    } catch (e) {
      setError(`Copie de l'image impossible : ${String(e)}`);
    }
  }

  return (
    <div className="lo-screen lo-chat">
      <div className="lo-bar">
        <button
          type="button"
          className="lo-bar-menu"
          aria-label="Historique"
          title="Historique"
          onClick={() => setDrawerOpen(true)}
        >
          ☰
        </button>
        <span className="lo-dot" />
        <span className="lo-bar-title">{status.server_name ?? "Locaryn"}</span>
        <span className="lo-bar-spacer" />
        {status.username && <span className="lo-bar-away">{status.username}</span>}
        <button
          type="button"
          className="lo-bar-menu"
          aria-label="Ouvrir le menu"
          title="Menu"
          onClick={() => setMenuOpen(true)}
        >
          ⋮
        </button>
      </div>
      <div className="lo-thread">
        {messages.length === 0 && !busy && (
          <p className="lo-sub lo-empty">
            Posez une question. Le modèle tourne sur {status.server_name ?? "votre serveur"}.
          </p>
        )}
        {messages.map((m, i) => (
          <div
            key={m.id}
            className={`lo-msg ${m.role === "user" ? "lo-msg-me" : "lo-msg-ai"}`}
            // Une entrée en cascade : l'historique chargé arrive d'un bloc,
            // chaque message entre un court instant après le précédent.
            style={{ animationDelay: `${Math.min(i * 35, 350)}ms` }}
          >
            {m.content}
            {m.images?.map((image) => (
              <button
                key={image.name}
                type="button"
                className="lo-msg-image"
                onClick={() => setLightbox(image)}
                title="Ouvrir l'image"
              >
                <img src={imageSrc(image)} alt={m.content || "Image générée"} loading="lazy" />
              </button>
            ))}
          </div>
        ))}
        {busy && (
          <output className="lo-msg lo-msg-ai lo-typing" aria-label="Le modèle réfléchit">
            <i />
            <i />
            <i />
          </output>
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
      </div>{" "}
      <Drawer
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        conversations={conversations}
        currentId={currentId}
        onPick={(id) => void open(id)}
        onNew={newConversation}
      />
      <MainMenu
        open={menuOpen}
        onClose={() => setMenuOpen(false)}
        canCreate={canCreate}
        canFigures={canFigures}
        onGo={onGo}
      />
      {lightbox && (
        <div
          className="lo-image-lightbox"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setLightbox(null);
          }}
        >
          <div
            className="lo-image-lightbox-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="Image agrandie"
          >
            <div className="lo-image-lightbox-toolbar">
              <a
                className="lo-image-lightbox-action"
                href={imageSrc(lightbox)}
                download={lightbox.name}
              >
                Enregistrer sous
              </a>
              <button
                type="button"
                className="lo-image-lightbox-action"
                onClick={() => void copyImage(lightbox)}
              >
                Copier l'image
              </button>
              <button
                type="button"
                className="lo-image-lightbox-close"
                onClick={() => setLightbox(null)}
                aria-label="Fermer l'image agrandie"
              >
                ×
              </button>
            </div>
            <img
              className="lo-image-lightbox-image"
              src={imageSrc(lightbox)}
              alt="Image agrandie"
            />
          </div>
        </div>
      )}
    </div>
  );
}
