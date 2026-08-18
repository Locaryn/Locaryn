import { useCallback, useEffect, useRef, useState } from "react";
import {
  type PairingCode,
  type PairingMode,
  type ServerStatus,
  type TravelStatus,
  core,
} from "../lib/core";

/**
 * Panneau des codes d'appairage.
 *
 * Le code local est disponible dès que le serveur est réellement en écoute.
 * Remote ajoute les deux modes extérieurs, mais ils restent explicitement
 * indisponibles tant que le tunnel n'est pas démarré : on ne fabrique jamais
 * un QR valide qui pointe vers une adresse qui ne répond pas.
 */
type QrChoice = PairingMode | "home";

const MODES: { id: PairingMode; label: string; description: string }[] = [
  {
    id: "local",
    label: "Réseau local",
    description: "Pour un téléphone connecté au même Wi‑Fi que cette machine.",
  },
  {
    id: "tunnel",
    label: "Remote · Tunnel sortant",
    description: "Pour joindre cette machine depuis Internet via le tunnel Remote.",
  },
  {
    id: "public",
    label: "Remote · Port ouvert",
    description: "Pour une adresse publique ou un port redirigé vers cette machine.",
  },
];

const HOME_MODE = {
  id: "home" as const,
  label: "Retour au réseau local",
  description: "À scanner pour repasser un téléphone sur l'adresse locale.",
};

export function PairingCodes({ remoteEnabled = false }: { remoteEnabled?: boolean }) {
  const [mode, setMode] = useState<QrChoice>("local");
  const [adresse, setAdresse] = useState("");
  const [code, setCode] = useState<PairingCode | null>(null);
  const [server, setServer] = useState<ServerStatus | null>(null);
  const [remote, setRemote] = useState<TravelStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [availabilityError, setAvailabilityError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [sens, setSens] = useState<"gauche" | "droite">("droite");
  const precedent = useRef<QrChoice>("local");

  const refreshAvailability = useCallback(async () => {
    try {
      const nextServer = await core.serverStatus();
      setServer(nextServer);
      setAvailabilityError(null);

      if (remoteEnabled) {
        setRemote(await core.travelStatus());
      } else {
        setRemote(null);
      }
    } catch (e) {
      // Le démon peut être en train de démarrer ou de s'arrêter. Ce n'est pas
      // encore une erreur de QR : le prochain rafraîchissement tranchera.
      setAvailabilityError(String(e));
    }
  }, [remoteEnabled]);

  useEffect(() => {
    void refreshAvailability();
    // ServerSettings démarre un processus séparé. Re-lire l'état ici permet de
    // produire automatiquement le QR local après l'activation, sans demander
    // à l'utilisateur de changer d'onglet.
    const timer = window.setInterval(() => void refreshAvailability(), 2500);
    return () => window.clearInterval(timer);
  }, [refreshAvailability]);

  const charger = useCallback(async (m: QrChoice, url?: string) => {
    setBusy(true);
    setError(null);
    try {
      if (m === "home") {
        const home = await core.travelHomeCode();
        setCode({ mode: "local", url: home.link ?? "", qr_svg: home.qr_svg ?? "" });
      } else {
        setCode(await core.pairingCode(m, url));
      }
    } catch (e) {
      setCode(null);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  // Un code local doit apparaître dès que le serveur passe réellement en
  // écoute. Un code tunnel ou de retour attend que le plugin Remote ait ouvert
  // le tunnel.
  useEffect(() => {
    if (!server?.running) {
      setCode(null);
      setError(null);
      return;
    }
    if (
      mode === "public" ||
      (mode === "tunnel" && !remote?.active) ||
      (mode === "home" && !remote?.active)
    ) {
      setCode(null);
      setError(null);
      return;
    }
    void charger(mode);
  }, [mode, server?.running, remote?.active, charger]);

  useEffect(() => {
    if (remoteEnabled && (mode !== "home" || remote?.active)) return;
    if (!remoteEnabled && mode === "local") return;
    setMode("local");
    setCode(null);
    setAdresse("");
    setError(null);
    precedent.current = "local";
  }, [remoteEnabled, mode, remote?.active]);

  const modes: { id: QrChoice; label: string; description: string }[] = remoteEnabled
    ? [...MODES, ...(remote?.active ? [HOME_MODE] : [])]
    : MODES.slice(0, 1);
  const choisi = modes.find((m) => m.id === mode) ?? MODES[0];
  const serverStopped = server !== null && !server.running;
  const tunnelStopped = mode === "tunnel" && !remote?.active;
  const homeUnavailable = mode === "home" && !remote?.active;

  function choisir(next: QrChoice) {
    const from = modes.findIndex((x) => x.id === precedent.current);
    const to = modes.findIndex((x) => x.id === next);
    precedent.current = next;
    setMode(next);
    setSens(to >= from ? "droite" : "gauche");
    setError(null);
    setCode(null);
  }

  return (
    <aside className="locaryn-pairing-panel" aria-label="Codes QR d'appairage">
      <div className="locaryn-pairing-head">
        <div>
          <div className="locaryn-pairing-kicker">Appairage</div>
          <h3 className="locaryn-pairing-title">Code QR à afficher</h3>
        </div>
        <span className={`locaryn-pairing-status${server?.running ? " is-on" : ""}`}>
          {server?.running ? "Serveur actif" : "Serveur arrêté"}
        </span>
      </div>

      <p className="locaryn-pairing-intro">
        Choisissez le chemin par lequel le téléphone rejoindra cette machine, puis faites scanner le
        code.
      </p>

      <label className="locaryn-pairing-select-label" htmlFor="locaryn-pairing-mode">
        Code à afficher
      </label>
      <select
        id="locaryn-pairing-mode"
        className="locaryn-select locaryn-pairing-select"
        value={mode}
        onChange={(e) => choisir(e.target.value as QrChoice)}
      >
        {modes.map((m) => (
          <option key={m.id} value={m.id}>
            {m.label}
          </option>
        ))}
      </select>
      <p className="locaryn-pairing-description">{choisi.description}</p>

      {mode === "public" && remoteEnabled && (
        <div className="locaryn-pairing-public">
          <label className="locaryn-pairing-select-label" htmlFor="locaryn-pairing-address">
            Adresse publique
          </label>
          <div className="locaryn-pairing-public-row">
            <input
              id="locaryn-pairing-address"
              className="locaryn-input"
              placeholder="maison.exemple:7474"
              value={adresse}
              onChange={(e) => {
                setAdresse(e.target.value);
                setError(null);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" && server?.running && adresse.trim()) {
                  void charger("public", adresse);
                }
              }}
            />
            <button
              type="button"
              className="locaryn-btn-ghost"
              disabled={busy || !server?.running || !adresse.trim()}
              onClick={() => void charger("public", adresse)}
            >
              Générer
            </button>
          </div>
        </div>
      )}

      {serverStopped && (
        <div className="locaryn-pairing-notice">
          Activez <strong>Serveur actif</strong> dans la colonne de gauche. Le code local apparaîtra
          automatiquement dès que le service sera en écoute.
        </div>
      )}

      {mode === "tunnel" && remoteEnabled && tunnelStopped && !serverStopped && (
        <div className="locaryn-pairing-notice">
          Activez d'abord le mode <strong>Remote</strong> dans la colonne de gauche. Le code tunnel
          sera généré dès que le relais répondra.
        </div>
      )}

      {mode === "home" && remoteEnabled && homeUnavailable && !serverStopped && (
        <div className="locaryn-pairing-notice">
          Le code de retour sera disponible tant que le tunnel <strong>Remote</strong> est actif.
        </div>
      )}

      {server?.blocker && serverStopped && (
        <div className="locaryn-pairing-warning">{server.blocker}</div>
      )}

      {busy && <p className="locaryn-pairing-loading">Génération du code…</p>}

      {code?.qr_svg && (
        <div
          key={`${mode}-${code.url}`}
          className={`locaryn-travel-code locaryn-qr-entre locaryn-qr-${sens}`}
        >
          <div
            className="locaryn-travel-qr"
            // biome-ignore lint/security/noDangerouslySetInnerHtml: le SVG est produit par le démon local et ne contient pas de script.
            dangerouslySetInnerHTML={{ __html: code.qr_svg }}
          />
          <div className="locaryn-travel-say">
            <p className="locaryn-travel-title">Scannez avec le téléphone</p>
            <p className="locaryn-travel-sub">
              Le code contient l'adresse et le certificat de cette machine. Rien à saisir sur le
              téléphone.
            </p>
            <button
              type="button"
              className="locaryn-btn-ghost"
              onClick={() =>
                void navigator.clipboard
                  .writeText(code.url)
                  .then(() => {
                    setCopied(true);
                    window.setTimeout(() => setCopied(false), 1500);
                  })
                  .catch(() => {})
              }
            >
              {copied ? "Adresse copiée" : "Copier l'adresse"}
            </button>
          </div>
        </div>
      )}

      {!busy && code && !code.qr_svg && (
        <div className="locaryn-pairing-warning">Le service n'a pas renvoyé d'image QR.</div>
      )}
      {availabilityError && !server && (
        <div className="locaryn-pairing-warning">{availabilityError}</div>
      )}
      {error && <div className="locaryn-vp-error">{error}</div>}
    </aside>
  );
}
