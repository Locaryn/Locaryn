import { useCallback, useEffect, useState } from "react";
import { Chat } from "./components/Chat";
import { Paired } from "./components/Paired";
import { SignIn } from "./components/SignIn";
import { Studio } from "./components/Studio";
import { type MobileStatus, type PairingResult, api, coreMode } from "./lib/core";
import { isScannerAvailable, scan } from "./lib/scanner";

type Screen = "loading" | "signin" | "chat" | "studio";

export function App() {
  const [status, setStatus] = useState<MobileStatus | null>(null);
  const [screen, setScreen] = useState<Screen>("loading");
  const [paired, setPaired] = useState<PairingResult | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);
  const [startupError, setStartupError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    // Cet appel est le tout premier que fait l'application. S'il échoue sans
    // être rattrapé, `status` reste nul, l'écran reste sur « loading », et
    // l'application affiche un rectangle vide — indéfiniment, sans rien dire.
    // C'est ce qu'on voyait sur un téléphone : une app qui démarre sur du noir.
    try {
      const s = await api.status();
      setStartupError(null);
      setStatus(s);
      setScreen(s.signed_in ? "chat" : "signin");
      return s;
    } catch (e) {
      setStartupError(e instanceof Error ? e.message : String(e));
      throw e;
    }
  }, []);

  useEffect(() => {
    // `catch` vide : `refresh` a déjà retenu le message pour l'écran d'erreur.
    // Sans lui, l'échec ne serait qu'une promesse rejetée dans la console.
    refresh().catch(() => {});
  }, [refresh]);

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

  async function openScanner() {
    setScanError(null);
    if (!isScannerAvailable()) {
      setScanError("La caméra n'est pas disponible sur cet appareil.");
      return;
    }
    const text = await scan();
    if (text) await applyLink(text);
  }

  if (paired) {
    return <Paired result={paired} onDone={() => setPaired(null)} />;
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

  if (screen === "loading" || !status) {
    // Blank rather than a spinner: this lasts a few milliseconds.
    return <div className="lo-screen" />;
  }

  return (
    <>
      {screen === "chat" ? (
        <Chat status={status} onScan={openScanner} onStudio={() => setScreen("studio")} />
      ) : screen === "studio" ? (
        <Studio onBack={() => setScreen("chat")} />
      ) : (
        <SignIn
          status={status}
          onSignedIn={(s) => {
            setStatus(s);
            setScreen("chat");
          }}
          onScan={openScanner}
        />
      )}
      {scanError && (
        <div className="lo-pad">
          <p className="lo-error">{scanError}</p>
        </div>
      )}
    </>
  );
}
