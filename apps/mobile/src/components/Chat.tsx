import { Icon, renderMarkdown } from "@locaryn/ui-core";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  type ChatStreamEvent,
  type Conversation,
  type MediaForge,
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
import { AddContextSheet, type PieceJointe } from "./AddContextSheet";
import { ComposerActions } from "./ComposerActions";
import { ContextGauge } from "./ContextGauge";
import { Drawer } from "./Drawer";
import { type Destination, MainMenu, type ModelsTab } from "./MainMenu";
import { ToolApprovalModal } from "./ToolApprovalModal";
import { UpdateButton } from "./UpdateButton";
import { ExtensionSlot } from "./extensions/ExtensionSlot";

/** Un message écrit pendant que le modèle travaille (voir le bureau). */
type EnFile = { id: string; text: string; pieces?: PieceJointe[]; remis?: boolean };

/** La réflexion du modèle, telle qu'il l'a écrite (pour la déplier). */
function reflexion(texte: string): string {
  const blocs = [...texte.matchAll(/<think>([\s\S]*?)(?:<\/think>|$)/g)].map((m) => m[1].trim());
  return blocs.filter(Boolean).join("\n\n");
}

/** Le message qui part : le texte, puis chaque fichier texte joint, sous une
 *  enveloppe qui dit au modèle d'où il vient. */
function composer(texte: string, pieces: PieceJointe[]): string {
  const fichiers = pieces
    .filter((p) => p.genre === "texte")
    .map((p) => `\n\n--- Fichier joint : ${p.nom} ---\n${p.contenu}\n--- Fin de ${p.nom} ---`);
  return texte + fichiers.join("");
}

let compteur = 0;
function nouvelId(prefixe: string): string {
  compteur += 1;
  return `${prefixe}-${Date.now()}-${compteur}`;
}

/** Le texte à montrer : la réflexion du modèle (`<think>…</think>`) n'est pas
 *  la réponse, elle se résume à une ligne pendant qu'elle s'écrit. */
function sansReflexion(texte: string): string {
  return texte
    .replace(/<think>[\s\S]*?<\/think>/g, "")
    .replace(/<think>[\s\S]*$/, "")
    .trimStart();
}

function reflechit(texte: string): boolean {
  const ouvertes = (texte.match(/<think>/g) ?? []).length;
  const fermees = (texte.match(/<\/think>/g) ?? []).length;
  return ouvertes > fermees;
}

/** Clore les tuiles d'un appel (ou toutes) : les images reçues restent, une
 *  place vide disparaît — sauf échec, qu'elle dit. */
function terminerForge(liste: Message[], callId: string | null, echec: string | null): Message[] {
  return liste.flatMap((m) => {
    if (!m.forge || m.forge.failed) return [m];
    if (callId !== null && m.forge.callId !== callId) return [m];
    if ((m.images?.length ?? 0) > 0) return [{ ...m, forge: undefined }];
    if (echec) return [{ ...m, forge: { ...m.forge, failed: echec } }];
    return [];
  });
}

type Props = {
  status: MobileStatus;
  /** Chaque grand espace a son écran ; le tiroir dit lequel ouvrir. */
  onGo: (d: Destination | string, initialTab?: ModelsTab) => void;
  /** Ce que les extensions actives du serveur apportent, déjà lu par l'app. */
  capabilities: string[];
  /** Une conversation précise à ouvrir au montage — venue de l'écran Figures. */
  initialId?: string | null;
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
  /** Photos et fichiers joints au prochain message. */
  const [pieces, setPieces] = useState<PieceJointe[]>([]);
  const [feuille, setFeuille] = useState(false);
  /** L'autorisation choisie avant le premier message d'une conversation. */
  const [niveauAvant, setNiveauAvant] = useState<string | null>(null);
  /** Les réflexions dépliées, par message. */
  const [deplie, setDeplie] = useState<Set<string>>(new Set());
  const saisieRef = useRef<HTMLTextAreaElement>(null);
  const [busy, setBusy] = useState(false);
  /** Les messages écrits pendant que le modèle travaille. */
  const [file, setFile] = useState<EnFile[]>([]);
  const fileRef = useRef<EnFile[]>([]);
  fileRef.current = file;
  /** Après un Stop, la file attend la personne. */
  const [filePause, setFilePause] = useState(false);
  const [fileReduite, setFileReduite] = useState(false);
  /** L'outil que le modèle est en train d'utiliser, pour le dire. */
  const [outil, setOutil] = useState<string | null>(null);
  /** Le modèle se charge en mémoire sur le serveur (avant tout jeton). */
  const [chargement, setChargement] = useState(false);
  const conversationRef = useRef<string | null>(null);
  const arretRef = useRef(false);
  /** Le numéro de la réponse que ce fil suit : ouvrir une autre conversation
   *  le fait avancer, et la réponse précédente cesse d'écrire ici (elle
   *  continue sur le serveur et s'enregistre dans la sienne). */
  const runRef = useRef(0);

  /** Détacher la réponse en cours avant de changer de conversation. */
  function detacher() {
    runRef.current += 1;
    setBusy(false);
    setOutil(null);
    setChargement(false);
  }
  /** Une conversation est en train de charger ses messages. */
  const [loadingConversation, setLoadingConversation] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lightbox, setLightbox] = useState<MediaResult | null>(null);
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
        images: t.images,
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
    detacher();
    setDrawerOpen(false);
    setError(null);
    conversationRef.current = id;
    setCurrentId(id);
    setMessages([]);
    setLoadingConversation(true);
    try {
      const turns = await api.loadConversation(id);
      setMessages(
        turns.map((t) => ({
          id: t.id,
          role: t.role as Message["role"],
          content: t.content,
          images: t.images,
        })),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingConversation(false);
    }
  }

  function startNew() {
    detacher();
    setDrawerOpen(false);
    conversationRef.current = null;
    setCurrentId(null);
    setMessages([]);
    setError(null);
    setNiveauAvant(null);
  }

  // Une conversation venue d'ailleurs (l'écran Figures en a ouvert une) se
  // charge au montage. `key` sur le composant force le remontage à chaque
  // figure : le premier rendu suffit.
  // biome-ignore lint/correctness/useExhaustiveDependencies: l'ouverture ne se fait qu'au montage.
  useEffect(() => {
    if (initialId) void open(initialId);
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

  function imageSrc(img: MediaResult): string {
    return `data:${img.mime};base64,${img.data_base64}`;
  }

  async function copyImage(img: MediaResult) {
    try {
      const response = await fetch(imageSrc(img));
      const blob = await response.blob();
      if (!navigator.clipboard?.write || typeof ClipboardItem === "undefined") {
        throw new Error("presse-papier image indisponible");
      }
      await navigator.clipboard.write([new ClipboardItem({ [img.mime]: blob })]);
      setNotice("Image copiée.");
    } catch (e) {
      setNotice(`Copie impossible : ${String(e)}`);
    }
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

  /** Envoyer ce qui est écrit. Pendant une réponse, ou derrière une file qui
   *  attend, le message prend sa place dans la file : il ne la double jamais. */
  function send() {
    const text = draft.trim();
    if (!text && pieces.length === 0) return;
    const jointes = pieces;
    setDraft("");
    setPieces([]);
    if (saisieRef.current) saisieRef.current.style.height = "auto";
    setError(null);
    if (busy || fileRef.current.length > 0) {
      setFile((f) => [...f, { id: nouvelId("q"), text, pieces: jointes }]);
      if (!busy) setFilePause(false);
      return;
    }
    void repondre(text, jointes);
  }

  // La file part d'elle-même, dans l'ordre, dès que le modèle est libre.
  useEffect(() => {
    if (busy || filePause) return;
    const prochain = file.find((m) => !m.remis);
    if (!prochain) return;
    setFile((f) => f.filter((m) => m.id !== prochain.id));
    void repondre(prochain.text, prochain.pieces ?? []);
  }, [busy, filePause, file]);

  function ajouterAuFil(ev: ChatStreamEvent) {
    // Le premier signe de vie du modèle met fin au chargement.
    if (ev.type !== "loading" && ev.type !== "session") setChargement(false);
    switch (ev.type) {
      case "loading":
        setChargement(true);
        return;
      case "session": {
        const id = (ev as { id: string }).id;
        conversationRef.current = id;
        setCurrentId(id);
        return;
      }
      case "token": {
        const t = (ev as { text: string }).text;
        setMessages((m) => {
          const dernier = m[m.length - 1];
          if (
            dernier &&
            dernier.role === "assistant" &&
            !dernier.forge &&
            !dernier.images?.length
          ) {
            return [...m.slice(0, -1), { ...dernier, content: dernier.content + t }];
          }
          return [...m, { id: nouvelId("a"), role: "assistant", content: t }];
        });
        return;
      }
      case "tool_call":
        setOutil((ev as { tool: string }).tool);
        return;
      case "media_pending": {
        const e = ev as Extract<ChatStreamEvent, { type: "media_pending" }>;
        const forge: MediaForge = {
          callId: e.call_id,
          kind: e.kind,
          total: Math.max(1, e.count),
          width: e.width,
          height: e.height,
          etape: null,
          failed: null,
        };
        setMessages((m) => [
          ...m,
          { id: nouvelId("f"), role: "assistant", content: "", images: [], forge, forged: true },
        ]);
        return;
      }
      case "task_update": {
        const e = ev as { task_id: string; status: string };
        setMessages((m) =>
          m.map((x) =>
            x.forge?.callId === e.task_id ? { ...x, forge: { ...x.forge, etape: e.status } } : x,
          ),
        );
        return;
      }
      case "image_ready": {
        const media = (ev as { media: MediaResult }).media;
        setMessages((m) => {
          for (let i = m.length - 1; i >= 0; i--) {
            const x = m[i];
            if (x.forge && x.forge.kind === "image" && (x.images?.length ?? 0) < x.forge.total) {
              const suite = [...m];
              suite[i] = { ...x, images: [...(x.images ?? []), media] };
              return suite;
            }
          }
          return [...m, { id: nouvelId("i"), role: "assistant", content: "", images: [media] }];
        });
        return;
      }
      case "tool_result": {
        const e = ev as { call_id: string; ok: boolean };
        setOutil(null);
        setMessages((m) =>
          terminerForge(m, e.call_id, e.ok ? null : "La génération n'a pas abouti."),
        );
        return;
      }
      case "mail_read": {
        const ids = (ev as { ids: string[] }).ids;
        const lus = fileRef.current.filter((q) => ids.includes(q.id));
        setFile((f) => f.filter((q) => !ids.includes(q.id)));
        if (lus.length === 0) return;
        setMessages((m) => [
          ...m,
          ...lus.map((q) => ({
            id: nouvelId("u"),
            role: "user" as const,
            content: q.text,
            luEnCours: true,
          })),
        ]);
        return;
      }
      case "tool_approval": {
        const demande = ev as unknown as ToolApprovalRequest;
        setPendingApproval(demande);
        notifyToolApprovalRequired(demande.tool, demande.risk);
        return;
      }
      default:
        return;
    }
  }

  /** Envoyer un message et suivre la réponse en direct. */
  async function repondre(text: string, jointes: PieceJointe[] = []) {
    const images = jointes.filter((p) => p.genre === "image");
    setMessages((m) => [
      ...m,
      {
        id: nouvelId("u"),
        role: "user",
        content: [text, ...jointes.filter((p) => p.genre === "texte").map((p) => `📎 ${p.nom}`)]
          .filter(Boolean)
          .join("\n"),
        images: images.map((p) => ({
          name: p.nom,
          mime: p.apercu?.slice(5, p.apercu.indexOf(";")) || "image/png",
          data_base64: p.contenu,
        })),
      },
    ]);
    setBusy(true);
    arretRef.current = false;
    runRef.current += 1;
    const run = runRef.current;
    let reponse = "";
    try {
      await api.sendStream(
        composer(text, jointes),
        currentId,
        ephemeral,
        (ev) => {
          if (runRef.current !== run) return;
          if (ev.type === "token") reponse += (ev as { text: string }).text;
          ajouterAuFil(ev);
        },
        images.length ? images.map((p) => p.contenu) : undefined,
        currentId ? null : niveauAvant,
      );
      setNiveauAvant(null);
      if (document.hidden && reponse) {
        notifyMessageReceived(status.server_name ?? "Locaryn", sansReflexion(reponse));
      }
      // Une conversation éphémère n'apparaît nulle part : rien à rafraîchir.
      if (!ephemeral) void refreshList();
    } catch (e) {
      if (runRef.current !== run) return;
      setError(String(e));
      if (!reponse) {
        // Rien n'est venu : le texte revient dans le champ plutôt que d'être perdu.
        setDraft(text);
        setMessages((m) => m.filter((x, i) => !(i === m.length - 1 && x.role === "user")));
      }
    } finally {
      if (runRef.current === run) {
        setBusy(false);
        setOutil(null);
        setChargement(false);
        setMessages((m) => terminerForge(m, null, "Interrompu avant la fin."));
      }
      if (arretRef.current && fileRef.current.length > 0) setFilePause(true);
      void reprendreLesRemis();
    }
  }

  /** Stop : la réponse s'arrête, la file attend. */
  async function arreter() {
    const id = conversationRef.current ?? currentId;
    arretRef.current = true;
    if (fileRef.current.length > 0) setFilePause(true);
    if (!id) return;
    try {
      await api.cancelMessage(id);
    } catch (e) {
      setError(String(e));
    }
  }

  /** « Envoyer maintenant » : pendant une réponse, remis au modèle qui le lit
   *  à sa prochaine étape ; sinon, en tête de file et parti tout de suite. */
  async function envoyerMaintenant(q: EnFile) {
    const id = conversationRef.current ?? currentId;
    if (!busy || !id) {
      setFile((f) => [q, ...f.filter((x) => x.id !== q.id)]);
      setFilePause(false);
      return;
    }
    setFile((f) => f.map((x) => (x.id === q.id ? { ...x, remis: true } : x)));
    try {
      await api.depositMessage(id, q.id, q.text);
    } catch (e) {
      setError(String(e));
      setFile((f) => f.map((x) => (x.id === q.id ? { ...x, remis: false } : x)));
    }
  }

  async function retirer(q: EnFile) {
    const id = conversationRef.current ?? currentId;
    if (q.remis && id) {
      const repris = await api.withdrawMessage(id, q.id).catch(() => false);
      if (!repris) return; // déjà lu : il arrive dans le fil
    }
    setFile((f) => f.filter((x) => x.id !== q.id));
  }

  /** La réponse est finie et un message remis n'a pas été lu : il redevient
   *  un message en file, qui part normalement. */
  async function reprendreLesRemis() {
    const id = conversationRef.current;
    if (!id) return;
    for (const q of fileRef.current.filter((x) => x.remis)) {
      const repris = await api.withdrawMessage(id, q.id).catch(() => true);
      if (repris) setFile((f) => f.map((x) => (x.id === q.id ? { ...x, remis: false } : x)));
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
        <ExtensionSlot
          extensions={extensions}
          name="topbar.actions"
          context={{ onNavigate: onGo }}
        />
        <ExtensionSlot extensions={extensions} name="chat.header" context={{ onNavigate: onGo }} />
        <ContextGauge
          conversationId={currentId}
          busy={busy}
          onCompressed={() => {
            if (currentId) void open(currentId);
          }}
        />
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
        {messages.map((m) => {
          const texte = m.role === "assistant" ? sansReflexion(m.content) : m.content;
          const enCours = m.role === "assistant" && busy && reflechit(m.content);
          return (
            <div
              key={m.id}
              className={`lo-msg-group${m.role === "user" ? " lo-msg-group-me" : ""}`}
            >
              {m.role === "assistant" && reflexion(m.content) !== "" && (
                <button
                  type="button"
                  className={`lo-reflexion${enCours ? " is-live" : ""}`}
                  aria-expanded={deplie.has(m.id)}
                  onClick={() =>
                    setDeplie((d) => {
                      const n = new Set(d);
                      if (n.has(m.id)) n.delete(m.id);
                      else n.add(m.id);
                      return n;
                    })
                  }
                >
                  <span className="lo-reflexion-tete">
                    {enCours && <span className="lo-spinner" aria-hidden />}
                    {enCours ? "Réflexion…" : "Réflexion"}
                    <span className={`lo-reflexion-caret${deplie.has(m.id) ? " is-open" : ""}`}>
                      ›
                    </span>
                  </span>
                  {deplie.has(m.id) && (
                    <span className="lo-reflexion-texte">{reflexion(m.content)}</span>
                  )}
                </button>
              )}
              {texte !== "" &&
                (m.role === "user" ? (
                  <div className="lo-msg lo-msg-me">{texte}</div>
                ) : (
                  <div
                    className="lo-msg lo-msg-ai lo-md"
                    // biome-ignore lint/security/noDangerouslySetInnerHtml: renderMarkdown échappe tout le HTML source avant d'injecter ses propres balises (packages-ui/core/src/markdown.ts) : rien de ce que produit le modèle n'atteint le DOM sous forme de balise.
                    dangerouslySetInnerHTML={{ __html: renderMarkdown(texte) }}
                  />
                ))}
              {m.luEnCours && <span className="lo-msg-lu">✓ Lu pendant la tâche</span>}
              {(m.images?.length || m.forge) && (
                <div
                  className={`lo-msg-images${m.forged ? " lo-msg-images-forged" : ""}`}
                  data-count={m.forge ? m.forge.total : m.images?.length}
                >
                  {m.images?.map((img) => (
                    <button
                      key={img.name}
                      type="button"
                      className={`lo-msg-image${m.forged ? " lo-msg-image-pop" : ""}`}
                      onClick={() => setLightbox(img)}
                      title="Ouvrir l'image"
                    >
                      <img
                        src={`data:${img.mime};base64,${img.data_base64}`}
                        alt={m.content || "image générée"}
                        // Une image générée pèse un mégaoctet et demi : la
                        // décoder sur le fil principal fige l'application le
                        // temps de l'afficher, et Android finit par proposer
                        // de la fermer.
                        decoding="async"
                        loading="lazy"
                      />
                    </button>
                  ))}
                  {m.forge &&
                    !m.forge.failed &&
                    Array.from(
                      { length: Math.max(0, m.forge.total - (m.images?.length ?? 0)) },
                      (_, i) => (
                        <div
                          // biome-ignore lint/suspicious/noArrayIndexKey: une tuile n'a pas d'autre identité que sa place.
                          key={`forge-${i}`}
                          className="lo-forge"
                          role="img"
                          aria-label="Image en cours de création"
                          style={{
                            aspectRatio: `${m.forge?.width ?? 1} / ${m.forge?.height ?? 1}`,
                          }}
                        >
                          <span className="lo-forge-dots" />
                          <span className="lo-forge-glow" />
                        </div>
                      ),
                    )}
                </div>
              )}
              {m.forge &&
                (m.forge.failed ? (
                  <span className="lo-forge-caption lo-forge-failed">{m.forge.failed}</span>
                ) : (
                  m.forge.etape && <span className="lo-forge-caption">{m.forge.etape}</span>
                ))}
              {texte.trim() !== "" && (
                <button type="button" className="lo-msg-copy" onClick={() => void copy(texte)}>
                  Copier
                </button>
              )}
            </div>
          );
        })}
        {busy && (messages.length === 0 || messages[messages.length - 1].role === "user") && (
          <div className="lo-msg lo-msg-ai lo-msg-busy" role="status">
            <span className="lo-spinner" aria-hidden />
            <span>
              {chargement
                ? "Chargement du modèle en mémoire…"
                : outil
                  ? `Utilise ${outil}…`
                  : "Le modèle réfléchit…"}
            </span>
          </div>
        )}
        {busy && outil && messages[messages.length - 1]?.role !== "user" && (
          <p className="lo-outil" role="status">
            <span className="lo-spinner" aria-hidden />
            Utilise {outil}…
          </p>
        )}
        {error && <p className="lo-error">{error}</p>}
        <div ref={endRef} />
      </div>

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
              <button
                type="button"
                className="lo-image-lightbox-action"
                onClick={() => void keepImage(lightbox)}
              >
                Enregistrer sous
              </button>
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

      {notice && (
        <div className="lo-toast">
          <p className="lo-notice">{notice}</p>
        </div>
      )}

      {file.length > 0 && (
        <section className="lo-queue" aria-label="Messages en attente">
          <div className="lo-queue-head">
            <button
              type="button"
              className="lo-queue-fold"
              aria-expanded={!fileReduite}
              onClick={() => setFileReduite((v) => !v)}
            >
              <span className={`lo-queue-caret${fileReduite ? " is-closed" : ""}`} aria-hidden>
                ▾
              </span>
              {filePause ? "En pause" : busy ? "Après cette réponse" : "Envoi…"} · {file.length}
              {fileReduite && <span className="lo-queue-peek">{file[0].text}</span>}
            </button>
            {filePause && !busy && (
              <button type="button" className="lo-queue-btn" onClick={() => setFilePause(false)}>
                Reprendre
              </button>
            )}
          </div>
          {!fileReduite && (
            <ol className="lo-queue-list">
              {file.map((q) => (
                <li key={q.id} className={`lo-queue-item${q.remis ? " is-remis" : ""}`}>
                  <div className="lo-queue-body">
                    <span className="lo-queue-text">{q.text}</span>
                    {q.remis && (
                      <span className="lo-queue-state">Remis — lu à la prochaine étape</span>
                    )}
                  </div>
                  {!q.remis && (
                    <button
                      type="button"
                      className="lo-queue-btn"
                      onClick={() => void envoyerMaintenant(q)}
                    >
                      Maintenant
                    </button>
                  )}
                  <button
                    type="button"
                    className="lo-queue-btn lo-queue-x"
                    onClick={() => void retirer(q)}
                    aria-label="Retirer de la file"
                  >
                    ×
                  </button>
                </li>
              ))}
            </ol>
          )}
        </section>
      )}

      <div className="lo-compose">
        <div className="lo-composer">
          {pieces.length > 0 && (
            <div className="lo-pieces">
              {pieces.map((p) => (
                <span key={p.id} className="lo-piece">
                  {p.apercu ? <img src={p.apercu} alt="" /> : <Icon name="edit" size={14} />}
                  <span className="lo-piece-nom">{p.nom}</span>
                  <button
                    type="button"
                    aria-label={`Retirer ${p.nom}`}
                    onClick={() => setPieces((l) => l.filter((x) => x.id !== p.id))}
                  >
                    ×
                  </button>
                </span>
              ))}
            </div>
          )}
          <textarea
            ref={saisieRef}
            className="lo-composer-input"
            rows={1}
            placeholder={busy ? "Mettez un message en attente…" : "Votre message"}
            value={draft}
            onChange={(e) => {
              setDraft(e.target.value);
              const el = e.target;
              el.style.height = "auto";
              el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
            }}
          />
          <div className="lo-composer-bar">
            <button
              type="button"
              className="lo-composer-plus"
              onClick={() => setFeuille(true)}
              aria-label="Ajouter du contexte"
            >
              <Icon name="plus" size={20} />
            </button>
            <ComposerActions draft={draft} onDraft={setDraft} onError={setError} />
            <ExtensionSlot
              extensions={extensions}
              name="composer.toolbar"
              context={{
                input: draft,
                setInput: setDraft,
                send,
                canCompose: true,
                onNavigate: onGo,
              }}
            />
            <span className="lo-composer-espace" />
            {busy && !draft.trim() && pieces.length === 0 ? (
              <button
                type="button"
                className="lo-composer-send lo-stop"
                onClick={() => void arreter()}
                aria-label="Arrêter la réponse"
              >
                <span className="lo-stop-carre" aria-hidden />
              </button>
            ) : (
              <button
                type="button"
                className="lo-composer-send"
                disabled={!draft.trim() && pieces.length === 0}
                onClick={send}
                aria-label={busy ? "Mettre en file" : "Envoyer"}
              >
                ↑
              </button>
            )}
          </div>
        </div>
      </div>

      <AddContextSheet
        ouvert={feuille}
        onFermer={() => setFeuille(false)}
        conversationId={currentId}
        onJoindre={(liste) =>
          setPieces((l) => [...l, ...liste.filter((x) => !l.some((y) => y.id === x.id))])
        }
        onErreur={setError}
        niveauAvant={niveauAvant}
        onNiveauAvant={setNiveauAvant}
      />

      <ToolApprovalModal
        approval={pendingApproval}
        onResolve={handleResolveApproval}
        onCancel={() => setPendingApproval(null)}
      />
    </div>
  );
}
