import { useCallback, useEffect, useState } from "react";
import { SEGMENT_LOCAL, useSegmentsAppairage } from "../hooks/useSegmentsAppairage";
import { type ServerStatus, type ServerUserSummary, core } from "../lib/core";
import { confirmer } from "../lib/dialog";
import { choisirSegment, useModeAcces } from "../lib/modeAcces";
import { ecrirePressePapiers } from "../lib/pressePapiers";
import { UserLoginQr } from "./UserLoginQr";

/** Le serveur arrêté : pas un segment d'appairage, l'état du service. */
const OFF = "off";

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
  /** Le compte dont on affiche le QR de connexion. */
  const [qrCompte, setQrCompte] = useState<{ id: string; username: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const segments = useSegmentsAppairage();
  const { segment, transport } = useModeAcces();

  // Modal d'ajout d'identifiant / configuration initiale
  const [showAddUser, setShowAddUser] = useState(false);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [passwordConfirm, setPasswordConfirm] = useState("");
  const [isAdmin, setIsAdmin] = useState(true);
  const [userError, setUserError] = useState<string | null>(null);
  const [userBusy, setUserBusy] = useState(false);
  const [autoStartAfterUser, setAutoStartAfterUser] = useState(false);
  // Le premier compte est forcément administrateur (un serveur en a besoin) ;
  // les suivants sont membres, sauf choix contraire.
  const aucunAdmin = !users.some((u) => u.role === "admin");
  useEffect(() => {
    if (showAddUser) setIsAdmin(aucunAdmin);
  }, [showAddUser, aucunAdmin]);

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
      const newStatus = await core.createServerUser(trimmedUser, password, isAdmin || aucunAdmin);
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
    if (!(await confirmer(`Supprimer l'identifiant « ${uName} » ?`))) return;
    try {
      const s = await core.deleteServerUser(userId);
      setStatus(s);
      await refreshUsers();
    } catch (e) {
      setError(String(e));
    }
  }

  const blocked = Boolean(status?.blocker);
  const bloqueSansCompte = blocked && !status?.running && (status?.accounts ?? 0) > 0;
  const choisi = segments.find((m) => m.id === segment) ?? SEGMENT_LOCAL;
  const actif = status?.running ? choisi.id : OFF;
  const quiPeutSeConnecter = !status?.running
    ? "Personne : le serveur est arrêté, Locaryn reste sur cette machine."
    : choisi.description;

  // Le QR d'un compte reprend l'adresse du code d'appairage : tant que
  // celui-ci n'existe pas (serveur arrêté, adresse publique ou tunnel encore
  // en recherche), il n'y a rien de valable à mettre dedans.
  const qrPret = Boolean(status?.running) && (segment === "local" || transport !== null);

  async function choisirMode(id: string) {
    if (id === OFF) {
      await toggle(false);
      return;
    }
    choisirSegment(id);
    if (!status?.running) await toggle(true);
  }

  return (
    <div className="locaryn-field">
      <div className="locaryn-field-label">Service & Mode Serveur</div>
      <p className="locaryn-field-hint">Ouvre Locaryn à vos autres appareils.</p>

      {/* Un seul choix : qui peut joindre ce serveur. Chaque palier garde le
          précédent — l'accès par Internet ou par un tunnel sert aussi le
          réseau local. */}
      <div
        className="locaryn-segmented locaryn-srv-modes"
        role="radiogroup"
        aria-label="Mode d'accès"
      >
        {[{ id: OFF, label: "Désactivé" }, ...segments].map((m) => {
          const on = actif === m.id;
          return (
            <button
              key={m.id}
              type="button"
              role="radio"
              aria-checked={on}
              className={`locaryn-segment${on ? " locaryn-segment-on" : ""}`}
              disabled={busy || (m.id !== OFF && bloqueSansCompte)}
              onClick={() => void choisirMode(m.id)}
            >
              {m.label}
            </button>
          );
        })}
      </div>
      <div className="locaryn-srv-row" aria-live="polite">
        <span
          className={`locaryn-srv-dot${status?.running ? " is-on" : busy ? " is-busy" : ""}`}
          aria-hidden
        />
        <span className="locaryn-srv-who">{busy ? "Un instant…" : quiPeutSeConnecter}</span>
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
            Pour se connecter depuis un autre appareil.
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
                  checked={isAdmin || aucunAdmin}
                  disabled={aucunAdmin}
                  onChange={(e) => setIsAdmin(e.target.checked)}
                />
                <span>Droits administrateur</span>
              </label>
            </div>
            <p className="locaryn-field-hint" style={{ margin: 0 }}>
              {aucunAdmin
                ? "Le premier compte est administrateur."
                : "Un administrateur gère les comptes et les clés d'API."}
            </p>

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
          <div className="locaryn-field-label">Identifiants ({users.length})</div>
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
                <div style={{ display: "flex", gap: 6 }}>
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    style={{ fontSize: 11, padding: "2px 8px" }}
                    disabled={u.disabled || !qrPret}
                    title={
                      qrPret
                        ? "Ouvre la session de ce compte sur un téléphone, sans mot de passe"
                        : "Disponible dès que le code QR du mode choisi est affiché"
                    }
                    onClick={() => setQrCompte({ id: u.id, username: u.username })}
                  >
                    QR de connexion
                  </button>
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    style={{ fontSize: 11, padding: "2px 8px", color: "var(--danger)" }}
                    onClick={() => void handleDeleteUser(u.id, u.username)}
                  >
                    Supprimer
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {qrCompte && (
        <UserLoginQr
          userId={qrCompte.id}
          username={qrCompte.username}
          distant={choisi.id === SEGMENT_LOCAL.id ? null : choisi.label}
          onClose={() => setQrCompte(null)}
        />
      )}

      {status?.running && (
        <>
          {/* Une seule adresse, celle du mode choisi. Hors du réseau local, elle
              n'est écrite que dans le QR : une IP publique ne s'affiche pas. */}
          {choisi.id === SEGMENT_LOCAL.id && (
            <div className="locaryn-kv-list" style={{ marginTop: 20 }}>
              <div className="locaryn-kv">
                <span className="locaryn-kv-key">Adresse de connexion</span>
                <span className="locaryn-kv-val locaryn-kv-mono">{status.url}</span>
              </div>
            </div>
          )}

          {choisi.id === SEGMENT_LOCAL.id && (
            <div className="locaryn-field-actions" style={{ marginTop: 10 }}>
              <button
                type="button"
                className="locaryn-btn-ghost"
                onClick={async () => {
                  try {
                    await ecrirePressePapiers(status.url);
                    setCopied(true);
                    window.setTimeout(() => setCopied(false), 1500);
                  } catch (e) {
                    setError(`Presse-papiers indisponible : ${String(e)}`);
                  }
                }}
              >
                {copied ? "Adresse copiée" : "Copier l'adresse"}
              </button>
            </div>
          )}
        </>
      )}

      {error && (
        <div className="locaryn-vp-error" style={{ marginTop: 12 }}>
          {error}
        </div>
      )}

      {/* Le technique, pour qui le cherche : rien ici ne sert à agir. */}
      <details className="locaryn-voir-plus">
        <summary>Voir plus</summary>
        <div className="locaryn-kv-list">
          <div className="locaryn-kv">
            <span className="locaryn-kv-key">Port</span>
            <span className="locaryn-kv-val locaryn-kv-mono">{status?.port ?? "—"}</span>
          </div>
          {status?.running && (
            <div className="locaryn-kv">
              <span className="locaryn-kv-key">Écoute</span>
              <span className="locaryn-kv-val locaryn-kv-mono">{status.bind}</span>
            </div>
          )}
        </div>
        {status?.fingerprint && (
          <>
            <p className="locaryn-field-hint">
              Empreinte du certificat : le téléphone la vérifie seul, via le QR.
            </p>
            <div className="locaryn-srv-fingerprint">{status.fingerprint}</div>
          </>
        )}
        <p className="locaryn-field-hint">
          Fermer la fenêtre garde le service actif ; « Quitter Locaryn » dans la zone de
          notification arrête les deux.
        </p>
      </details>
    </div>
  );
}
