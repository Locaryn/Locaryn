import { Icon, type IconName } from "@locaryn/ui-core";
import { type FormEvent, useCallback, useEffect, useState } from "react";
import { SEGMENT_LOCAL, useSegmentsAppairage } from "../hooks/useSegmentsAppairage";
import {
  type AccountDevice,
  type AccountDevicesReport,
  type DeviceClient,
  type DeviceConnection,
  type MachineAccount,
  core,
} from "../lib/core";
import { confirmer } from "../lib/dialog";
import { useModeAcces } from "../lib/modeAcces";
import { UserLoginQr } from "./UserLoginQr";

const CLIENTS: Record<DeviceClient, { icon: IconName; label: string }> = {
  mobile: { icon: "device-mobile", label: "Application mobile" },
  web: { icon: "globe", label: "Interface web" },
  desktop: { icon: "monitor", label: "Application PC" },
  cli: { icon: "terminal-window", label: "Terminal" },
  app: { icon: "devices", label: "Application" },
};

const METHODES: Record<string, string> = {
  password: "Connexion par mot de passe",
  code: "Connexion par QR code",
  pairing: "Appairage",
  new_address: "Nouvelle adresse",
};

/** « 12 novembre 2026 à 09:41:27 », à l'heure de cette machine. */
function horodatage(iso: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString("fr-FR", {
    day: "numeric",
    month: "long",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function messageErreur(e: unknown): string {
  return String(e).replace(/^Error:\s*/, "");
}

type Props = { account: MachineAccount };

/**
 * L'onglet « Appareils et sécurité » d'un compte du serveur : mot de passe,
 * QR de connexion, appareils connectés et leur historique, à la manière des
 * pages « appareils » des grands services.
 */
export function DevicesSecurity({ account }: Props) {
  return (
    <div className="locaryn-devsec">
      <PasswordCard />
      <LoginQrCard account={account} />
      <DevicesCard />
    </div>
  );
}

function PasswordCard() {
  const [actuel, setActuel] = useState("");
  const [nouveau, setNouveau] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [busy, setBusy] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);
  const [fait, setFait] = useState(false);

  async function envoyer(e: FormEvent) {
    e.preventDefault();
    setFait(false);
    if (nouveau.length < 8) return setErreur("8 caractères minimum.");
    if (nouveau !== confirmation) return setErreur("Les deux mots de passe diffèrent.");
    setBusy(true);
    setErreur(null);
    try {
      await core.machineAccountPassword(actuel, nouveau);
      setActuel("");
      setNouveau("");
      setConfirmation("");
      setFait(true);
    } catch (err) {
      setErreur(messageErreur(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className="locaryn-card locaryn-devsec-card" onSubmit={(e) => void envoyer(e)}>
      <h3>
        <Icon name="lock" size={16} /> Mot de passe
      </h3>
      <p className="locaryn-field-hint">
        Les appareils déjà connectés le restent : déconnectez-les plus bas si besoin.
      </p>
      <div className="locaryn-devsec-fields">
        <PasswordField
          id="devsec-actuel"
          label="Mot de passe actuel"
          value={actuel}
          onChange={setActuel}
          autoComplete="current-password"
        />
        <PasswordField
          id="devsec-nouveau"
          label="Nouveau mot de passe"
          value={nouveau}
          onChange={setNouveau}
          autoComplete="new-password"
        />
        <PasswordField
          id="devsec-confirmation"
          label="Confirmer"
          value={confirmation}
          onChange={setConfirmation}
          autoComplete="new-password"
        />
      </div>
      {erreur && <div className="locaryn-vp-error">{erreur}</div>}
      <div className="locaryn-field-actions">
        <button
          type="submit"
          className="locaryn-btn-primary"
          disabled={busy || !actuel || !nouveau}
        >
          {busy ? "Modification…" : "Changer le mot de passe"}
        </button>
        {fait && <span className="locaryn-account-saved">Mot de passe changé</span>}
      </div>
    </form>
  );
}

type PasswordFieldProps = {
  id: string;
  label: string;
  value: string;
  onChange: (v: string) => void;
  autoComplete: string;
};

function PasswordField({ id, label, value, onChange, autoComplete }: PasswordFieldProps) {
  return (
    <div className="locaryn-field">
      <label className="locaryn-field-label" htmlFor={id}>
        {label}
      </label>
      <input
        id={id}
        type="password"
        className="locaryn-input"
        value={value}
        autoComplete={autoComplete}
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  );
}

function LoginQrCard({ account }: Props) {
  const [ouvert, setOuvert] = useState(false);
  const [enMarche, setEnMarche] = useState<boolean | null>(null);
  const segments = useSegmentsAppairage();
  const { segment, transport } = useModeAcces();
  const choisi = segments.find((m) => m.id === segment) ?? SEGMENT_LOCAL;

  useEffect(() => {
    core
      .serverStatus()
      .then((s) => setEnMarche(s.running))
      .catch((e) => {
        console.warn("[Appareils] état du serveur illisible :", e);
        setEnMarche(false);
      });
  }, []);
  const pret = enMarche === true && (segment === "local" || transport !== null);

  return (
    <div className="locaryn-card locaryn-devsec-card">
      <h3>
        <Icon name="qr-code" size={16} /> Connecter un appareil
      </h3>
      <p className="locaryn-field-hint">
        {pret
          ? "Scannez ce QR depuis l'application mobile : la session de ce compte s'ouvre sans mot de passe."
          : "Démarrez le serveur dans « Serveur & fonctions » pour afficher le QR de connexion."}
      </p>
      <div className="locaryn-field-actions">
        <button
          type="button"
          className="locaryn-btn-ghost"
          disabled={!pret}
          onClick={() => setOuvert(true)}
        >
          Afficher le QR de connexion
        </button>
      </div>
      {ouvert && (
        <UserLoginQr
          userId={account.id}
          username={account.username}
          distant={choisi.id === SEGMENT_LOCAL.id ? null : choisi.label}
          onClose={() => setOuvert(false)}
        />
      )}
    </div>
  );
}

function DevicesCard() {
  const [rapport, setRapport] = useState<AccountDevicesReport | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);

  const charger = useCallback(async () => {
    try {
      setRapport(await core.machineAccountDevices());
      setErreur(null);
    } catch (e) {
      setErreur(messageErreur(e));
    }
  }, []);

  useEffect(() => {
    void charger();
  }, [charger]);

  async function deconnecter(d: AccountDevice) {
    const nom = d.label ?? CLIENTS[d.client].label;
    if (!(await confirmer(`Déconnecter « ${nom} » ? Il devra se reconnecter.`))) return;
    try {
      await core.machineAccountRevoke(d.token_id);
      await charger();
    } catch (e) {
      setErreur(messageErreur(e));
    }
  }

  const actifs = rapport?.devices.filter((d) => !estFerme(d)) ?? [];
  const fermes = rapport?.devices.filter(estFerme) ?? [];
  return (
    <div className="locaryn-card locaryn-devsec-card">
      <div className="locaryn-devsec-head">
        <h3>
          <Icon name="devices" size={16} /> Appareils connectés
        </h3>
        <button type="button" className="locaryn-btn-ghost" onClick={() => void charger()}>
          Actualiser
        </button>
      </div>
      {erreur && <div className="locaryn-vp-error">{erreur}</div>}
      {!rapport && !erreur && <p className="locaryn-field-hint">Chargement…</p>}
      {rapport && actifs.length === 0 && (
        <p className="locaryn-field-hint">Aucun appareil n'est connecté à ce compte.</p>
      )}
      <ul className="locaryn-devsec-list">
        {actifs.map((d) => (
          <DeviceRow key={d.token_id} device={d} onRevoke={() => void deconnecter(d)} />
        ))}
      </ul>
      {fermes.length > 0 && (
        <details className="locaryn-devsec-closed">
          <summary>Sessions terminées ({fermes.length})</summary>
          <ul className="locaryn-devsec-list">
            {fermes.map((d) => (
              <DeviceRow key={d.token_id} device={d} />
            ))}
          </ul>
        </details>
      )}
      {rapport && (
        <GeolocationNote
          installed={rapport.geolocation}
          attribution={rapport.attribution}
          onInstalled={charger}
        />
      )}
    </div>
  );
}

function estFerme(d: AccountDevice): boolean {
  if (d.revoked_at) return true;
  return d.expires_at !== null && new Date(d.expires_at).getTime() < Date.now();
}

type DeviceRowProps = { device: AccountDevice; onRevoke?: () => void };

function DeviceRow({ device, onRevoke }: DeviceRowProps) {
  const client = CLIENTS[device.client] ?? CLIENTS.app;
  const derniere = device.history[0];
  const lieu = [derniere?.ip, derniere?.location].filter(Boolean).join(" · ");
  return (
    <li className="locaryn-devsec-device">
      <div className="locaryn-devsec-device-main">
        <span className="locaryn-devsec-icon" title={client.label}>
          <Icon name={client.icon} size={18} />
        </span>
        <div className="locaryn-devsec-device-text">
          <strong>
            {device.label ?? client.label}
            {device.current && <span className="locaryn-devsec-tag">Cet appareil</span>}
            {estFerme(device) && <span className="locaryn-devsec-tag">Déconnecté</span>}
          </strong>
          <span>
            {client.label} · dernière activité le{" "}
            {horodatage(device.last_used_at ?? device.created_at)}
          </span>
          {lieu && <span>{lieu}</span>}
        </div>
        {onRevoke && (
          <button type="button" className="locaryn-btn-ghost locaryn-btn-danger" onClick={onRevoke}>
            Déconnecter
          </button>
        )}
      </div>
      {device.history.length > 0 && (
        <details className="locaryn-devsec-history">
          <summary>Historique des connexions ({device.history.length})</summary>
          <ol>
            {device.history.map((c) => (
              <ConnectionLine key={`${c.at}-${c.ip ?? ""}-${c.method}`} connection={c} />
            ))}
          </ol>
        </details>
      )}
    </li>
  );
}

function ConnectionLine({ connection: c }: { connection: DeviceConnection }) {
  const client = CLIENTS[c.client] ?? CLIENTS.app;
  return (
    <li>
      <span className="locaryn-devsec-icon locaryn-devsec-icon-small" title={client.label}>
        <Icon name={client.icon} size={14} />
      </span>
      <time dateTime={c.at}>{horodatage(c.at)}</time>
      <span>{c.ip ?? "adresse inconnue"}</span>
      <span>{c.location ?? "lieu inconnu"}</span>
      <span className="locaryn-devsec-method">{METHODES[c.method] ?? c.method}</span>
    </li>
  );
}

type GeoProps = { installed: boolean; attribution: string; onInstalled: () => Promise<void> };

function GeolocationNote({ installed, attribution, onInstalled }: GeoProps) {
  const [busy, setBusy] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);

  async function installer() {
    setBusy(true);
    setErreur(null);
    try {
      await core.installGeolocation();
      await onInstalled();
    } catch (e) {
      setErreur(messageErreur(e));
    } finally {
      setBusy(false);
    }
  }

  if (installed) return <p className="locaryn-devsec-attribution">{attribution}</p>;
  return (
    <div className="locaryn-devsec-geo">
      <p className="locaryn-field-hint">
        Pour situer les adresses publiques (ville, pays), Locaryn peut installer une base de
        géolocalisation sur ce PC (DB-IP Lite, environ 130 Mo). Seul le fichier est téléchargé
        depuis db-ip.com : aucune adresse IP n'est envoyée à qui que ce soit.
      </p>
      {erreur && <div className="locaryn-vp-error">{erreur}</div>}
      <button
        type="button"
        className="locaryn-btn-ghost"
        disabled={busy}
        onClick={() => void installer()}
      >
        {busy ? "Téléchargement…" : "Installer la base de géolocalisation"}
      </button>
    </div>
  );
}
