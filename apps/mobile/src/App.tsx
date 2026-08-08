import { useCallback, useEffect, useState } from "react";
import { api, coreMode, type MobileStatus, type PairingResult } from "./lib/core";
import { Chat } from "./components/Chat";
import { Paired } from "./components/Paired";
import { SignIn } from "./components/SignIn";
import { scan, isScannerAvailable } from "./lib/scanner";

type Screen = "loading" | "signin" | "chat";

export function App() {
  const [status, setStatus] = useState<MobileStatus | null>(null);
  const [screen, setScreen] = useState<Screen>("loading");
  const [paired, setPaired] = useState<PairingResult | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const s = await api.status();
    setStatus(s);
    setScreen(s.signed_in ? "chat" : "signin");
    return s;
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  /**
   * A code was read — from the in-app scanner, or handed to us by Android
   * because the camera app opened a `lochor://` link.
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

  if (screen === "loading" || !status) {
    // Blank rather than a spinner: this lasts a few milliseconds.
    return <div className="lo-screen" />;
  }

  return (
    <>
      {screen === "chat" ? (
        <Chat status={status} onScan={openScanner} />
      ) : (
        <SignIn status={status} onSignedIn={(s) => { setStatus(s); setScreen("chat"); }} onScan={openScanner} />
      )}
      {scanError && (
        <div className="lo-pad">
          <p className="lo-error">{scanError}</p>
        </div>
      )}
    </>
  );
}
