import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type MobileStatus, type PhoneUserSummary, type UserProfile, api } from "../lib/core";
import {
  getNotificationPermission,
  isPushEnabled,
  requestNotificationPermission,
  sendNotification,
  setPushEnabled,
} from "../lib/notifications";
import { ExtensionSettings } from "./ExtensionSettings";
import { Screen } from "./Screen";
import { VersionSection } from "./VersionSection";

type Props = {
  /** Absent tant que personne n'est connecté : l'écran reste utilisable. */
  status: MobileStatus | null;
  onBack: () => void;
  onSignedOut: (s: MobileStatus) => void;
  onMemory: () => void;
};

export function Settings({ status, onBack, onSignedOut, onMemory }: Props) {
  // ── Utilisateur actif & serveur ──
  const [profile, setProfile] = useState<UserProfile | null>(null);
  const [serverUsers, setServerUsers] = useState<PhoneUserSummary[]>([]);
  const [loadingUsers, setLoadingUsers] = useState(false);
  const [userError, setUserError] = useState<string | null>(null);
  const [userNotice, setUserNotice] = useState<string | null>(null);

  // ── Modale Changement de mot de passe ──
  const [showPasswordModal, setShowPasswordModal] = useState(false);
  const [currentPass, setCurrentPass] = useState("");
  const [newPass, setNewPass] = useState("");
  const [confirmPass, setConfirmPass] = useState("");
  const [passError, setPassError] = useState<string | null>(null);
  const [passBusy, setPassBusy] = useState(false);

  // ── Modale Ajout d'utilisateur ──
  const [showAddUserModal, setShowAddUserModal] = useState(false);
  const [newUsername, setNewUsername] = useState("");
  const [newUserPass, setNewUserPass] = useState("");
  const [newUserAdmin, setNewUserAdmin] = useState(false);
  const [addUserError, setAddUserError] = useState<string | null>(null);
  const [addUserBusy, setAddUserBusy] = useState(false);

  // ── Notifications ──
  const [pushActive, setPushActive] = useState(isPushEnabled());
  const [notifPermission, setNotifPermission] = useState(getNotificationPermission());

  const loadUserData = useCallback(async () => {
    if (!status?.signed_in) return;
    try {
      const u = await api.currentUser();
      setProfile(u);
      if (u.role === "admin") {
        setLoadingUsers(true);
        const list = await api.listServerUsers().catch(() => []);
        setServerUsers(list);
      }
    } catch {
      // mode silencieux
    } finally {
      setLoadingUsers(false);
    }
  }, [status?.signed_in]);

  useEffect(() => {
    void loadUserData();
  }, [loadUserData]);

  async function handleTogglePush() {
    if (!pushActive) {
      const ok = await requestNotificationPermission();
      setNotifPermission(getNotificationPermission());
      setPushActive(ok);
      if (ok) {
        sendNotification("Notifications activées", {
          body: "Vous recevrez les alertes et les réponses du serveur.",
        });
      }
    } else {
      setPushEnabled(false);
      setPushActive(false);
    }
  }

  function handleTestPush() {
    sendNotification("Test Locaryn", {
      body: "Notification reçue avec succès sur votre téléphone !",
    });
  }

  async function handleChangePassword(e: React.FormEvent) {
    e.preventDefault();
    if (newPass !== confirmPass) {
      setPassError("Les mots de passe ne correspondent pas.");
      return;
    }
    setPassBusy(true);
    setPassError(null);
    try {
      await api.changePassword(currentPass, newPass);
      setShowPasswordModal(false);
      setCurrentPass("");
      setNewPass("");
      setConfirmPass("");
      setUserNotice("Mot de passe modifié avec succès.");
      window.setTimeout(() => setUserNotice(null), 3000);
    } catch (err) {
      setPassError(String(err));
    } finally {
      setPassBusy(false);
    }
  }

  async function handleCreateUser(e: React.FormEvent) {
    e.preventDefault();
    const uname = newUsername.trim();
    if (!uname || !newUserPass) return;
    setAddUserBusy(true);
    setAddUserError(null);
    try {
      await api.createServerUser(uname, newUserPass, newUserAdmin);
      setShowAddUserModal(false);
      setNewUsername("");
      setNewUserPass("");
      setNewUserAdmin(false);
      setUserNotice(`Utilisateur « ${uname} » créé.`);
      window.setTimeout(() => setUserNotice(null), 3000);
      await loadUserData();
    } catch (err) {
      setAddUserError(String(err));
    } finally {
      setAddUserBusy(false);
    }
  }

  async function handleDeleteUser(user: PhoneUserSummary) {
    if (!window.confirm(`Supprimer le compte « ${user.username} » définitivement ?`)) return;
    try {
      await api.deleteServerUser(user.id);
      setUserNotice(`Utilisateur « ${user.username} » supprimé.`);
      window.setTimeout(() => setUserNotice(null), 3000);
      await loadUserData();
    } catch (err) {
      setUserError(String(err));
    }
  }

  return (
    <Screen title="Réglages" onBack={onBack}>
      {userNotice && (
        <div className="lo-toast">
          <p className="lo-notice">{userNotice}</p>
        </div>
      )}

      <VersionSection />

      {/* ── Compte & Profil Utilisateur ── */}
      {status?.signed_in && (
        <section className="lo-section">
          <h2 className="lo-section-title">Compte Utilisateur</h2>
          <div className="lo-card" style={{ flexDirection: "column", alignItems: "stretch" }}>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
              <div className="lo-card-text">
                <span className="lo-card-title">{profile?.username ?? "Utilisateur"}</span>
                <span className="lo-hint">
                  Rôle : {profile?.role === "admin" ? "Administrateur" : "Membre"}
                </span>
              </div>
              <button
                type="button"
                className="lo-btn-small"
                onClick={() => setShowPasswordModal(true)}
              >
                Changer le mot de passe
              </button>
            </div>
          </div>

          {/* Gestion des comptes par l'administrateur */}
          {profile?.role === "admin" && (
            <div style={{ marginTop: "var(--space-3)" }}>
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  marginBottom: 6,
                }}
              >
                <span style={{ fontSize: 13, fontWeight: 700, color: "var(--text)" }}>
                  Utilisateurs du serveur ({serverUsers.length})
                </span>
                <button
                  type="button"
                  className="lo-btn-small lo-btn-small-on"
                  onClick={() => setShowAddUserModal(true)}
                >
                  + Ajouter
                </button>
              </div>

              {userError && <p className="lo-error">{userError}</p>}
              {loadingUsers && <p className="lo-sub">Chargement des comptes…</p>}

              <ul className="lo-cards">
                {serverUsers.map((u) => (
                  <li key={u.id} className="lo-card" style={{ padding: "8px 12px" }}>
                    <div className="lo-card-text">
                      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                        <span style={{ fontWeight: 600, color: "var(--text)" }}>{u.username}</span>
                        <span
                          style={{
                            fontSize: 10,
                            padding: "1px 5px",
                            borderRadius: 4,
                            background:
                              u.role === "admin"
                                ? "rgba(var(--accent-rgb), 0.2)"
                                : "rgba(255, 255, 255, 0.08)",
                            color: u.role === "admin" ? "var(--accent)" : "var(--text-dim)",
                          }}
                        >
                          {u.role === "admin" ? "Admin" : "Membre"}
                        </span>
                      </div>
                    </div>
                    {u.username !== profile?.username && (
                      <button
                        type="button"
                        className="lo-btn-small"
                        style={{ color: "var(--danger)" }}
                        onClick={() => void handleDeleteUser(u)}
                      >
                        Supprimer
                      </button>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </section>
      )}

      {/* ── Notifications Push ── */}
      <section className="lo-section">
        <h2 className="lo-section-title">Notifications & Alertes</h2>
        <div className="lo-card" style={{ flexDirection: "column", alignItems: "stretch" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <div className="lo-card-text">
              <span className="lo-card-title">Notifications push</span>
              <span className="lo-hint">
                Réponses en arrière-plan, générations prêtes et demandes d'autorisation
              </span>
            </div>
            <button
              type="button"
              className={`lo-toggle ${pushActive ? "lo-toggle-on" : ""}`}
              onClick={() => void handleTogglePush()}
              aria-pressed={pushActive}
            >
              {pushActive ? "Activé" : "Désactivé"}
            </button>
          </div>
          {pushActive && (
            <div style={{ marginTop: 8, display: "flex", justifyContent: "flex-end" }}>
              <button type="button" className="lo-btn-small" onClick={handleTestPush}>
                Envoyer un test
              </button>
            </div>
          )}
        </div>
      </section>

      {/* ── Réglages des extensions ── */}
      <ExtensionSettings />

      {/* ── Serveur & Déconnexion ── */}
      <section className="lo-section">
        <h2 className="lo-section-title">Serveur Locaryn</h2>
        <p className="lo-hint">
          {status?.server_name ?? "Aucun serveur enregistré"}
          {status?.travelling ? " — joint depuis l'extérieur (mode voyage)" : ""}
        </p>
        {status?.signed_in && (
          <button
            type="button"
            className="lo-btn-ghost"
            style={{ marginTop: 8 }}
            onClick={() => void api.signOut().then(onSignedOut)}
          >
            Se déconnecter de ce serveur
          </button>
        )}
      </section>

      {/* ── Personnalisation / Mémoire ── */}
      {status?.signed_in && (
        <section className="lo-section">
          <h2 className="lo-section-title">Personnalisation</h2>
          <button type="button" className="lo-row" onClick={onMemory}>
            <span className="lo-row-icon">
              <Icon name="memory" />
            </span>
            <span className="lo-row-text">
              <span className="lo-row-label">Mémoire</span>
              <span className="lo-hint">Ce que le serveur retient de vous</span>
            </span>
            <span className="lo-row-go">
              <Icon name="chevron" size={16} />
            </span>
          </button>
        </section>
      )}

      {/* ── Modale Changement de mot de passe ── */}
      {showPasswordModal && (
        <div className="lo-modal-backdrop" onClick={() => setShowPasswordModal(false)}>
          <div className="lo-modal" onClick={(e) => e.stopPropagation()}>
            <div className="lo-modal-header">
              <span className="lo-modal-title">Changer de mot de passe</span>
              <button
                type="button"
                className="lo-btn-ghost"
                style={{ width: "auto", minHeight: "auto", padding: "4px 8px", border: "none" }}
                onClick={() => setShowPasswordModal(false)}
              >
                ✕
              </button>
            </div>
            <form onSubmit={handleChangePassword}>
              <div className="lo-modal-body">
                {passError && <p className="lo-error">{passError}</p>}
                <div>
                  <label className="lo-label">Mot de passe actuel</label>
                  <input
                    type="password"
                    className="lo-input"
                    value={currentPass}
                    onChange={(e) => setCurrentPass(e.target.value)}
                    autoFocus
                    required
                  />
                </div>
                <div>
                  <label className="lo-label">Nouveau mot de passe</label>
                  <input
                    type="password"
                    className="lo-input"
                    value={newPass}
                    onChange={(e) => setNewPass(e.target.value)}
                    required
                  />
                </div>
                <div>
                  <label className="lo-label">Confirmer le nouveau mot de passe</label>
                  <input
                    type="password"
                    className="lo-input"
                    value={confirmPass}
                    onChange={(e) => setConfirmPass(e.target.value)}
                    required
                  />
                </div>
              </div>
              <div className="lo-modal-footer">
                <button type="submit" className="lo-btn" disabled={passBusy}>
                  {passBusy ? "Modification…" : "Mettre à jour le mot de passe"}
                </button>
                <button
                  type="button"
                  className="lo-btn-ghost"
                  onClick={() => setShowPasswordModal(false)}
                >
                  Annuler
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* ── Modale Ajout d'utilisateur (Admin) ── */}
      {showAddUserModal && (
        <div className="lo-modal-backdrop" onClick={() => setShowAddUserModal(false)}>
          <div className="lo-modal" onClick={(e) => e.stopPropagation()}>
            <div className="lo-modal-header">
              <span className="lo-modal-title">Créer un compte utilisateur</span>
              <button
                type="button"
                className="lo-btn-ghost"
                style={{ width: "auto", minHeight: "auto", padding: "4px 8px", border: "none" }}
                onClick={() => setShowAddUserModal(false)}
              >
                ✕
              </button>
            </div>
            <form onSubmit={handleCreateUser}>
              <div className="lo-modal-body">
                {addUserError && <p className="lo-error">{addUserError}</p>}
                <div>
                  <label className="lo-label">Identifiant / Nom d'utilisateur</label>
                  <input
                    type="text"
                    className="lo-input"
                    placeholder="ex: alice, lucas"
                    value={newUsername}
                    onChange={(e) => setNewUsername(e.target.value)}
                    autoFocus
                    required
                  />
                </div>
                <div>
                  <label className="lo-label">Mot de passe initial</label>
                  <input
                    type="password"
                    className="lo-input"
                    placeholder="Au moins 8 caractères"
                    value={newUserPass}
                    onChange={(e) => setNewUserPass(e.target.value)}
                    required
                  />
                </div>
                <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 4 }}>
                  <input
                    type="checkbox"
                    id="adminCheck"
                    checked={newUserAdmin}
                    onChange={(e) => setNewUserAdmin(e.target.checked)}
                  />
                  <label htmlFor="adminCheck" style={{ fontSize: 13, color: "var(--text)" }}>
                    Accorder les droits d'administrateur
                  </label>
                </div>
              </div>
              <div className="lo-modal-footer">
                <button type="submit" className="lo-btn" disabled={addUserBusy}>
                  {addUserBusy ? "Création…" : "Créer le compte"}
                </button>
                <button
                  type="button"
                  className="lo-btn-ghost"
                  onClick={() => setShowAddUserModal(false)}
                >
                  Annuler
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </Screen>
  );
}
