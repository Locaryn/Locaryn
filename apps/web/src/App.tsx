import { useCallback, useEffect, useState } from "react";
import { Chat } from "./components/Chat";
import { Extensions } from "./components/Extensions";
import { FiguresScreen } from "./components/FiguresScreen";
import type { Destination } from "./components/MainMenu";
import { MemoryScreen } from "./components/MemoryScreen";
import { Models } from "./components/Models";
import { Settings } from "./components/Settings";
import { SignIn } from "./components/SignIn";
import { Studio } from "./components/Studio";
import { type WebStatus, api } from "./lib/core";
import { isIOS, isStandalone, rememberDismissed, rememberInstalled, shouldPrompt } from "./lib/pwa";

type Screen = "loading" | "signin" | "chat" | Destination | "memory";

/** Android's install event, captured so we can call prompt() on demand. */
interface BeforeInstallPromptEvent extends Event {
  prompt: () => Promise<void>;
  userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

export function App() {
  const [status, setStatus] = useState<WebStatus | null>(null);
  const [screen, setScreen] = useState<Screen>("loading");
  const [installEvent, setInstallEvent] = useState<BeforeInstallPromptEvent | null>(null);
  const [showPrompt, setShowPrompt] = useState(false);
  /** Ce que les extensions actives du serveur apportent, relu quand elles bougent. */
  const [capabilities, setCapabilities] = useState<string[]>([]);
  /** Une conversation neuve tenue par une figure, à ouvrir dans le chat. */
  const [figureChatId, setFigureChatId] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const s = await api.status();
    setStatus(s);
    // Un service qui n'écoute que sa propre machine sert l'API sans jeton :
    // demander un mot de passe n'y protège rien, et bloque net celui qui n'a
    // jamais créé de compte. On ne présente donc l'écran de connexion que
    // lorsque le service l'exige vraiment.
    setScreen(s.signed_in || !(await api.authRequired()) ? "chat" : "signin");
    return s;
  }, []);

  const refreshCapabilities = useCallback(async () => {
    try {
      setCapabilities(await api.serverCapabilities());
    } catch {
      // Un serveur muet ne doit pas vider l'interface : on garde ce qu'on a.
    }
  }, []);

  // Les capacités sont lues dès que le statut est connu : sans session,
  // `serverCapabilities` renvoie déjà une liste vide plutôt que d'échouer,
  // et sur loopback (pas de compte exigé) elles sont tout de suite là.
  useEffect(() => {
    if (!status) return;
    void refreshCapabilities();
  }, [status, refreshCapabilities]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // The service worker makes the shell usable offline once visited. It only
  // registers on HTTPS or localhost, which is exactly where it is allowed to.
  useEffect(() => {
    if ("serviceWorker" in navigator) {
      void navigator.serviceWorker.register("/sw.js").catch(() => {
        // Not a secure context (plain HTTP on a LAN): the site still works,
        // the install prompt is simply not available.
      });
    }
  }, []);

  // Android: capture the native install prompt. iOS: no event — the page
  // shows its own instructions instead. Either way, never twice.
  useEffect(() => {
    if (isIOS()) {
      if (shouldPrompt()) setShowPrompt(true);
      return;
    }
    const onPrompt = (e: Event) => {
      e.preventDefault();
      setInstallEvent(e as BeforeInstallPromptEvent);
      if (shouldPrompt()) setShowPrompt(true);
    };
    window.addEventListener("beforeinstallprompt", onPrompt);
    return () => window.removeEventListener("beforeinstallprompt", onPrompt);
  }, []);

  async function install() {
    if (installEvent) {
      // Android: the real, native install flow.
      await installEvent.prompt();
      const choice = await installEvent.userChoice;
      if (choice.outcome === "accepted") rememberInstalled();
      else rememberDismissed();
    } else {
      // iOS: the instructions are the offer; tapping "Installer" just marks
      // the visit so the question does not come back.
      rememberInstalled();
    }
    setShowPrompt(false);
  }

  function dismiss() {
    rememberDismissed();
    setShowPrompt(false);
  }

  function go(d: Destination) {
    setScreen(d);
  }

  return (
    <>
      {screen === "chat" && status ? (
        <Chat
          status={status}
          onGo={go}
          capabilities={capabilities}
          key={figureChatId ?? "chat"}
          initialId={figureChatId}
        />
      ) : screen === "studio" ? (
        <Studio onBack={() => setScreen("chat")} />
      ) : screen === "figures" ? (
        <FiguresScreen
          onBack={() => setScreen("chat")}
          onOpenChat={(sessionId) => {
            setFigureChatId(sessionId);
            setScreen("chat");
          }}
        />
      ) : screen === "extensions" ? (
        <Extensions
          onBack={() => setScreen("chat")}
          // Une extension retirée doit faire disparaître ce qu'elle apportait,
          // pas rester à l'écran jusqu'au prochain démarrage.
          onChanged={() => void refreshCapabilities()}
        />
      ) : screen === "models" ? (
        <Models onBack={() => setScreen("chat")} />
      ) : screen === "memory" ? (
        <MemoryScreen onBack={() => setScreen("chat")} />
      ) : screen === "settings" && status ? (
        <Settings
          status={status}
          onBack={() => setScreen("chat")}
          onSignedOut={(s) => {
            setStatus(s);
            setScreen("signin");
          }}
          onMemory={() => setScreen("memory")}
        />
      ) : (
        status && (
          <SignIn
            status={status}
            onSignedIn={(s) => {
              setStatus(s);
              setScreen("chat");
            }}
          />
        )
      )}

      {showPrompt && !isStandalone() && (
        <dialog
          open
          className="pwa-backdrop"
          aria-label="Installer Locaryn"
          onCancel={(e) => {
            e.preventDefault();
            dismiss();
          }}
        >
          <div className="pwa-banner">
            <p className="pwa-title">Installer Locaryn</p>
            <p className="pwa-text">
              {isIOS()
                ? "Touchez Partager (la flèche vers le haut), puis « Sur l'écran d'accueil » pour utiliser Locaryn comme une application."
                : "Installez Locaryn pour l'ouvrir en plein écran, comme une application."}
            </p>
            <div className="pwa-actions">
              <button type="button" className="lo-btn-ghost" onClick={dismiss}>
                Pas maintenant
              </button>
              <button type="button" className="lo-btn" onClick={install}>
                Installer
              </button>
            </div>
          </div>
        </dialog>
      )}
    </>
  );
}
