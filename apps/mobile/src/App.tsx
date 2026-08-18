import { useCallback, useEffect, useState } from "react";
import { ArchivesScreen } from "./components/ArchivesScreen";
import { Chat } from "./components/Chat";
import { ConfirmServer, type ProvisioningApercu, lireApercu } from "./components/ConfirmServer";
import { ExtensionView } from "./components/ExtensionView";
import { Extensions } from "./components/Extensions";
import { FiguresScreen } from "./components/FiguresScreen";
import type { Destination } from "./components/MainMenu";
import { MemoryScreen } from "./components/MemoryScreen";
import { Models } from "./components/Models";
import { Paired } from "./components/Paired";
import { ReconnectPrompt } from "./components/ReconnectPrompt";
import { ScanOverlay } from "./components/ScanOverlay";
import { Settings } from "./components/Settings";
import { SignIn } from "./components/SignIn";
import { Studio } from "./components/Studio";
import {
  type MediaResult,
  type MobileStatus,
  type PairingResult,
  type PhoneExtension,
  api,
  coreMode,
} from "./lib/core";
import { useNavigation } from "./lib/navigation";
import { surEchecReseau } from "./lib/reachability";
import { annulerScan, isScannerAvailable, scan } from "./lib/scanner";
import { appliquerAccent, lireAccent } from "./lib/theme";

type Screen = "signin" | "chat" | "memory" | Destination | (string & {});

export function App() {
  const [status, setStatus] = useState<MobileStatus | null>(null);
  // La navigation passe par l'historique : c'est ce qui donne au bouton retour
  // d'Android quelque chose à faire, au lieu de fermer l'application depuis
  // n'importe quel écran.
  const { ecran: screen, aller, revenir, remplacer } = useNavigation<Screen>("chat");
  const [paired, setPaired] = useState<PairingResult | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);
  /** Un code de configuration vient d'être lu, mais rien n'est encore
   *  enregistré : la personne doit d'abord voir à quel serveur elle se
   *  connecterait, et par quel chemin. */
  const [pendingProvisioning, setPendingProvisioning] = useState<{
    raw: string;
    apercu: ProvisioningApercu;
  } | null>(null);
  const [connecting, setConnecting] = useState(false);
  /** Vrai pendant que la caméra est ouverte : c'est ce qui dessine le cadre. */
  const [scanning, setScanning] = useState(false);
  const [startupError, setStartupError] = useState<string | null>(null);
  /** Le serveur ne répond plus : proposer de reprendre à une nouvelle adresse
   *  plutôt que de laisser chaque écran l'annoncer à sa façon. `null` tant
   *  que la personne n'a pas rencontré l'échec, ou qu'elle l'a écarté. */
  const [showReconnect, setShowReconnect] = useState(false);
  const [reconnectBusy, setReconnectBusy] = useState(false);
  const [reconnectError, setReconnectError] = useState<string | null>(null);
  /** Ce que les extensions actives du serveur apportent. Lu une fois, relu
   *  quand une extension bouge : c'est ce qui décide des écrans disponibles. */
  const [capabilities, setCapabilities] = useState<string[]>([]);
  /** Les extensions actives : le menu et le Studio en tirent leurs
   *  contributions (nav_items, studio_tabs) sans jamais les nommer. */
  const [activeExtensions, setActiveExtensions] = useState<PhoneExtension[]>([]);
  /** Une conversation neuve tenue par une figure, à ouvrir dans le chat. */
  const [figureChatId, setFigureChatId] = useState<string | null>(null);
  /** Une conversation sortie des archives, à ouvrir dans le chat. */
  const [restoredChatId, setRestoredChatId] = useState<string | null>(null);
  /** Une image produite par le Studio, à poser dans le fil au retour au chat. */
  const [pendingMedia, setPendingMedia] = useState<MediaResult | null>(null);
  /** L'onglet d'ouverture de l'écran Modèles, choisi depuis le menu. */
  const [modelsTab, setModelsTab] = useState<"installed" | "marketplace">("installed");

  // La couleur d'accent choisie dans Paramètres → Apparence s'applique dès le
  // démarrage, pas seulement à l'ouverture des réglages.
  useEffect(() => {
    appliquerAccent(lireAccent());
  }, []);

  const refreshCapabilities = useCallback(async () => {
    try {
      const [caps, exts] = await Promise.all([api.serverCapabilities(), api.listExtensions()]);
      setCapabilities(caps);
      setActiveExtensions(exts.filter((e) => e.enabled));
    } catch {
      // Un serveur muet ne doit pas vider l'interface : on garde ce qu'on a.
    }
  }, []);

  // Après la connexion, pas avant : sans session, le serveur ne dit rien de
  // ses extensions, et l'appel au démarrage renvoyait toujours une liste vide
  // — le Studio n'apparaissait donc jamais.
  useEffect(() => {
    if (!status?.signed_in) return;
    void refreshCapabilities();
  }, [status?.signed_in, refreshCapabilities]);

  const refresh = useCallback(async () => {
    // Cet appel est le tout premier que fait l'application. S'il échoue sans
    // être rattrapé, `status` reste nul, l'écran reste sur « loading », et
    // l'application affiche un rectangle vide — indéfiniment, sans rien dire.
    // C'est ce qu'on voyait sur un téléphone : une app qui démarre sur du noir.
    try {
      const s = await api.status();
      setStartupError(null);
      setStatus(s);
      remplacer(s.signed_in ? "chat" : "signin");
      return s;
    } catch (e) {
      setStartupError(e instanceof Error ? e.message : String(e));
      throw e;
    }
  }, [remplacer]);

  useEffect(() => {
    // `catch` vide : `refresh` a déjà retenu le message pour l'écran d'erreur.
    // Sans lui, l'échec ne serait qu'une promesse rejetée dans la console.
    refresh().catch(() => {});
  }, [refresh]);

  // N'importe quel écran peut être le premier à découvrir que le serveur ne
  // répond plus. Un seul endroit décide alors de proposer une reconnexion —
  // pas une fois par écran, ni un message qui se contente de le dire sans
  // offrir de le corriger. Rien n'est enregistré tant que la personne n'a
  // pas choisi : le compte et l'historique restent tels quels en attendant.
  useEffect(() => {
    return surEchecReseau(() => {
      setShowReconnect(true);
      setReconnectError(null);
    });
  }, []);

  async function reconnectViaAdresse(address: string) {
    setReconnectBusy(true);
    setReconnectError(null);
    try {
      setStatus(await api.reconnectActiveServer(address));
      setShowReconnect(false);
      await refresh();
    } catch (e) {
      setReconnectError(e instanceof Error ? e.message : String(e));
    } finally {
      setReconnectBusy(false);
    }
  }

  /**
   * A code was read — from the in-app scanner, or handed to us by Android
   * because the camera app opened a `locaryn://` link.
   *
   * The verification is entirely in Rust: this only decides what to show.
   */
  const applyLink = useCallback(
    async (uri: string) => {
      setScanError(null);
      try {
        // Deux sortes de codes se photographient de la même façon. Celui d'un
        // premier appairage porte la configuration du serveur — adresse et
        // certificat — et se reconnaît à ce qu'il commence par une accolade.
        // Celui du mode voyage est un lien signé, et sa vérification est
        // entièrement en Rust.
        if (uri.trimStart().startsWith("{")) {
          // Rien ne s'enregistre tout de suite : la caméra se refermait et le
          // téléphone parlait déjà à un autre serveur, sans que personne ait
          // vu à qui, ni par quel chemin. L'écran de confirmation s'interpose
          // avant que quoi que ce soit ne change.
          const apercu = lireApercu(uri);
          if (!apercu) {
            setScanError("Ce code ne contient pas une configuration lisible.");
            return;
          }
          setPendingProvisioning({ raw: uri, apercu });
          return;
        }
        const result = await api.applyPairingLink(uri);
        setPaired(result);
        await refresh();
      } catch (e) {
        setScanError(String(e));
      }
    },
    [refresh],
  );

  // Android hands over the link that launched the application. Asking for it
  // rather than waiting to be pushed means a link that arrived before the
  // interface existed is not lost.
  useEffect(() => {
    if (coreMode !== "tauri") return;
    let cancelled = false;
    void (async () => {
      try {
        const { onOpenUrl, getCurrent } = await import("@tauri-apps/plugin-deep-link");
        const launched = await getCurrent();
        if (!cancelled && launched?.length) await applyLink(launched[0]);
        await onOpenUrl((urls) => {
          if (urls.length) void applyLink(urls[0]);
        });
      } catch {
        // The plugin is absent on desktop builds; the in-app scanner covers
        // the same ground.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [applyLink]);

  // Le scan n'est pas un écran de la navigation — c'est une caméra ouverte
  // par-dessus l'écran courant — mais le bouton retour d'Android doit quand
  // même savoir quoi en faire. Sans cette entrée d'historique, le retour
  // matériel tombait tout droit sur le comportement natif : fermer
  // l'application, caméra ouverte, en plein milieu d'un appairage.
  useEffect(() => {
    if (!scanning) return;
    window.history.pushState({ scan: true }, "");
    function auRetour(e: PopStateEvent) {
      const etat = e.state as { scan?: boolean } | null;
      if (etat?.scan) return;
      void annulerScan();
    }
    window.addEventListener("popstate", auRetour);
    return () => window.removeEventListener("popstate", auRetour);
  }, [scanning]);

  const [provisioningSuccess, setProvisioningSuccess] = useState(false);

  /** La personne a vu à quel serveur elle se connecterait, et a confirmé. */
  async function confirmerProvisioning() {
    if (!pendingProvisioning) return;
    setConnecting(true);
    try {
      const nextStatus = await api.registerServer(pendingProvisioning.raw);
      setProvisioningSuccess(true);
      await new Promise((r) => setTimeout(r, 850));
      setStatus(nextStatus);
      setProvisioningSuccess(false);
      setPendingProvisioning(null);
      await refresh();
    } catch (e) {
      setScanError(e instanceof Error ? e.message : String(e));
      setPendingProvisioning(null);
    } finally {
      setConnecting(false);
    }
  }

  async function openScanner() {
    setScanError(null);
    if (!isScannerAvailable()) {
      setScanError("La caméra n'est pas disponible sur cet appareil.");
      return;
    }
    // Sans ce `catch`, un refus de l'appareil photo n'était qu'une promesse
    // rejetée : le bouton ne faisait visiblement rien.
    try {
      const text = await scan(setScanning);
      // L'entrée d'historique pour le retour n'a plus de raison d'être une
      // fois la caméra refermée — par annulation ou par une lecture réussie.
      if (window.history.state?.scan) window.history.back();
      if (text) await applyLink(text);
    } catch (e) {
      setScanError(e instanceof Error ? e.message : String(e));
    }
  }

  if (paired) {
    return <Paired result={paired} onDone={() => setPaired(null)} />;
  }

  if (pendingProvisioning) {
    return (
      <ConfirmServer
        apercu={pendingProvisioning.apercu}
        busy={connecting}
        success={provisioningSuccess}
        onConfirm={() => void confirmerProvisioning()}
        onCancel={() => setPendingProvisioning(null)}
      />
    );
  }

  if (startupError) {
    return (
      <div className="lo-screen">
        <div className="lo-center">
          <h1 className="lo-title">Locaryn n'a pas pu démarrer</h1>
          <p className="lo-sub">
            L'application n'a pas obtenu son état de départ. Rien n'est perdu : réessayez, et si
            cela persiste, réinstallez-la.
          </p>
          <p className="lo-error">{startupError}</p>
          <button type="button" className="lo-btn" onClick={() => void refresh().catch(() => {})}>
            Réessayer
          </button>
        </div>
      </div>
    );
  }

  if (!status) {
    // Un instant, rarement plus : une animation vaut mieux qu'un écran vide
    // qu'on pourrait prendre pour un plantage.
    return (
      <div className="lo-screen lo-center">
        <div className="lo-loading-row" role="status">
          <span className="lo-spinner" aria-hidden />
          <span>Connexion…</span>
        </div>
      </div>
    );
  }

  // Non connecté : seuls SignIn et Settings (pour version / mise à jour) sont accessibles.
  // Impossible d'atterrir sur Chat ou un écran nécessitant une session.
  if (!status.signed_in) {
    return (
      <>
        {screen === "settings" ? (
          <Settings
            status={status}
            onBack={() => {
              if (screen === "settings") revenir();
              else aller("signin");
            }}
            onSignedOut={(s) => {
              setStatus(s);
              remplacer("signin");
            }}
            onMemory={() => aller("memory")}
            onArchives={() => aller("archives")}
            onOpenChat={(sessionId) => {
              setRestoredChatId(sessionId);
              aller("chat");
            }}
          />
        ) : (
          <SignIn
            status={status}
            onSignedIn={(s) => {
              setStatus(s);
              remplacer("chat");
            }}
            onRegistered={setStatus}
            onScan={openScanner}
            onSettings={() => aller("settings")}
          />
        )}
        {scanning && <ScanOverlay onCancel={() => void annulerScan()} />}
        {scanError && (
          <div className="lo-toast">
            <p className="lo-error">{scanError}</p>
          </div>
        )}
      </>
    );
  }

  return (
    <>
      {screen === "chat" ? (
        <Chat
          status={status}
          capabilities={capabilities}
          onGo={(d, initialTab) => {
            // Le chat est l'accueil : le menu se referme, on n'y navigue pas.
            if (d === "chat") return;
            if (initialTab) setModelsTab(initialTab);
            aller(d);
          }}
          extensions={activeExtensions}
          key={figureChatId ?? restoredChatId ?? "chat"}
          initialId={figureChatId ?? restoredChatId}
          initialMedia={pendingMedia}
          onConsumedMedia={() => setPendingMedia(null)}
        />
      ) : screen === "archives" ? (
        <ArchivesScreen
          onBack={revenir}
          onOpenChat={(sessionId) => {
            setRestoredChatId(sessionId);
            aller("chat");
          }}
        />
      ) : screen === "figures" ? (
        <FiguresScreen
          onBack={revenir}
          onOpenChat={(sessionId) => {
            setFigureChatId(sessionId);
            aller("chat");
          }}
        />
      ) : screen === "studio" ? (
        <Studio
          onBack={revenir}
          extensions={activeExtensions}
          onSendToChat={(media) => {
            setPendingMedia(media);
            aller("chat");
          }}
        />
      ) : screen === "extensions" ? (
        <Extensions
          onBack={revenir}
          // Une extension retirée doit faire disparaître ce qu'elle apportait,
          // pas rester à l'écran jusqu'au prochain démarrage.
          onChanged={() => void refreshCapabilities()}
        />
      ) : screen === "models" ? (
        <Models onBack={revenir} initialTab={modelsTab} />
      ) : screen === "memory" ? (
        <MemoryScreen onBack={revenir} />
      ) : screen === "settings" ? (
        <Settings
          status={status}
          onBack={revenir}
          onSignedOut={(s) => {
            setStatus(s);
            remplacer("signin");
          }}
          onMemory={() => aller("memory")}
          onArchives={() => aller("archives")}
          onOpenChat={(sessionId) => {
            setRestoredChatId(sessionId);
            aller("chat");
          }}
        />
      ) : (
        <ExtensionView screenId={screen} onBack={revenir} onOpenChat={() => aller("chat")} />
      )}
      {/*
        Posé par-dessus l'écran, pas à la suite : les écrans font toute la
        hauteur, donc un message placé après eux tombait sous le pli d'une page
        qui ne défile pas — un code refusé ne disait rien du tout.
      */}
      {scanning && <ScanOverlay onCancel={() => void annulerScan()} />}
      {showReconnect && !scanning && (
        <ReconnectPrompt
          busy={reconnectBusy}
          error={reconnectError}
          onScan={() => {
            setShowReconnect(false);
            void openScanner();
          }}
          onAddress={(a) => void reconnectViaAdresse(a)}
          onDismiss={() => setShowReconnect(false)}
        />
      )}
      {scanError && (
        <div className="lo-toast">
          <p className="lo-error">{scanError}</p>
        </div>
      )}
    </>
  );
}
