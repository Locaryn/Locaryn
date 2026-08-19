import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  type Conversation,
  type MediaResult,
  type Message,
  type MobileStatus,
  type ToolApprovalDecision,
  type ToolApprovalRequest,
  api,
} from "../lib/core";
import type { PhoneExtension } from "../lib/core";
import { useCoucheRetour } from "../lib/navigation";
import { notifyMessageReceived, notifyToolApprovalRequired } from "../lib/notifications";
import { ComposerActions } from "./ComposerActions";
import { Drawer } from "./Drawer";
import { type Destination, MainMenu, type ModelsTab } from "./MainMenu";
import { ToolApprovalModal } from "./ToolApprovalModal";
import { UpdateButton } from "./UpdateButton";
import { ExtensionSlot } from "./extensions/ExtensionSlot";

type Props = {
  status: MobileStatus;
  /** Chaque grand espace a son écran ; le tiroir dit lequel ouvrir. */
  onGo: (d: Destination | string, initialTab?: ModelsTab) => void;
  /** Ce que les extensions actives du serveur apportent, déjà lu par l'app. */
  capabilities: string[];
  /** Une conversation précise à ouvrir au montage — venue de l'écran Figures. */
  initialId?: string | null;
  /** Une image produite par le Studio, à poser dans le fil au montage. */
  initialMedia?: MediaResult | null;
  /** L'image initiale a été posée : l'application peut l'oublier. */
  onConsumedMedia?: () => void;
  /** Extensions actives : le menu en tire ses `nav_items`. */
  extensions?: PhoneExtension[];
  /** Le bouton « Mettre à jour » mène directement à la section À propos. */
  onOpenUpdate: () => void;
};

/**
 * The conversation.
 *
 * Everything heavy runs on the machine at the other end; this is a thread and
 * a text field. Le tiroir de gauche tient les conversations — celles du
 * serveur, donc celles de l'ordinateur : une phrase écrite ici se lit là-bas,
 * et une conversation commencée là-bas se continue ici.
 */
export function Chat({
  status,
  onGo,
  capabilities,
  initialId,
  initialMedia,
  onConsumedMedia,
  extensions = [],
  onOpenUpdate,
}: Props) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [conversations, setConversations] = useState<Conversation[] | null>(null);
  const [currentId, setCurrentId] = useState<string | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [pendingApproval, setPendingApproval] = useState<ToolApprovalRequest | null>(null);
  /**
   * Conversation éphémère : rien n'en sera gardé, pas même son titre. L'écran
   * le dit — un mode dont on ne se souvient pas n'en est pas un.
   */
  const [ephemeral, setEphemeral] = useState(false);
  /**
   * Le Studio n'existe que si le serveur a une extension qui apporte de quoi
   * générer. La liste vient de l'application, qui la relit quand les
   * extensions bougent.
   */
  const canCreate = capabilities.some((c) => c.endsWith("-gen") || c === "voice-tts");
  const canFigures = capabilities.includes("figures");
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  /** Une conversation est en train de charger ses messages. */
  const [loadingConversation, setLoadingConversation] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** Confirmation brève : copie faite, image enregistrée. */
  const [notice, setNotice] = useState<string | null>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const threadRef = useRef<HTMLDivElement>(null);
  /** Vrai tant que la personne lit le bas du fil : on peut l'y suivre quand
   *  un message arrive. Faux si elle a remonté pour relire — on ne la tire
   *  pas vers le bas à chaque rafraîchissement. */
  const nearBottomRef = useRef(true);

  // Une confirmation qui reste à l'écran devient du décor. Trois secondes, le
  // temps de la lire.
  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(() => setNotice(null), 3000);
    return () => clearTimeout(t);
  }, [notice]);

  // Le retour d'Android ferme ce qui est ouvert au lieu de quitter
  // l'application : le tiroir, le menu, la demande d'autorisation.
  useCoucheRetour(drawerOpen, () => setDrawerOpen(false));
  useCoucheRetour(menuOpen, () => setMenuOpen(false));
  useCoucheRetour(pendingApproval !== null, () => setPendingApproval(null));

  const refreshList = useCallback(async () => {
    try {
      setConversations(await api.listConversations());
    } catch {
      // Une liste indisponible ne doit pas empêcher d'écrire : le tiroir dira
      // simplement qu'il n'a rien à montrer.
      setConversations([]);
    }
  }, []);

  useEffect(() => {
    void refreshList();
  }, [refreshList]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: ni `messages` ni `busy` ne sont lus ici — ils déclenchent. Les retirer immobiliserait la vue au premier message au lieu de suivre la conversation.
  useEffect(() => {
    if (!nearBottomRef.current) return;
    endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, busy]);

  // ── Synchronisation avec le serveur ────────────────────────────
  //
  // Le téléphone n'est pas le seul à écrire : l'ordinateur continue les mêmes
  // conversations. Sans un rafraîchissement régulier, une conversation ouverte
  // ici restait figée sur ce qu'elle montrait à l'ouverture, et une
  // conversation commencée là-bas n'apparaissait pas dans le tiroir. On relit
  // la liste et le fil tant que l'application est visible — cinq secondes, le
  // temps de ne pas rater une réponse sans vider la batterie.
  const pollMessages = useCallback(async () => {
    if (!currentId || busy || loadingConversation) return;
    try {
      const turns = await api.loadConversation(currentId);
      const recus = turns.map((t) => ({
        id: t.id,
        role: t.role as Message["role"],
        content: t.content,
      }));
      setMessages((prev) => {
        // Rien de nouveau : on garde ce qu'on a — y compris les images que le
        // rechargement ne renvoie pas. Le serveur est en append-only, donc si
        // la liste s'allonge, ce qui manque est à la fin.
        if (recus.length <= prev.length) return prev;
        return [...prev, ...recus.slice(prev.length)];
      });
    } catch {
      // Un serveur qui ne répond pas n'a rien à ajouter : au prochain tour.
    }
  }, [currentId, busy, loadingConversation]);

  useEffect(() => {
    const t = window.setInterval(() => {
      if (document.visibilityState === "visible") void refreshList();
    }, 5000);
    return () => window.clearInterval(t);
  }, [refreshList]);

  // Le fil est relu plus souvent que la liste : c'est lui qu'on regarde. Trois
  // secondes, le temps de ne pas rater une réponse écrite sur l'ordinateur.
  useEffect(() => {
    const t = window.setInterval(() => {
      if (document.visibilityState === "visible") void pollMessages();
    }, 3000);
    return () => window.clearInterval(t);
  }, [pollMessages]);

  // Revenir à l'application rafraîchit tout de suite : pas besoin d'attendre
  // le prochain tour de minuterie pour voir ce qui s'est passé ailleurs.
  useEffect(() => {
    function auPremierPlan() {
      if (document.visibilityState !== "visible") return;
      void refreshList();
      void pollMessages();
    }
    document.addEventListener("visibilitychange", auPremierPlan);
    return () => document.removeEventListener("visibilitychange", auPremierPlan);
  }, [refreshList, pollMessages]);

  /** Reprendre une conversation, d'où qu'elle vienne. */
  async function open(id: string) {
    setDrawerOpen(false);
    setError(null);
    setCurrentId(id);
    setMessages([]);
    setLoadingConversation(true);
    try {
      const turns = await api.loadConversation(id);
      setMessages(
        turns.map((t) => ({ id: t.id, role: t.role as Message["role"], content: t.content })),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingConversation(false);
    }
  }

  function startNew() {
    setDrawerOpen(false);
    setCurrentId(null);
    setMessages([]);
    setError(null);
  }

  // Une conversation venue d'ailleurs (l'écran Figures en a ouvert une) se
  // charge au montage. `key` sur le composant force le remontage à chaque
  // figure : le premier rendu suffit.
  // biome-ignore lint/correctness/useExhaustiveDependencies: l'ouverture ne se fait qu'au montage.
  useEffect(() => {
    if (initialId) void open(initialId);
  }, []);

  // Une image produite par le Studio arrive avec le montage : elle s'affiche
  // dans le fil, comme le ferait une réponse. Une fois posée, l'application
  // l'oublie — un aller-retour au Studio ne doit pas la reposer deux fois.
  // biome-ignore lint/correctness/useExhaustiveDependencies: ne se fait qu'au montage.
  useEffect(() => {
    if (!initialMedia) return;
    setMessages([
      {
        id: `media-${initialMedia.name}`,
        role: "assistant",
        content: "Image générée dans le Studio.",
        images: [initialMedia],
      },
    ]);
    onConsumedMedia?.();
  }, []);

  /** Reprendre une conversation gardée quitte le mode éphémère. */
  function openKept(id: string) {
    setEphemeral(false);
    void open(id);
  }

  /**
   * Copier un message.
   *
   * `navigator.clipboard` exige un contexte sûr ; la vue web d'Android en est
   * un (`http://tauri.localhost`), mais le repli couvre le cas contraire
   * plutôt que d'échouer sans rien dire.
   */
  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      const zone = document.createElement("textarea");
      zone.value = text;
      zone.style.position = "fixed";
      zone.style.opacity = "0";
      document.body.appendChild(zone);
      zone.select();
      document.execCommand("copy");
      zone.remove();
    }
    setNotice("Message copié.");
  }

  /** Écrire l'image sur l'appareil et la confier au système. */
  async function keepImage(img: MediaResult) {
    try {
      const nom = await api.saveImage(img);
      setNotice(`${nom} enregistrée. Android propose de la ranger ou de l'envoyer.`);
    } catch (e) {
      setNotice(String(e));
    }
  }

  async function handleResolveApproval(decision: ToolApprovalDecision) {
    setPendingApproval(null);
    try {
      await api.approveToolCall(decision);
    } catch (e) {
      setError(String(e));
    }
  }

  async function send() {
    const text = draft.trim();
    if (!text || busy) return;
    setDraft("");
    setError(null);
    setMessages((m) => [...m, { id: `u${m.length}`, role: "user", content: text }]);
    setBusy(true);
    try {
      const reply = await api.send(text, currentId, ephemeral);
      setCurrentId(reply.conversation_id);
      setMessages((m) => [
        ...m,
        { id: `a${m.length}`, role: "assistant", content: reply.text, images: reply.images },
      ]);
      if (reply.approval) {
        setPendingApproval(reply.approval);
        notifyToolApprovalRequired(reply.approval.tool, reply.approval.risk);
      }
      if (document.hidden && reply.text) {
        notifyMessageReceived(status.server_name ?? "Locaryn", reply.text);
      }
      // Une conversation éphémère n'apparaît nulle part : rien à rafraîchir.
      if (!ephemeral) void refreshList();
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
    <div className={`lo-screen${ephemeral ? " lo-ephemeral" : ""}`}>
      <div className="lo-bar">
        <button
          type="button"
          className="lo-bar-menu"
          onClick={() => {
            // Le tiroir se rouvre : c'est le moment où on veut voir ce qui
            // s'est passé ailleurs. On relit tout de suite, pas au prochain
            // tour de minuterie.
            setDrawerOpen(true);
            void refreshList();
          }}
          aria-label="Ouvrir l'historique"
        >
          <Icon name="menu" />
        </button>
        <span className="lo-dot" />
        <span>{status.server_name ?? "Locaryn"}</span>
        {status.travelling && <span className="lo-bar-away">à distance</span>}
        <span className="lo-bar-spacer" />
        {/*
          L'éphémère se propose là où il se décide : en haut à droite d'une
          conversation encore vide. Une fois le premier message parti, le choix
          n'a plus de sens — la conversation existe — et le bouton disparaît
          plutôt que de rester à ne rien faire.
        */}
        {messages.length === 0 && !currentId && (
          <button
            type="button"
            className={`lo-bar-icon${ephemeral ? " lo-bar-icon-on" : ""}`}
            onClick={() => setEphemeral((v) => !v)}
            aria-pressed={ephemeral}
            aria-label="Conversation éphémère"
            title="Rien de cette conversation ne sera gardé"
          >
            <Icon name="private" />
          </button>
        )}
        {/* Bouton Figures si disponible */}
        {canFigures && (
          <button
            type="button"
            className="lo-bar-icon"
            onClick={() => onGo("figures")}
            aria-label="Mode Figures"
            title="Figures & Personas"
          >
            <Icon name="figures" />
          </button>
        )}
        <ExtensionSlot name="topbar.actions" context={{ onNavigate: onGo }} />
        <ExtensionSlot name="chat.header" context={{ onNavigate: onGo }} />
        <UpdateButton onOpen={onOpenUpdate} />
        <button
          type="button"
          className="lo-bar-menu"
          onClick={() => setMenuOpen(true)}
          aria-label="Ouvrir le menu"
        >
          <Icon name="more" />
        </button>
      </div>

      {ephemeral && (
        <p className="lo-ephemeral-banner">
          Conversation éphémère — rien n'en sera gardé, pas même son titre.
        </p>
      )}

      <Drawer
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        conversations={conversations}
        currentId={currentId}
        onPick={openKept}
        onNew={startNew}
        onChanged={() => void refreshList()}
      />

      <MainMenu
        open={menuOpen}
        onClose={() => setMenuOpen(false)}
        canCreate={canCreate}
        canFigures={canFigures}
        extensions={extensions}
        onGo={(d, tab) => {
          setMenuOpen(false);
          if (d === "chat") return;
          onGo(d, tab);
        }}
      />

      <div
        className="lo-thread"
        ref={threadRef}
        onScroll={() => {
          const el = threadRef.current;
          if (!el) return;
          // À moins de 120 px du bas, on est « en bas » : les nouveaux
          // messages peuvent nous y suivre. Au-delà, on lit plus haut et on
          // ne veut pas être tiré vers le bas.
          nearBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 120;
        }}
      >
        {messages.length === 0 && loadingConversation && (
          <div className="lo-thread-loading" role="status">
            <span className="lo-spinner" aria-hidden />
            <span>Chargement de la conversation…</span>
          </div>
        )}
        {messages.length === 0 && !loadingConversation && (
          <p className="lo-sub" style={{ marginTop: "var(--space-6)", textAlign: "center" }}>
            Posez une question. Le modèle tourne sur {status.server_name ?? "votre serveur"}.
          </p>
        )}
        {messages.map((m) => (
          <div key={m.id} className={`lo-msg-group${m.role === "user" ? " lo-msg-group-me" : ""}`}>
            <div className={`lo-msg ${m.role === "user" ? "lo-msg-me" : "lo-msg-ai"}`}>
              {m.content}
            </div>
            {m.images?.map((img) => (
              <button
                key={img.name}
                type="button"
                className="lo-msg-image"
                onClick={() => void keepImage(img)}
                title="Enregistrer l'image"
              >
                <img
                  src={`data:${img.mime};base64,${img.data_base64}`}
                  alt={m.content}
                  // Une image générée pèse un mégaoctet et demi : la décoder
                  // sur le fil principal fige l'application le temps de
                  // l'afficher, et Android finit par proposer de la fermer.
                  decoding="async"
                  loading="lazy"
                />
              </button>
            ))}
            {m.content.trim() !== "" && (
              <button type="button" className="lo-msg-copy" onClick={() => void copy(m.content)}>
                Copier
              </button>
            )}
          </div>
        ))}
        {busy && (
          <div className="lo-msg lo-msg-ai lo-msg-busy" role="status">
            <span className="lo-spinner" aria-hidden />
            <span>Le modèle réfléchit…</span>
          </div>
        )}
        {error && <p className="lo-error">{error}</p>}
        <div ref={endRef} />
      </div>

      {notice && (
        <div className="lo-toast">
          <p className="lo-notice">{notice}</p>
        </div>
      )}

      <div className="lo-compose">
        <ComposerActions draft={draft} onDraft={setDraft} onError={setError} />
        <ExtensionSlot
          name="composer.toolbar"
          context={{ input: draft, setInput: setDraft, send, canCompose: !busy, onNavigate: onGo }}
        />
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

      <ToolApprovalModal
        approval={pendingApproval}
        onResolve={handleResolveApproval}
        onCancel={() => setPendingApproval(null)}
      />
    </div>
  );
}
