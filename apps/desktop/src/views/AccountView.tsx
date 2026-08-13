import { useState } from "react";

export function AccountView() {
  const [serverUrl, setServerUrl] = useState("https://private.locaryn.internal");
  const [token, setToken] = useState("");
  // Vide par défaut : un nom en dur devient le nom de tout le monde dans les
  // captures d'écran, la doc et les rapports de bug.
  const [username, setUsername] = useState("");
  const [isConnected, setIsConnected] = useState(true);

  return (
    <section className="locaryn-view-container">
      <div className="locaryn-view-header">
        <h2>Gestion du Compte & Serveur Privé</h2>
        <p className="locaryn-view-desc">
          Connectez votre instance desktop à un serveur privé distant pour la synchronisation, les
          modèles hébergés et l'exécution distante.
        </p>
      </div>

      <div className="locaryn-card" style={{ maxWidth: "600px" }}>
        <h3>Profil Utilisateur</h3>
        <div className="locaryn-field">
          <label className="locaryn-field-label" htmlFor="account-username">
            Nom d'utilisateur / Alias
          </label>
          <input
            id="account-username"
            className="locaryn-input"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
          />
        </div>

        <h3 style={{ marginTop: "24px" }}>Connexion Serveur Privé (Gateway)</h3>
        <div className="locaryn-field">
          <label className="locaryn-field-label" htmlFor="account-server-url">
            URL du Serveur Privé
          </label>
          <input
            id="account-server-url"
            className="locaryn-input"
            placeholder="https://votre-serveur-locaryn.net"
            value={serverUrl}
            onChange={(e) => setServerUrl(e.target.value)}
          />
        </div>

        <div className="locaryn-field">
          <label className="locaryn-field-label" htmlFor="account-token">
            Jeton d'accès (API Key / Token)
          </label>
          <input
            id="account-token"
            type="password"
            className="locaryn-input"
            placeholder="loch_sec_..."
            value={token}
            onChange={(e) => setToken(e.target.value)}
          />
        </div>

        <div
          className="locaryn-field-actions"
          style={{ marginTop: "20px", display: "flex", gap: "12px" }}
        >
          <button
            type="button"
            className="locaryn-btn-primary"
            onClick={() => setIsConnected(true)}
          >
            Enregistrer et Connecter
          </button>
          {isConnected && (
            <button
              type="button"
              className="locaryn-btn-ghost"
              style={{ color: "var(--danger)" }}
              onClick={() => setIsConnected(false)}
            >
              Se déconnecter
            </button>
          )}
        </div>

        <div className="locaryn-account-status" style={{ marginTop: "16px" }}>
          <span
            className={`locaryn-health-dot ${isConnected ? "locaryn-health-ok" : "locaryn-health-off"}`}
          />
          {isConnected
            ? `Connecté à ${serverUrl} en tant que ${username}`
            : "Non connecté (Mode autonome local actif)"}
        </div>
      </div>
    </section>
  );
}
