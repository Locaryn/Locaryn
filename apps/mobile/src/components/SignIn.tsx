import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type DiscoveredServer, type MobileStatus, api } from "../lib/core";

type Props = {
  status: MobileStatus;
  onSignedIn: (s: MobileStatus) => void;
  /** Un serveur vient d'être ajouté : l'écran doit repasser aux identifiants. */
  onRegistered: (s: MobileStatus) => void;
  onScan: () => void;
  /**
   * Les réglages, atteignables sans être connecté.
   */
  onSettings: () => void;
};

export function SignIn({ status, onSignedIn, onRegistered, onScan, onSettings }: Props) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [address, setAddress] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Animation de succès
  const [successInfo, setSuccessInfo] = useState<{ title: string; subtitle?: string } | null>(null);

  // Mode Découverte
  const [discoveryMode, setDiscoveryMode] = useState(false);
  const [discovering, setDiscovering] = useState(false);
  const [discoveredServers, setDiscoveredServers] = useState<DiscoveredServer[]>([]);
  const [discoveryDone, setDiscoveryDone] = useState(false);

  /** Lance la recherche de serveurs sur le réseau local. */
  const runDiscovery = useCallback(async () => {
    setDiscovering(true);
    setError(null);
    setDiscoveryDone(false);
    try {
      const list = await api.discoverServers();
      setDiscoveredServers(list);
    } catch (e) {
      setError(String(e));
    } finally {
      setDiscovering(false);
      setDiscoveryDone(true);
    }
  }, []);

  const openDiscovery = () => {
    setDiscoveryMode(true);
    void runDiscovery();
  };

  /** Enregistre le serveur depuis l'adresse tapée, puis passe à la connexion. */
  async function addByAddress(targetAddress?: string) {
    const raw = targetAddress ?? address;
    setBusy(true);
    setError(null);
    try {
      const s = await api.registerAddress(raw);
      setSuccessInfo({
        title: "Serveur enregistré !",
        subtitle: s.server_name ?? raw,
      });
      await new Promise((r) => setTimeout(r, 750));
      setSuccessInfo(null);
      onRegistered(s);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function submit() {
    if (!username.trim() || !password) {
      setError("Renseignez votre identifiant et votre mot de passe.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const next = await api.signIn(username.trim(), password);
      setPassword("");
      setSuccessInfo({
        title: "Connecté avec succès !",
        subtitle: next.server_name ?? "Session active",
      });
      await new Promise((r) => setTimeout(r, 900));
      setSuccessInfo(null);
      onSignedIn(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  // Écran Mode Découverte
  if (discoveryMode && status.servers === 0) {
    return (
      <div className="lo-screen">
        <div className="lo-bar">
          <button
            type="button"
            className="lo-bar-icon"
            onClick={() => setDiscoveryMode(false)}
            aria-label="Retour"
          >
            <Icon name="back" />
          </button>
          <span className="lo-bar-spacer" />
          <button
            type="button"
            className="lo-bar-icon"
            onClick={onSettings}
            aria-label="Réglages"
            title="Version, mise à jour"
          >
            <Icon name="settings" />
          </button>
        </div>

        <div className="lo-center" style={{ justifyContent: "flex-start", paddingTop: 20 }}>
          <h1 className="lo-title">Mode découverte</h1>
          <p className="lo-sub">Recherche des serveurs Locaryn sur votre réseau Wi-Fi…</p>

          {discovering && (
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                alignItems: "center",
                gap: 12,
                padding: "28px 0",
              }}
            >
              <div
                style={{
                  width: 40,
                  height: 40,
                  borderRadius: "50%",
                  border: "3px solid var(--border)",
                  borderTopColor: "var(--accent)",
                  animation: "lo-spin 0.8s linear infinite",
                }}
              />
              <span className="lo-hint">Balayage des adresses IP locales en cours…</span>
            </div>
          )}

          {discoveryDone && discoveredServers.length > 0 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 10, width: "100%" }}>
              <span className="lo-label">Serveurs détectés sur le réseau :</span>
              {discoveredServers.map((srv) => (
                <div
                  key={srv.url}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    padding: 14,
                    background: "var(--surface)",
                    border: "1px solid var(--border)",
                    borderRadius: "var(--radius)",
                    gap: 10,
                  }}
                >
                  <div style={{ minWidth: 0, flex: 1 }}>
                    <div style={{ fontWeight: 700, fontSize: 15, color: "var(--text)" }}>
                      {srv.name} {srv.version ? `(v${srv.version})` : ""}
                    </div>
                    <div
                      style={{
                        fontSize: 13,
                        color: "var(--text-faint)",
                        fontFamily: "var(--font-mono, monospace)",
                      }}
                    >
                      {srv.ip}:{srv.port}
                    </div>
                  </div>
                  <button
                    type="button"
                    className="lo-btn"
                    style={{ minHeight: 38, padding: "0 14px", width: "auto", fontSize: 13 }}
                    disabled={busy}
                    onClick={() => void addByAddress(srv.url)}
                  >
                    {busy ? "Connexion…" : "Choisir"}
                  </button>
                </div>
              ))}
            </div>
          )}

          {discoveryDone && discoveredServers.length === 0 && !discovering && (
            <div
              style={{
                padding: 16,
                background: "var(--surface)",
                border: "1px solid var(--border)",
                borderRadius: "var(--radius)",
                display: "flex",
                flexDirection: "column",
                gap: 8,
              }}
            >
              <div style={{ fontWeight: 600, color: "var(--text)" }}>Aucun serveur détecté</div>
              <p className="lo-hint">
                Certains routeurs ou box Wi-Fi bloquent le scan direct (isolation AP / pare-feu).
              </p>
              <p className="lo-hint" style={{ color: "var(--accent)" }}>
                Taper directement l'adresse IP ou scanner le QR code reste la méthode la plus
                directe et fiable.
              </p>
            </div>
          )}

          {error && <p className="lo-error">{error}</p>}

          <div style={{ display: "flex", flexDirection: "column", gap: 10, marginTop: "auto" }}>
            <button
              type="button"
              className="lo-btn-ghost"
              disabled={discovering}
              onClick={() => void runDiscovery()}
            >
              {discovering ? "Recherche en cours…" : "Relancer la recherche"}
            </button>
            <button type="button" className="lo-btn-ghost" onClick={() => setDiscoveryMode(false)}>
              Retour à la saisie manuelle
            </button>
          </div>
        </div>

        {successInfo && (
          <div className="lo-connection-feedback">
            <div className="lo-success-badge">
              <svg className="lo-checkmark-svg" viewBox="0 0 52 52">
                <circle className="lo-checkmark-circle" cx="26" cy="26" r="24" />
                <path className="lo-checkmark-check" d="M14 27l8 8 16-16" />
              </svg>
              <div style={{ fontWeight: 800, fontSize: 18, color: "var(--text)" }}>
                {successInfo.title}
              </div>
              {successInfo.subtitle && (
                <div style={{ fontSize: 14, color: "var(--text-faint)" }}>
                  {successInfo.subtitle}
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    );
  }

  // Première ouverture : aucun serveur n'est connu
  if (status.servers === 0) {
    return (
      <div className="lo-screen">
        <div className="lo-bar">
          <span className="lo-bar-spacer" />
          <button
            type="button"
            className="lo-bar-icon"
            onClick={onSettings}
            aria-label="Réglages"
            title="Version, mise à jour"
          >
            <Icon name="settings" />
          </button>
        </div>
        <div className="lo-center">
          <h1 className="lo-title">Locaryn</h1>
          <p className="lo-sub">Indiquez où joindre votre serveur.</p>

          <div>
            <label className="lo-label" htmlFor="a">
              Adresse du serveur
            </label>
            <input
              id="a"
              className="lo-input"
              autoCapitalize="none"
              autoCorrect="off"
              inputMode="url"
              placeholder="192.168.1.20"
              value={address}
              disabled={busy}
              onChange={(e) => setAddress(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void addByAddress()}
            />
          </div>

          <button
            type="button"
            className="lo-btn"
            disabled={busy || !address.trim()}
            onClick={() => void addByAddress()}
          >
            {busy ? "Connexion…" : "Continuer"}
          </button>

          {error && <p className="lo-error">{error}</p>}

          <p className="lo-hint">
            Le QR code affiché sur l'application PC (Réglages → Appareils) transmet l'adresse et le
            certificat sécurisé en un scan.
          </p>

          <button type="button" className="lo-btn-ghost" onClick={onScan}>
            Scanner un QR code
          </button>

          <div style={{ marginTop: 6 }}>
            <button
              type="button"
              className="lo-btn-ghost"
              style={{
                fontSize: 13,
                color: "var(--text-faint)",
                borderColor: "rgba(255, 255, 255, 0.08)",
              }}
              onClick={openDiscovery}
            >
              Mode découverte (Rechercher sur le réseau)
            </button>
          </div>
        </div>

        {successInfo && (
          <div className="lo-connection-feedback">
            <div className="lo-success-badge">
              <svg className="lo-checkmark-svg" viewBox="0 0 52 52">
                <circle className="lo-checkmark-circle" cx="26" cy="26" r="24" />
                <path className="lo-checkmark-check" d="M14 27l8 8 16-16" />
              </svg>
              <div style={{ fontWeight: 800, fontSize: 18, color: "var(--text)" }}>
                {successInfo.title}
              </div>
              {successInfo.subtitle && (
                <div style={{ fontSize: 14, color: "var(--text-faint)" }}>
                  {successInfo.subtitle}
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    );
  }

  // Écran de connexion (serveur déjà enregistré)
  return (
    <div className="lo-screen">
      <div className="lo-bar">
        <span className="lo-bar-spacer" />
        <button
          type="button"
          className="lo-bar-icon"
          onClick={onSettings}
          aria-label="Réglages"
          title="Version, mise à jour"
        >
          <Icon name="settings" />
        </button>
      </div>
      <div className="lo-center">
        <h1 className="lo-title">{status.server_name ?? "Locaryn"}</h1>
        <p className="lo-sub">
          {status.travelling ? "Connexion depuis l'extérieur." : "Connexion sur le réseau local."}
        </p>

        <div>
          <label className="lo-label" htmlFor="u">
            Identifiant
          </label>
          <input
            id="u"
            className="lo-input"
            autoCapitalize="none"
            autoCorrect="off"
            value={username}
            disabled={busy}
            onChange={(e) => setUsername(e.target.value)}
          />
        </div>

        <div>
          <label className="lo-label" htmlFor="p">
            Mot de passe
          </label>
          <input
            id="p"
            className="lo-input"
            type="password"
            value={password}
            disabled={busy}
            onChange={(e) => setPassword(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void submit()}
          />
        </div>

        <button type="button" className="lo-btn" disabled={busy} onClick={submit}>
          {busy ? "Connexion…" : "Se connecter"}
        </button>

        {error && <p className="lo-error">{error}</p>}

        <div style={{ marginTop: 8, textAlign: "center" }}>
          <button
            type="button"
            className="lo-btn-small"
            style={{
              fontSize: 12,
              color: "var(--text-faint)",
              background: "transparent",
              border: "none",
            }}
            onClick={() => onRegistered({ ...status, servers: 0 })}
          >
            Changer de serveur
          </button>
        </div>
      </div>

      {successInfo && (
        <div className="lo-connection-feedback">
          <div className="lo-success-badge">
            <svg className="lo-checkmark-svg" viewBox="0 0 52 52">
              <circle className="lo-checkmark-circle" cx="26" cy="26" r="24" />
              <path className="lo-checkmark-check" d="M14 27l8 8 16-16" />
            </svg>
            <div style={{ fontWeight: 800, fontSize: 18, color: "var(--text)" }}>
              {successInfo.title}
            </div>
            {successInfo.subtitle && (
              <div style={{ fontSize: 14, color: "var(--text-faint)" }}>{successInfo.subtitle}</div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
