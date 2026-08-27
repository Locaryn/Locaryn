import { Icon, type IconName, LoSwitch } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import {
  type Conversation,
  type MobileStatus,
  type PhoneProject,
  type PhoneUserSummary,
  type UserProfile,
  api,
} from "../lib/core";
import { useCoucheRetour } from "../lib/navigation";
import {
  getNotificationPermission,
  isPushEnabled,
  requestNotificationPermission,
  sendNotification,
  setPushEnabled,
} from "../lib/notifications";
import {
  ACCENT_PRESETS,
  type ReglageTheme,
  type ThemeMode,
  appliquerTheme,
  lireTheme,
} from "../lib/theme";
import { ExtensionSettings } from "./ExtensionSettings";
import { Screen } from "./Screen";
import { VersionSection } from "./VersionSection";

type Props = {
  /** Absent tant que personne n'est connecté : l'écran reste utilisable. */
  status: MobileStatus | null;
  onBack: () => void;
  onSignedOut: (s: MobileStatus) => void;
  onMemory: () => void;
  /** Les conversations rangées — un endroit rarement visité, comme sur le bureau. */
  onArchives: () => void;
  /** Ouvrir une conversation depuis l'historique des réglages. */
  onOpenChat: (sessionId: string) => void;
  /** La catégorie à ouvrir dès l'arrivée — le bouton « Mettre à jour » vise À propos. */
  initialSection?: Section | null;
};

/**
 * Les mêmes catégories que « Paramètres Système » sur l'ordinateur, dans le
 * même ordre, avec les mêmes phrases. Le téléphone est un client du serveur :
 * ce qui tourne sur la machine qui héberge Locaryn (moteur, performance,
 * stockage…) s'y règle, et le téléphone le dit clairement au lieu de le
 * cacher — la catégorie existe, le réglage vit sur le bureau.
 */
export type Section =
  | "account"
  | "engine"
  | "performance"
  | "huggingface"
  | "projects"
  | "extensions"
  | "connectors"
  | "appearance"
  | "language"
  | "server"
  | "storage"
  | "about";

const SECTIONS: { id: Section; icon: IconName; label: string; desc: string; connecte?: boolean }[] =
  [
    {
      id: "account",
      icon: "private",
      label: "Compte",
      desc: "Profil local, identité, préférences et mémoire",
      connecte: true,
    },
    {
      id: "engine",
      icon: "settings",
      label: "Moteur IA",
      desc: "Runtime llama.cpp, capacités, adaptateurs LoRA",
    },
    {
      id: "performance",
      icon: "speed",
      label: "Performance",
      desc: "GPU, cache KV, contexte, offload",
    },
    {
      id: "huggingface",
      icon: "marketplace",
      label: "HuggingFace",
      desc: "Token pour les dépôts restreints (modèles gated)",
    },
    {
      id: "projects",
      icon: "project",
      label: "Projets",
      desc: "Autorisations, base de connaissances, archivage",
      connecte: true,
    },
    {
      id: "extensions",
      icon: "extensions",
      label: "Extensions",
      desc: "Extensions Locaryn, plugins compatibles et noyaux",
      connecte: true,
    },
    {
      id: "connectors",
      icon: "server",
      label: "Connecteurs & MCP",
      desc: "Connexions SSH, bases de données et serveurs MCP",
    },
    { id: "appearance", icon: "studio", label: "Apparence", desc: "Couleur d'accentuation, thème" },
    { id: "language", icon: "chat", label: "Langue", desc: "Langue de l'interface" },
    {
      id: "server",
      icon: "server",
      label: "Serveur & fonctions",
      desc: "Service Locaryn, accès local et appairage",
    },
    {
      id: "storage",
      icon: "models",
      label: "Stockage",
      desc: "Emplacement des modèles, espace disque, nettoyage",
    },
    { id: "about", icon: "warning", label: "À propos", desc: "Version, licences, système" },
  ];

/** Les trois réglages de thème, dans l'ordre où ils se lisent. */
const MODES_THEME: { value: ThemeMode; label: string; icon: IconName }[] = [
  { value: "dark", label: "Sombre", icon: "moon" },
  { value: "light", label: "Clair", icon: "sun" },
  { value: "system", label: "Système", icon: "monitor" },
];

export function Settings({
  status,
  onBack,
  onSignedOut,
  onMemory,
  onArchives,
  onOpenChat,
  initialSection = null,
}: Props) {
  const connecte = status?.signed_in ?? false;

  // ── Navigation interne : la liste des catégories, puis la catégorie. Le
  // bouton « Mettre à jour » arrive directement sur À propos. ──
  const [section, setSection] = useState<Section | null>(initialSection);
  const courante = section ? SECTIONS.find((s) => s.id === section) : null;

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
  const [notifError, setNotifError] = useState<string | null>(null);

  // ── Apparence ──
  const [theme, setTheme] = useState<ReglageTheme>(() => lireTheme());

  // Le retour d'Android referme ce qui est ouvert : la catégorie courante,
  // puis les modales — jamais l'application d'un seul coup.
  useCoucheRetour(section !== null, () => setSection(null));
  useCoucheRetour(showPasswordModal, () => setShowPasswordModal(false));
  useCoucheRetour(showAddUserModal, () => setShowAddUserModal(false));

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

  /** Poser un réglage de thème : l'état, le document, et l'appareil. */
  function choisirTheme(reglage: ReglageTheme) {
    setTheme(reglage);
    appliquerTheme(reglage, true);
  }

  async function handleTogglePush() {
    setNotifError(null);
    if (!pushActive) {
      const ok = await requestNotificationPermission();
      setNotifPermission(getNotificationPermission());
      setPushActive(ok);
      if (ok) {
        sendNotification("Notifications activées", {
          body: "Vous recevrez les alertes et les réponses du serveur.",
        });
      } else {
        // Refus ou indisponibilité : le dire, sinon le bouton semble mort.
        setNotifError(
          "Locaryn n'a pas obtenu le droit de notifier. Autorisez les notifications dans les réglages Android : Paramètres → Applications → Locaryn → Notifications.",
        );
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

  const visibles = SECTIONS.filter((s) => !s.connecte || connecte);

  return (
    <Screen
      title={courante ? courante.label : "Paramètres"}
      onBack={courante ? () => setSection(null) : onBack}
    >
      {userNotice && (
        <div className="lo-toast">
          <p className="lo-notice">{userNotice}</p>
        </div>
      )}

      {!courante ? (
        <>
          <p className="lo-hint lo-settings-intro">
            Tous les réglages de Locaryn, rangés comme sur l'ordinateur. Les options propres à une
            conversation restent accessibles depuis l'écran du chat.
          </p>
          <ul className="lo-cards">
            {visibles.map((s) => (
              <li key={s.id}>
                <button type="button" className="lo-row" onClick={() => setSection(s.id)}>
                  <span className="lo-row-icon">
                    <Icon name={s.icon} />
                  </span>
                  <span className="lo-row-text">
                    <span className="lo-row-label">{s.label}</span>
                    <span className="lo-hint">{s.desc}</span>
                  </span>
                  <span className="lo-row-go">
                    <Icon name="chevron" size={16} />
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </>
      ) : (
        <>
          <p className="lo-hint lo-settings-intro">{courante.desc}</p>

          {/* ── Compte : profil, identité, préférences et mémoire ── */}
          {section === "account" && (
            <>
              <section className="lo-section">
                <h2 className="lo-section-title">Profil</h2>
                <div className="lo-card" style={{ flexDirection: "column", alignItems: "stretch" }}>
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                    }}
                  >
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
                              <span style={{ fontWeight: 600, color: "var(--text)" }}>
                                {u.username}
                              </span>
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

              {/* L'historique vit dans le Compte, comme sur le bureau — les
                  conversations récentes sont une affaire de compte, pas une
                  catégorie de réglages. */}
              <ConversationHistory onOpenChat={onOpenChat} />

              <section className="lo-section">
                <h2 className="lo-section-title">Préférences & mémoire</h2>
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
                <button
                  type="button"
                  className="lo-row"
                  style={{ marginTop: "var(--space-2)" }}
                  onClick={onArchives}
                >
                  <span className="lo-row-icon">
                    <Icon name="archive" />
                  </span>
                  <span className="lo-row-text">
                    <span className="lo-row-label">Archives</span>
                    <span className="lo-hint">Conversations rangées</span>
                  </span>
                  <span className="lo-row-go">
                    <Icon name="chevron" size={16} />
                  </span>
                </button>
              </section>
            </>
          )}

          {/* ── Moteur IA : tourne sur l'ordinateur ── */}
          {section === "engine" && (
            <SurOrdinateur
              quoi="Le moteur d'inférence — runtime llama.cpp, capacités, adaptateurs LoRA"
              detail="C'est la machine qui héberge le serveur Locaryn qui fait tourner les modèles. Ses réglages s'y règlent."
            />
          )}

          {/* ── Performance : tourne sur l'ordinateur ── */}
          {section === "performance" && (
            <SurOrdinateur
              quoi="La performance — GPU, cache KV, contexte, offload"
              detail="Ces réglages dépendent du matériel de la machine qui héberge le serveur : ils ne se font que sur place."
            />
          )}

          {/* ── HuggingFace : token du serveur ── */}
          {section === "huggingface" && (
            <SurOrdinateur
              quoi="Le jeton HuggingFace"
              detail="Il déverrouille les dépôts restreints (modèles gated) du côté du serveur qui les télécharge."
            />
          )}

          {/* ── Projets : la liste, et le reste sur l'ordinateur ── */}
          {section === "projects" && <ProjectsSection />}

          {/* ── Extensions : piloteables depuis le téléphone ── */}
          {section === "extensions" && <ExtensionSettings />}

          {/* ── Connecteurs & MCP : sur l'ordinateur ── */}
          {section === "connectors" && (
            <SurOrdinateur
              quoi="Les connecteurs et serveurs MCP — SSH, bases de données"
              detail="Ces connexions partent de la machine qui héberge le serveur, vers d'autres machines. Elles se configurent sur place."
            />
          )}

          {/* ── Apparence : le mode, puis la couleur d'accentuation ── */}
          {section === "appearance" && (
            <section className="lo-section">
              <h2 className="lo-section-title">Thème</h2>
              <p className="lo-hint">
                Sombre par défaut. En clair, l'accent s'assombrit tout seul pour rester lisible.
              </p>
              <div className="lo-segmented" style={{ marginTop: 12 }} role="group">
                {MODES_THEME.map((m) => (
                  <button
                    key={m.value}
                    type="button"
                    className={`lo-segment${theme.mode === m.value ? " lo-segment-on" : ""}`}
                    aria-pressed={theme.mode === m.value}
                    onClick={() => choisirTheme({ ...theme, mode: m.value })}
                  >
                    <Icon name={m.icon} size={15} />
                    {m.label}
                  </button>
                ))}
              </div>

              <h2 className="lo-section-title" style={{ marginTop: 32 }}>
                Couleur d'accentuation
              </h2>
              <p className="lo-hint">
                La teinte unique de l'interface. Sobre et naturelle par défaut — la même palette que
                sur l'ordinateur.
              </p>
              <div className="lo-swatch-grid" style={{ marginTop: 12 }}>
                {ACCENT_PRESETS.map((p) => {
                  const actif = theme.hex.toLowerCase() === p.hex.toLowerCase();
                  return (
                    <button
                      key={p.hex}
                      type="button"
                      className={`lo-swatch${actif ? " lo-swatch-active" : ""}`}
                      style={{ background: p.hex }}
                      title={p.name}
                      aria-label={`Accent ${p.name}`}
                      aria-pressed={actif}
                      onClick={() => choisirTheme({ ...theme, hex: p.hex })}
                    >
                      {actif && (
                        <span className="lo-swatch-check">
                          <Icon name="check" size={14} />
                        </span>
                      )}
                    </button>
                  );
                })}
              </div>
              <div className="lo-custom-color" style={{ marginTop: 16 }}>
                <input
                  type="color"
                  value={theme.hex}
                  onChange={(e) => choisirTheme({ ...theme, hex: e.target.value })}
                  className="lo-color-input"
                  aria-label="Couleur personnalisée"
                />
                <span className="lo-color-value">{theme.hex}</span>
              </div>
              <button
                type="button"
                className="lo-btn-ghost lo-settings-reset"
                style={{ marginTop: 16 }}
                onClick={() => choisirTheme({ mode: "dark", hex: ACCENT_PRESETS[0].hex })}
              >
                Réinitialiser l'apparence
              </button>
            </section>
          )}

          {/* ── Langue : le français, comme partout ── */}
          {section === "language" && (
            <section className="lo-section">
              <h2 className="lo-section-title">Langue de l'interface</h2>
              <div className="lo-card">
                <div className="lo-card-text">
                  <span className="lo-card-title">Français</span>
                  <span className="lo-hint">
                    La langue de l'application sur le téléphone. Les noms de modèles et les termes
                    techniques restent inchangés.
                  </span>
                </div>
              </div>
            </section>
          )}

          {/* ── Serveur & fonctions ── */}
          {section === "server" && (
            <>
              <section className="lo-section">
                <h2 className="lo-section-title">Serveur Locaryn</h2>
                <div className="lo-card">
                  <div className="lo-card-text">
                    <span className="lo-card-title">
                      {status?.server_name ?? "Aucun serveur enregistré"}
                    </span>
                    <span className="lo-hint">
                      {status?.travelling
                        ? "Joint depuis l'extérieur (mode voyage)"
                        : "Service local, accès direct"}
                    </span>
                  </div>
                </div>
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

              <section className="lo-section">
                <h2 className="lo-section-title">Notifications & Alertes</h2>
                <div className="lo-card" style={{ flexDirection: "column", alignItems: "stretch" }}>
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                    }}
                  >
                    <div className="lo-card-text">
                      <span className="lo-card-title">Notifications push</span>
                      <span className="lo-hint">
                        Réponses en arrière-plan, générations prêtes et demandes d'autorisation
                      </span>
                    </div>
                    <LoSwitch
                      checked={pushActive}
                      onChange={() => void handleTogglePush()}
                      label="Notifications push"
                    />
                  </div>
                  {pushActive && (
                    <div style={{ marginTop: 8, display: "flex", justifyContent: "flex-end" }}>
                      <button type="button" className="lo-btn-small" onClick={handleTestPush}>
                        Envoyer un test
                      </button>
                    </div>
                  )}
                  {notifError && (
                    <p className="lo-error" style={{ marginTop: 8 }}>
                      {notifError}
                    </p>
                  )}
                </div>
              </section>

              <section className="lo-section">
                <h2 className="lo-section-title">Appairage</h2>
                <div className="lo-note">
                  <p>
                    Un autre serveur s'ajoute par code QR depuis l'écran de connexion. Le code
                    d'appairage du bureau se lit avec la caméra du téléphone.
                  </p>
                </div>
              </section>
            </>
          )}

          {/* ── Stockage : sur l'ordinateur ── */}
          {section === "storage" && (
            <SurOrdinateur
              quoi="Le stockage — emplacement des modèles, espace disque, nettoyage"
              detail="Les modèles vivent sur le disque de la machine qui héberge le serveur : c'est là que se gèrent l'espace et le nettoyage."
            />
          )}

          {/* ── À propos : version et mise à jour ── */}
          {section === "about" && (
            <>
              <VersionSection />
              <section className="lo-section">
                <h2 className="lo-section-title">Système</h2>
                <div className="lo-note">
                  <p>
                    Les licences et les informations système détaillées sont sur l'ordinateur, dans
                    Paramètres → À propos.
                  </p>
                </div>
              </section>
            </>
          )}
        </>
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

/** Ce qui se règle uniquement sur la machine qui héberge le serveur. */
function SurOrdinateur({ quoi, detail }: { quoi: string; detail?: string }) {
  return (
    <section className="lo-section">
      <div className="lo-note">
        <p>
          <strong>{quoi}</strong> se règle sur l'ordinateur qui héberge le serveur Locaryn.
        </p>
        {detail && (
          <p className="lo-hint" style={{ marginTop: 6 }}>
            {detail}
          </p>
        )}
        <p className="lo-hint" style={{ marginTop: 6 }}>
          Ouvrez les Paramètres du bureau, dans la catégorie correspondante : le téléphone en
          reprendra les effets.
        </p>
      </div>
    </section>
  );
}

/** L'historique des conversations, comme « Conversation » sur le bureau. */
function ConversationHistory({ onOpenChat }: { onOpenChat: (id: string) => void }) {
  const [search, setSearch] = useState("");
  const [libres, setLibres] = useState<Conversation[] | null>(null);
  const [projets, setProjets] = useState<PhoneProject[] | null>(null);
  const [parProjet, setParProjet] = useState<Record<string, Conversation[]>>({});

  useEffect(() => {
    let actif = true;
    void (async () => {
      const l = await api.listConversations().catch(() => [] as Conversation[]);
      if (!actif) return;
      setLibres(l);
      const ps = await api.listProjects().catch(() => [] as PhoneProject[]);
      if (!actif) return;
      setProjets(ps);
      const tout: Record<string, Conversation[]> = {};
      await Promise.all(
        ps.map(async (p) => {
          const list = await api.listProjectConversations(p.id).catch(() => [] as Conversation[]);
          tout[p.id] = list;
        }),
      );
      if (actif) setParProjet(tout);
    })();
    return () => {
      actif = false;
    };
  }, []);

  const q = search.trim().toLowerCase();
  const filtre = (c: Conversation) => !q || c.title.toLowerCase().includes(q);

  const libresFiltrees = (libres ?? []).filter(filtre);
  const groupesProjets = (projets ?? [])
    .map((p) => ({ p, chats: (parProjet[p.id] ?? []).filter(filtre) }))
    .filter((g) => g.chats.length > 0);
  const total = libresFiltrees.length + groupesProjets.reduce((n, g) => n + g.chats.length, 0);

  return (
    <>
      <section className="lo-section">
        <h2 className="lo-section-title">Historique des conversations</h2>
        <p className="lo-hint">
          Retrouvez vos échanges par espace de travail. Touchez une conversation pour l'ouvrir dans
          le chat. Les archivées sont rangées dans Compte → Archives.
        </p>
      </section>

      <div className="lo-search-box">
        <input
          type="text"
          className="lo-search-input"
          placeholder="Rechercher une conversation…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </div>

      {libres === null && (
        <div className="lo-loading-row" role="status">
          <span className="lo-spinner" aria-hidden />
          <span>Chargement…</span>
        </div>
      )}
      {libres !== null && total === 0 && (
        <p className="lo-sub">
          {search ? "Aucune conversation ne correspond." : "Aucune conversation à afficher."}
        </p>
      )}

      {libresFiltrees.length > 0 && (
        <section className="lo-section">
          <h3 className="lo-settings-groupe">Conversations</h3>
          <ul className="lo-cards">
            {libresFiltrees.map((c) => (
              <LigneConversation key={c.id} c={c} onOpen={onOpenChat} />
            ))}
          </ul>
        </section>
      )}

      {groupesProjets.map(({ p, chats }) => (
        <section key={p.id} className="lo-section">
          <h3 className="lo-settings-groupe">{p.name}</h3>
          <ul className="lo-cards">
            {chats.map((c) => (
              <LigneConversation key={c.id} c={c} onOpen={onOpenChat} />
            ))}
          </ul>
        </section>
      ))}
    </>
  );
}

function LigneConversation({ c, onOpen }: { c: Conversation; onOpen: (id: string) => void }) {
  return (
    <li>
      <button type="button" className="lo-row" onClick={() => onOpen(c.id)}>
        <span className="lo-row-icon">
          <Icon name="chat" />
        </span>
        <span className="lo-row-text">
          <span className="lo-row-label">{c.title}</span>
          <span className="lo-hint">
            {c.last_message_at ? dateCourte(c.last_message_at) : "Conversation récente"}
          </span>
        </span>
        <span className="lo-row-go">
          <Icon name="forward" size={14} />
        </span>
      </button>
    </li>
  );
}

function dateCourte(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/** Les projets du serveur, avec leur nombre de conversations. */
function ProjectsSection() {
  const [projets, setProjets] = useState<PhoneProject[] | null>(null);
  const [comptes, setComptes] = useState<Record<string, number>>({});

  useEffect(() => {
    let actif = true;
    void (async () => {
      const ps = await api.listProjects().catch(() => [] as PhoneProject[]);
      if (!actif) return;
      setProjets(ps);
      const n: Record<string, number> = {};
      await Promise.all(
        ps.map(async (p) => {
          const list = await api.listProjectConversations(p.id).catch(() => [] as Conversation[]);
          n[p.id] = list.length;
        }),
      );
      if (actif) setComptes(n);
    })();
    return () => {
      actif = false;
    };
  }, []);

  return (
    <>
      <section className="lo-section">
        <h2 className="lo-section-title">Projets du serveur</h2>
        {projets === null && (
          <div className="lo-loading-row" role="status">
            <span className="lo-spinner" aria-hidden />
            <span>Chargement…</span>
          </div>
        )}
        {projets?.length === 0 && (
          <p className="lo-sub">Aucun projet. Créez-en un depuis l'historique.</p>
        )}
        <ul className="lo-cards">
          {projets?.map((p) => (
            <li key={p.id} className="lo-card">
              <span className="lo-row-icon">
                <Icon name="project" />
              </span>
              <div className="lo-card-text">
                <span className="lo-card-title">{p.name}</span>
                <span className="lo-hint">
                  {comptes[p.id] ?? 0} conversation{comptes[p.id] === 1 ? "" : "s"}
                </span>
              </div>
            </li>
          ))}
        </ul>
      </section>

      <section className="lo-section">
        <h2 className="lo-section-title">Gestion</h2>
        <div className="lo-note">
          <p>
            Les autorisations, la base de connaissances et l'archivage d'un projet se gèrent sur
            l'ordinateur, dans Paramètres → Projets.
          </p>
        </div>
      </section>
    </>
  );
}
