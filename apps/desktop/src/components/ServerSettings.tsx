import { useCallback, useEffect, useState } from "react";
import { type ServerStatus, type ServerUserSummary, core } from "../lib/core";

/**
 * Share this machine's models with other people.
 *
 * The application does not serve HTTP itself — it starts the Locaryn service,
 * which already carries the accounts, the tokens and the encryption. What the
 * switch really does is expose that service on the network, and everything the
 * service guarantees comes with it: authentication becomes mandatory, traffic
 * is encrypted, and it refuses to run at all with no account.
 */
export function ServerSettings() {
  const [status, setStatus] = useState<ServerStatus | null>(null);
  const [users, setUsers] = useState<ServerUserSummary[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  // Modal d'ajout d'identifiant / configuration initiale
  const [showAddUser, setShowAddUser] = useState(false);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [passwordConfirm, setPasswordConfirm] = useState("");
  const [isAdmin, setIsAdmin] = useState(true);
  const [userError, setUserError] = useState<string | null>(null);
  const [userBusy, setUserBusy] = useState(false);
  const [autoStartAfterUser, setAutoStartAfterUser] = useState(false);

  const refreshUsers = useCallback(async () => {
    try {
      const list = await core.listServerUsers();
      setUsers(list);
    } catch {
      // Pas bloquant si la base n'est pas encore initialisée
    }
  }, []);

  const refresh = useCallback(async () => {
    try {
      const s = await core.serverStatus();
      setStatus(s);
      await refreshUsers();
    } catch (e) {
      setError(String(e));
    }
  }, [refreshUsers]);

  useEffect(() => {
    void refresh();
    const t = window.setInterval(() => void refresh(), 5000);
    return () => window.clearInterval(t);
  }, [refresh]);

  async function toggle(enabled: boolean) {
    if (enabled && status && status.accounts === 0) {
      // Aucun compte n'existe encore : demander à l'utilisateur de définir son identifiant
      setAutoStartAfterUser(true);
      setShowAddUser(true);
      return;
    }

    setBusy(true);
    setError(null);
    try {
      setStatus(await core.setServerMode(enabled));
      await refreshUsers();
    } catch (e) {
      setError(String(e));
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  async function handleCreateUser(e: React.FormEvent) {
    e.preventDefault();
    setUserError(null);

    const trimmedUser = username.trim();
    if (!trimmedUser) {
      setUserError("Veuillez saisir un nom d'utilisateur.");
      return;
    }
    if (password.length < 8) {
      setUserError("Le mot de passe doit comporter au moins 8 caractères.");
      return;
    }
    if (password !== passwordConfirm) {
      setUserError("Les mots de passe ne correspondent pas.");
      return;
    }

    setUserBusy(true);
    try {
      const newStatus = await core.createServerUser(trimmedUser, password, isAdmin);
      setStatus(newStatus);
      await refreshUsers();
      setShowAddUser(false);
      setUsername("");
      setPassword("");
      setPasswordConfirm("");

      if (autoStartAfterUser || !newStatus.running) {
        setBusy(true);
        try {
          setStatus(await core.setServerMode(true));
        } catch (startErr) {
          setError(String(startErr));
        } finally {
          setBusy(false);
          setAutoStartAfterUser(false);
        }
      }
    } catch (err) {
      setUserError(String(err));
    } finally {
      setUserBusy(false);
    }
  }

  async function handleDeleteUser(userId: string, uName: string) {
    if (!window.confirm(`Supprimer l'identifiant « ${uName} » ?`)) return;
    try {
      const s = await core.deleteServerUser(userId);
      setStatus(s);
      await refreshUsers();
    } catch (e) {
      setError(String(e));
    }
  }

  const blocked = Boolean(status?.blocker);

  return (
    <div className="locaryn-field">
      <div className="locaryn-field-label">Service & Mode Serveur</div>
      <p className="locaryn-field-hint">
        Démarre le serveur qui expose les fonctions de Locaryn — modèles, conversations et outils —
        aux appareils autorisés sur votre réseau (application mobile, interface web).
      </p>
      <p className="locaryn-field-hint">
        Fermer la fenêtre la place dans la zone de notification et conserve le service actif.
        Utilisez <strong>Quitter Locaryn</strong> depuis l'icône du tray pour arrêter les deux.
      </p>

      <div className="locaryn-srv-row">
        <label className="locaryn-srv-toggle">
          <input
            type="checkbox"
            checked={Boolean(status?.running)}
            disabled={busy || (blocked && !status?.running && (status?.accounts ?? 0) > 0)}
            onChange={(e) => toggle(e.target.checked)}
          />
          <span>{busy ? "…" : status?.running ? "Serveur actif" : "Serveur arrêté"}</span>
        </label>
        {status?.running && <span className="locaryn-srv-live">en écoute</span>}
      </div>

      {status?.blocker && !status.running && (status.accounts ?? 0) === 0 && (
        <div className="locaryn-vp-warn" style={{ marginTop: 12 }}>
          <p style={{ margin: "0 0 8px 0" }}>{status.blocker}</p>
          <button
            type="button"
            className="locaryn-btn"
            style={{ width: "auto", padding: "6px 14px", fontSize: 13 }}
            onClick={() => {
              setAutoStartAfterUser(true);
              setShowAddUser(true);
            }}
          >
            + Définir un identifiant administrateur
          </button>
        </div>
      )}

      {status?.blocker && !status.running && status.accounts > 0 && (
        <p className="locaryn-vp-warn">{status.blocker}</p>
      )}

      {/* ── Modal de création d'identifiant ── */}
      {showAddUser && (
        <div
          style={{
            marginTop: 16,
            padding: 16,
            background: "var(--surface)",
            border: "1px solid var(--accent)",
            borderRadius: "var(--radius, 8px)",
          }}
        >
          <div style={{ fontWeight: 700, fontSize: 15, marginBottom: 6, color: "var(--text)" }}>
            Définir un identifiant de connexion
          </div>
          <p className="locaryn-field-hint" style={{ marginBottom: 12 }}>
            Cet identifiant (nom d'utilisateur et mot de passe) sera requis pour vous connecter
            depuis l'application mobile ou l'interface web.
          </p>

          <form
            onSubmit={handleCreateUser}
            style={{ display: "flex", flexDirection: "column", gap: 10 }}
          >
            <div>
              <label className="locaryn-field-label" style={{ fontSize: 12, marginBottom: 4 }}>
                Nom d'utilisateur (Identifiant)
              </label>
              <input
                type="text"
                className="locaryn-input"
                placeholder="ex: marie, admin..."
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                autoFocus
                required
              />
            </div>

            <div>
              <label className="locaryn-field-label" style={{ fontSize: 12, marginBottom: 4 }}>
                Mot de passe (8 caractères minimum)
              </label>
              <input
                type="password"
                className="locaryn-input"
                placeholder="••••••••"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                required
              />
            </div>

            <div>
              <label className="locaryn-field-label" style={{ fontSize: 12, marginBottom: 4 }}>
                Confirmer le mot de passe
              </label>
              <input
                type="password"
                className="locaryn-input"
                placeholder="••••••••"
                value={passwordConfirm}
                onChange={(e) => setPasswordConfirm(e.target.value)}
                required
              />
            </div>

            <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 4 }}>
              <label
                style={{
                  fontSize: 13,
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  cursor: "pointer",
                }}
              >
                <input
                  type="checkbox"
                  checked={isAdmin}
                  onChange={(e) => setIsAdmin(e.target.checked)}
                />
                <span>Droits Administrateur</span>
              </label>
            </div>

            {userError && <div className="locaryn-vp-error">{userError}</div>}

            <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
              <button type="submit" className="locaryn-btn" disabled={userBusy} style={{ flex: 1 }}>
                {userBusy
                  ? "Création…"
                  : autoStartAfterUser
                    ? "Créer l'identifiant et démarrer le serveur"
                    : "Créer l'identifiant"}
              </button>
              <button
                type="button"
                className="locaryn-btn-ghost"
                disabled={userBusy}
                onClick={() => {
                  setShowAddUser(false);
                  setAutoStartAfterUser(false);
                }}
              >
                Annuler
              </button>
            </div>
          </form>
        </div>
      )}

      {/* ── Liste des identifiants existants ── */}
      <div style={{ marginTop: 20 }}>
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            marginBottom: 8,
          }}
        >
          <div className="locaryn-field-label">Identifiants configurés ({users.length})</div>
          {!showAddUser && (
            <button
              type="button"
              className="locaryn-btn-ghost"
              style={{ fontSize: 12, padding: "4px 10px" }}
              onClick={() => {
                setUserError(null);
                setAutoStartAfterUser(false);
                setShowAddUser(true);
              }}
            >
              + Nouvel identifiant
            </button>
          )}
        </div>

        {users.length === 0 ? (
          <p className="locaryn-field-hint">Aucun compte utilisateur configuré.</p>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {users.map((u) => (
              <div
                key={u.id}
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  padding: "8px 12px",
                  background: "var(--surface)",
                  borderRadius: "var(--radius-sm, 6px)",
                  border: "1px solid var(--border)",
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <span style={{ fontWeight: 600, color: "var(--text)" }}>{u.username}</span>
                  <span
                    style={{
                      fontSize: 11,
                      padding: "2px 6px",
                      borderRadius: 10,
                      background:
                        u.role === "admin"
                          ? "rgba(var(--accent-rgb), 0.2)"
                          : "rgba(255, 255, 255, 0.1)",
                      color: u.role === "admin" ? "var(--accent)" : "var(--text-muted)",
                      fontWeight: 600,
                    }}
                  >
                    {u.role === "admin" ? "Administrateur" : "Membre"}
                  </span>
                </div>
                <button
                  type="button"
                  className="locaryn-btn-ghost"
                  style={{ fontSize: 11, padding: "2px 8px", color: "var(--danger)" }}
                  onClick={() => void handleDeleteUser(u.id, u.username)}
                >
                  Supprimer
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {status?.running && (
        <>
          <div className="locaryn-kv-list" style={{ marginTop: 20 }}>
            <div className="locaryn-kv">
              <span className="locaryn-kv-key">Adresse de connexion (Mobile / Web)</span>
              <span className="locaryn-kv-val locaryn-kv-mono">{status.url}</span>
            </div>
            <div className="locaryn-kv">
              <span className="locaryn-kv-key">Comptes actifs</span>
              <span className="locaryn-kv-val locaryn-kv-mono">{status.accounts}</span>
            </div>
          </div>

          <div className="locaryn-field-actions" style={{ marginTop: 10 }}>
            <button
              type="button"
              className="locaryn-btn-ghost"
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(status.url);
                  setCopied(true);
                  window.setTimeout(() => setCopied(false), 1500);
                } catch {
                  /* clipboard unavailable — the address is visible above */
                }
              }}
            >
              {copied ? "Adresse copiée" : "Copier l'adresse"}
            </button>
          </div>

          {status.fingerprint && (
            <>
              <div className="locaryn-field-label" style={{ marginTop: 20 }}>
                Empreinte du certificat
              </div>
              <p className="locaryn-field-hint">
                Le certificat est généré par cette machine, donc les postes clients afficheront un
                avertissement au premier contact. C'est attendu : cette empreinte est ce qui permet
                de vérifier qu'ils parlent bien à<em> cet</em> ordinateur et pas à un autre.
              </p>
              <div className="locaryn-srv-fingerprint">{status.fingerprint}</div>
            </>
          )}

          <p className="locaryn-field-hint" style={{ marginTop: 16 }}>
            Pour éviter à vos collègues toute configuration, vous pouvez générer un code d'appairage
            ou utiliser le fichier de provisionnement.
          </p>
        </>
      )}

      {error && (
        <div className="locaryn-vp-error" style={{ marginTop: 12 }}>
          {error}
        </div>
      )}
    </div>
  );
}
