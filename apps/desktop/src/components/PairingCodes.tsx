import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  type InstalledExtension,
  type PairingCode,
  type PairingMode,
  type ServerStatus,
  core,
} from "../lib/core";
import {
  LOCAL_STEPS,
  PairingCheckerboard,
  PairingStepList,
  REMOTE_STEPS,
  useStepProgress,
} from "./PairingSteps";
import { DynamicPluginWidget } from "./extensions/DynamicPluginWidget";
import { type ResolvedSlotContribution, getSlotContributions } from "./extensions/SlotRegistry";

/**
 * Panneau d'appairage.
 *
 * Il n'y a **pas** de code à saisir : le QR suffit. Il porte l'adresse et
 * l'empreinte du certificat, et le téléphone n'a rien à taper.
 *
 * Un seul mode est natif : le réseau local. L'accès distant et le tunnel ne
 * sont pas des variantes de celui-ci — ce sont d'autres transports, avec
 * d'autres risques, et l'application seule n'en propose aucun. Une extension
 * qui les apporte declare ses propres segments sur le point d'extension
 * ci-dessous : c'est elle qui nomme, decrit et dessine ce qu'elle ajoute.
 */
const SLOT_APPAIRAGE = "settings.server.pairing";

/** Le segment natif, et le seul que l'application connaisse d'elle-même. */
const SEGMENT_LOCAL: {
  id: string;
  label: string;
  description: string;
  apport?: ResolvedSlotContribution;
} = {
  id: "local",
  label: "Réseau local",
  description:
    "Pour un téléphone sur le même Wi‑Fi. Le QR porte l'adresse locale et l'empreinte du certificat ; mDNS évite la saisie.",
};

export function PairingCodes() {
  const [mode, setMode] = useState<string>("local");
  const [code, setCode] = useState<PairingCode | null>(null);
  const [server, setServer] = useState<ServerStatus | null>(null);
  const [extensions, setExtensions] = useState<InstalledExtension[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [availabilityError, setAvailabilityError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [enlarged, setEnlarged] = useState(false);
  /** Le sens du glissement : la carte suit la direction du déplacement. */
  const [sens, setSens] = useState<"a" | "b">("a");
  const precedent = useRef<string>("local");
  /** Un numero de passage : il s'incremente a chaque fabrication demandee. */
  const [passage, setPassage] = useState(0);

  // Les segments apportes par une extension. Elle en declare le nom, la
  // description et le panneau ; l'application ne fait que les ranger a la
  // suite du sien.
  const segments = useMemo<
    Array<{
      id: string;
      label: string;
      description: string;
      apport?: ResolvedSlotContribution;
    }>
  >(
    () => [
      SEGMENT_LOCAL,
      ...getSlotContributions(extensions, SLOT_APPAIRAGE).map((c) => ({
        id: `${c.extensionId}:${c.id}`,
        label: c.label ?? c.id,
        description: c.hint ?? "",
        apport: c,
      })),
    ],
    [extensions],
  );
  const choisi = segments.find((m) => m.id === mode) ?? SEGMENT_LOCAL;
  const natif = choisi.apport === undefined;

  const steps = LOCAL_STEPS;
  const step = useStepProgress(steps, passage);
  // Le QR n'apparaît que quand le service a répondu ET que les étapes sont
  // toutes franchies : afficher un code sous une étape en cours mentirait.
  const pret = !busy && step >= steps.length && Boolean(code?.qr_svg);

  useEffect(() => {
    let annule = false;
    const lire = () => {
      core
        .listExtensions()
        .then((l) => {
          if (!annule) setExtensions(l);
        })
        .catch(() => {
          if (!annule) setExtensions([]);
        });
    };
    lire();
    window.addEventListener("locaryn:extensions-changed", lire);
    return () => {
      annule = true;
      window.removeEventListener("locaryn:extensions-changed", lire);
    };
  }, []);

  const refreshAvailability = useCallback(async () => {
    try {
      setServer(await core.serverStatus());
      setAvailabilityError(null);
    } catch (e) {
      // Le démon peut être en train de démarrer ou de s'arrêter. Ce n'est pas
      // encore une erreur de QR : le prochain rafraîchissement tranchera.
      setAvailabilityError(String(e));
    }
  }, []);

  useEffect(() => {
    void refreshAvailability();
    // ServerSettings démarre un processus séparé. Re-lire l'état ici permet de
    // produire automatiquement le QR local après l'activation, sans demander
    // à l'utilisateur de changer d'onglet.
    const timer = window.setInterval(() => void refreshAvailability(), 2500);
    return () => window.clearInterval(timer);
  }, [refreshAvailability]);

  const charger = useCallback(async () => {
    setBusy(true);
    setError(null);
    setCode(null);
    setPassage((n) => n + 1);
    try {
      setCode(await core.pairingCode("local"));
    } catch (e) {
      setCode(null);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  // Le code local apparaît dès que le serveur est en écoute. Un segment apporte
  // par une extension a son propre panneau : rien a fabriquer ici.
  useEffect(() => {
    if (!server?.running || !natif) {
      setCode(null);
      setError(null);
      return;
    }
    void charger();
  }, [natif, server?.running, charger]);

  // Le segment choisi vient de disparaitre — l'extension a ete desactivee ou
  // retiree. On revient au seul mode que l'application tient d'elle-meme.
  useEffect(() => {
    if (segments.some((m) => m.id === mode)) return;
    setMode("local");
    precedent.current = "local";
  }, [segments, mode]);

  /**
   * Ce que l'hote prete au panneau d'une extension.
   *
   * Volontairement etroit : de quoi savoir si le service ecoute, et de quoi
   * lui demander un code pour un mode que l'extension nomme. L'application ne
   * connait pas ces modes — c'est l'extension qui sait ce qu'elle apporte, et
   * le service local qui sait le dessiner. Rien ici ne parle de tunnel : c'est
   * un canal, pas une liste.
   */
  const contexteAppairage = useMemo(
    () => ({
      serveurActif: Boolean(server?.running),
      demanderCode: (mode: string, url?: string) => core.pairingCode(mode as PairingMode, url),
      // Le partage sortant appartient au service, pas au panneau : il doit
      // survivre a la fermeture de la fenetre, et c'est le service qui produit
      // ensuite le code correspondant. Une extension qui ouvrirait le sien
      // aurait un lien que le code ne porterait pas.
      relaisDisponibles: () => core.travelRelays(),
      etatPartage: () => core.travelStatus(),
      // `null` ferme le partage ; un identifiant de relais l'ouvre avec lui.
      reglerPartage: (relais: string | null) => core.setTravelMode(relais),
    }),
    [server?.running],
  );

  const serverStopped = server !== null && !server.running;

  function choisir(next: string) {
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

      {/* Un seul segment ne se choisit pas : sans extension installee, le
          selecteur n'offrirait qu'une option deja active. */}
      {segments.length > 1 && (
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
      )}

      <div key={mode} className={`locaryn-pairing-card locaryn-mode-${sens}`}>
        <p className="locaryn-pairing-description">{choisi.description}</p>

        {/* ── Ce qu'une extension ajoute, c'est elle qui le dessine ──
            L'application ne connait ni son transport ni sa facon d'appairer :
            elle lui prete la place et le style de la carte, rien de plus. */}
        {choisi.apport && (
          <DynamicPluginWidget
            contribution={choisi.apport}
            className="locaryn-pairing-ext"
            context={contexteAppairage}
          />
        )}

        {serverStopped && natif && (
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
                Le code porte l'adresse et l'empreinte de cette machine. Après le scan, saisissez-y
                le code de confirmation ci-dessous.
              </p>
              {code.pairing_code && (
                <div className="locaryn-pairing-code">
                  <p className="locaryn-pairing-code-label">Code de confirmation</p>
                  <p className="locaryn-pairing-code-value">{code.pairing_code}</p>
                  {(code.pairing_ttl_seconds ?? 0) > 0 && (
                    <p className="locaryn-pairing-code-hint">
                      Valable {Math.round((code.pairing_ttl_seconds ?? 0) / 60)} min, à usage
                      unique.
                    </p>
                  )}
                </div>
              )}
              {code.pairing_code && (
                <div className="locaryn-pairing-code">
                  <p className="locaryn-pairing-code-label">
                    Код подтверждения — назовите его пользователю телефона
                  </p>
                  <p className="locaryn-pairing-code-value">{code.pairing_code}</p>
                  {(code.pairing_ttl_seconds ?? 0) > 0 && (
                    <p className="locaryn-pairing-code-hint">
                      Действует {Math.round((code.pairing_ttl_seconds ?? 0) / 60)} мин, одноразовый.
                    </p>
                  )}
                </div>
              )}
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
