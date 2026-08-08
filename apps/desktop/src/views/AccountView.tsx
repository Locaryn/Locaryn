import { useState } from "react";

export function AccountView() {
  const [serverUrl, setServerUrl] = useState("https://private.lochor.internal");
  const [token, setToken] = useState("");
  const [username, setUsername] = useState("Teano");
  const [isConnected, setIsConnected] = useState(true);

  return (
    <section className="lochor-view-container">
      <div className="lochor-view-header">
        <h2>Gestion du Compte & Serveur Privé</h2>
        <p className="lochor-view-desc">
          Connectez votre instance desktop à un serveur privé distant pour la synchronisation, les modèles hébergés et l'exécution distante.
        </p>
      </div>

      <div className="lochor-card" style={{ maxWidth: "600px" }}>
        <h3>Profil Utilisateur</h3>
        <div className="lochor-field">
          <label className="lochor-field-label">Nom d'utilisateur / Alias</label>
          <input
            className="lochor-input"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
          />
        </div>

        <h3 style={{ marginTop: "24px" }}>Connexion Serveur Privé (Gateway)</h3>
        <div className="lochor-field">
          <label className="lochor-field-label">URL du Serveur Privé</label>
          <input
            className="lochor-input"
            placeholder="https://votre-serveur-lochor.net"
            value={serverUrl}
            onChange={(e) => setServerUrl(e.target.value)}
          />
        </div>

        <div className="lochor-field">
          <label className="lochor-field-label">Jeton d'accès (API Key / Token)</label>
          <input
            type="password"
            className="lochor-input"
            placeholder="loch_sec_..."
            value={token}
            onChange={(e) => setToken(e.target.value)}
          />
        </div>

        <div className="lochor-field-actions" style={{ marginTop: "20px", display: "flex", gap: "12px" }}>
          <button
            type="button"
            className="lochor-btn-primary"
            onClick={() => setIsConnected(true)}
          >
            Enregistrer et Connecter
          </button>
          {isConnected && (
            <button
              type="button"
              className="lochor-btn-ghost"
              style={{ color: "var(--danger)" }}
              onClick={() => setIsConnected(false)}
            >
              Se déconnecter
            </button>
          )}
        </div>

        <div className="lochor-account-status" style={{ marginTop: "16px" }}>
          <span
            className={`lochor-health-dot ${isConnected ? "lochor-health-ok" : "lochor-health-off"}`}
          />
          {isConnected
            ? `Connecté à ${serverUrl} en tant que ${username}`
            : "Non connecté (Mode autonome local actif)"}
        </div>
      </div>
    </section>
  );
}
