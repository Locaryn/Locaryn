import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  type PairingCode,
  type PairingMode,
  type ServerStatus,
  type TravelStatus,
  core,
} from "../lib/core";
import {
  LOCAL_STEPS,
  PairingCheckerboard,
  PairingStepList,
  REMOTE_STEPS,
  useStepProgress,
} from "./PairingSteps";

/**
 * Panneau d'appairage.
 *
 * Il n'y a **pas** de code à saisir : le QR suffit. Il porte l'adresse et
 * l'empreinte du certificat, et le téléphone n'a rien à taper.
 *
 * Deux modes sont natifs — le réseau local et l'accès distant par port ouvert.
 * Le troisième segment, le tunnel, n'affiche aucun QR : rien n'est natif, ce
 * sont les extensions qui déclarent leur propre mode d'appairage.
 */
type QrChoice = PairingMode | "home";

const SEGMENTS: { id: QrChoice; label: string; description: string }[] = [
  {
    id: "local",
    label: "Réseau local",
    description:
      "Pour un téléphone sur le même Wi‑Fi. Le QR porte l'adresse locale et l'empreinte du certificat ; mDNS évite la saisie.",
  },
  {
    id: "public",
    label: "Accès distant",
    description:
      "Pour une adresse publique ou un port redirigé vers cette machine. Le QR porte l'adresse, le port et l'empreinte.",
  },
  {
    id: "tunnel",
    label: "Tunnel",
    description: "Rien n'est natif ici : ce sont les extensions qui ouvrent un tunnel sortant.",
  },
];

const HOME_SEGMENT = {
  id: "home" as const,
  label: "Retour au local",
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
  const [enlarged, setEnlarged] = useState(false);
  /** Le sens du glissement : la carte suit la direction du déplacement. */
  const [sens, setSens] = useState<"a" | "b">("a");
  const precedent = useRef<QrChoice>("local");

  const steps = mode === "local" ? LOCAL_STEPS : REMOTE_STEPS;
  const step = useStepProgress(steps, busy);
  // Le QR n'apparaît que quand le service a répondu ET que les étapes sont
  // toutes franchies : afficher un code sous une étape en cours mentirait.
  const pret = !busy && step >= steps.length && Boolean(code?.qr_svg);

  const refreshAvailability = useCallback(async () => {
    try {
      const nextServer = await core.serverStatus();
      setServer(nextServer);
      setAvailabilityError(null);
      setRemote(remoteEnabled ? await core.travelStatus() : null);
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
    setCode(null);
    try {
      if (m === "home") {
        const home = await core.travelHomeCode();
        setCode({ mode: "local", url: home.link ?? "", qr_svg: home.qr_svg ?? "" });
      } else {
        setCode(await core.pairingCode(m as PairingMode, url));
      }
    } catch (e) {
      setCode(null);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  // Le code local apparaît dès que le serveur est en écoute. L'accès distant
  // attend une adresse, et le tunnel n'a rien de natif à montrer.
  useEffect(() => {
    if (!server?.running || mode === "public" || mode === "tunnel") {
      setCode(null);
      setError(null);
      return;
    }
    if (mode === "home" && !remote?.active) {
      setCode(null);
      setError(null);
      return;
    }
    void charger(mode);
  }, [mode, server?.running, remote?.active, charger]);

  useEffect(() => {
    if (remoteEnabled || mode !== "home") return;
    setMode("local");
    setCode(null);
    precedent.current = "local";
  }, [remoteEnabled, mode]);

  const segments: typeof SEGMENTS =
    remoteEnabled && remote?.active ? [...SEGMENTS, HOME_SEGMENT] : SEGMENTS;
  const choisi = segments.find((m) => m.id === mode) ?? SEGMENTS[0];
  const serverStopped = server !== null && !server.running;

  function choisir(next: QrChoice) {
    const from = segments.findIndex((x) => x.id === precedent.current);
    const to = segments.findIndex((x) => x.id === next);
    precedent.current = next;
    setSens(to >= from ? "a" : "b");
    setMode(next);
    setError(null);
    setCode(null);
  }

  return (
    <aside className="locaryn-pairing-panel" aria-label="Appairage par QR">
      <div className="locaryn-pairing-head">
        <div>
          <div className="locaryn-pairing-kicker">Appairage</div>
          <h3 className="locaryn-pairing-title">Code QR à faire scanner</h3>
        </div>
        <span className={`locaryn-pairing-status${server?.running ? " is-on" : ""}`}>
          {server?.running ? "Serveur actif" : "Serveur arrêté"}
        </span>
      </div>

      <p className="locaryn-pairing-intro">
        Rien à saisir sur le téléphone : le code porte l'adresse et l'empreinte du certificat.
      </p>

      <div className="locaryn-segmented locaryn-pairing-segments" role="group">
        {segments.map((m) => (
          <button
            key={m.id}
            type="button"
            className={`locaryn-segment${mode === m.id ? " locaryn-segment-on" : ""}`}
            aria-pressed={mode === m.id}
            onClick={() => choisir(m.id)}
          >
            {m.label}
          </button>
        ))}
      </div>

      <div key={mode} className={`locaryn-pairing-card locaryn-mode-${sens}`}>
        <p className="locaryn-pairing-description">{choisi.description}</p>

        {/* ── Le tunnel n'est pas natif : il vit dans une extension ── */}
        {mode === "tunnel" && (
          <div className="locaryn-pairing-notice">
            <Icon name="extensions" size={15} />
            <span>
              Le tunnel sortant est apporté par une <strong>extension</strong>, qui déclare son
              propre mode d'appairage. Installez-la depuis Paramètres → Morphs &amp; Skills.
            </span>
          </div>
        )}

        {mode === "public" && (
          <div className="locaryn-pairing-public">
            <label className="locaryn-pairing-select-label" htmlFor="locaryn-pairing-address">
              Adresse publique et port
            </label>
            <div className="locaryn-pairing-public-row">
              <input
                id="locaryn-pairing-address"
                className="locaryn-input"
                placeholder="maison.exemple:7443"
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

        {serverStopped && mode !== "tunnel" && (
          <div className="locaryn-pairing-notice">
            <Icon name="server" size={15} />
            <span>
              Activez <strong>Serveur actif</strong> dans la colonne de gauche. Le code apparaîtra
              dès que le service sera en écoute.
            </span>
          </div>
        )}

        {server?.blocker && serverStopped && (
          <div className="locaryn-pairing-warning">{server.blocker}</div>
        )}

        {/* ── Pendant la fabrication : le damier pulse, les étapes s'égrènent ── */}
        {(busy || (code && !pret)) && (
          <div className="locaryn-pair-making">
            <PairingCheckerboard />
            <PairingStepList steps={steps} current={step} />
          </div>
        )}

        {pret && code && (
          <div className="locaryn-travel-code">
            <button
              type="button"
              className="locaryn-travel-qr"
              onClick={() => setEnlarged(true)}
              title="Afficher le code en grand"
            >
              {/* biome-ignore lint/security/noDangerouslySetInnerHtml: le SVG est produit par le démon local et ne contient pas de script. */}
              <div dangerouslySetInnerHTML={{ __html: code.qr_svg }} />
            </button>
            <div className="locaryn-travel-say">
              <p className="locaryn-travel-title">Scannez avec le téléphone</p>
              <p className="locaryn-travel-sub">
                Le code porte l'adresse et l'empreinte de cette machine. Rien à saisir.
              </p>
              <div className="locaryn-pairing-actions">
                <button
                  type="button"
                  className="locaryn-btn-ghost"
                  onClick={() => setEnlarged(true)}
                >
                  Plein écran
                </button>
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
                      .catch((e) => setError(String(e)))
                  }
                >
                  {copied ? "Adresse copiée" : "Copier l'adresse"}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>

      {enlarged && code?.qr_svg && (
        <dialog open className="locaryn-qr-overlay" aria-label="Code QR d'appairage">
          <button
            type="button"
            className="locaryn-qr-overlay-veil"
            aria-label="Fermer"
            onClick={() => setEnlarged(false)}
          />
          <div className="locaryn-qr-overlay-card">
            <div className="locaryn-qr-overlay-head">
              <div>
                <h3>Code QR d'appairage</h3>
                <span className="locaryn-field-hint">{choisi.label}</span>
              </div>
              <button
                type="button"
                className="locaryn-icon-btn"
                onClick={() => setEnlarged(false)}
                aria-label="Fermer"
              >
                <Icon name="close" size={16} />
              </button>
            </div>
            <div
              className="locaryn-qr-overlay-code"
              // biome-ignore lint/security/noDangerouslySetInnerHtml: SVG local
              dangerouslySetInnerHTML={{ __html: code.qr_svg }}
            />
            <p className="locaryn-field-hint">
              Pointez la caméra du téléphone vers ce code : la connexion se fait seule.
            </p>
          </div>
        </dialog>
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
