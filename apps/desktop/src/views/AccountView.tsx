import { Icon } from "@locaryn/ui-core";
import { useEffect, useMemo, useState } from "react";
import { ConversationHistorySettings } from "../components/ConversationHistorySettings";
import { MemorySettings } from "../components/MemorySettings";
import { ModelPreferencesSettings } from "../components/ModelPreferencesSettings";
import { type LocalProfile, type Project, type Session, core } from "../lib/core";
import { pickImageFile } from "../lib/dialog";
import { toMediaUrl } from "../lib/media";
import { ArchivesView } from "./ArchivesView";

type Props = {
  /** Ouvre une conversation sortie des archives dans le chat principal. */
  onOpenSession: (session: Session) => void;
  activeCapabilities?: string[];
  embedded?: boolean;
  /** Les conversations, pour l'historique consultable du compte. */
  projects?: Project[];
  sessionsByProject?: Record<string, Session[]>;
  standaloneSessions?: Session[];
};

type AccountSection = "profile" | "models" | "memory" | "archives" | "conversations";

/**
 * Espace compte.
 *
 * Une installation locale est déjà un compte utilisable : elle n'a pas besoin
 * d'une adresse distante pour avoir un profil, un avatar ou ses archives. La
 * connexion à un gateway reste une option, rangée dans le profil plutôt qu'au
 * premier plan de la navigation.
 */
export function AccountView({
  onOpenSession,
  activeCapabilities = [],
  embedded = false,
  projects = [],
  sessionsByProject = {},
  standaloneSessions = [],
}: Props) {
  const [section, setSection] = useState<AccountSection>("profile");
  const [serverUrl, setServerUrl] = useState("");
  const [token, setToken] = useState("");
  const [username, setUsername] = useState("");
  const [localProfile, setLocalProfile] = useState<LocalProfile>({
    display_name: "",
    avatar_path: null,
  });
  const [profileLoaded, setProfileLoaded] = useState(false);
  const [profileBusy, setProfileBusy] = useState(false);
  const [profileSaved, setProfileSaved] = useState(false);
  const [profileError, setProfileError] = useState<string | null>(null);
  const [isConnected, setIsConnected] = useState(false);

  useEffect(() => {
    let cancelled = false;
    core
      .getLocalProfile()
      .then((profile) => {
        if (cancelled) return;
        setLocalProfile(profile);
        setUsername(profile.display_name);
        setProfileLoaded(true);
      })
      .catch((error) => {
        if (cancelled) return;
        setProfileError(String(error));
        setProfileLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const displayName = username.trim() || "Local";
  const initial = useMemo(() => displayName.slice(0, 1).toUpperCase(), [displayName]);
  const avatarUrl = useMemo(
    () => (localProfile.avatar_path ? toMediaUrl(localProfile.avatar_path) : null),
    [localProfile.avatar_path],
  );

  async function saveProfileName() {
    if (!profileLoaded) return;
    setProfileBusy(true);
    setProfileError(null);
    try {
      const saved = await core.setLocalProfile(username);
      setLocalProfile(saved);
      setUsername(saved.display_name);
      setProfileSaved(true);
      window.setTimeout(() => setProfileSaved(false), 1600);
    } catch (error) {
      setProfileError(String(error));
    } finally {
      setProfileBusy(false);
    }
  }

  async function chooseAvatar() {
    const sourcePath = await pickImageFile();
    if (!sourcePath) return;
    setProfileBusy(true);
    setProfileError(null);
    try {
      const saved = await core.setLocalAvatar(sourcePath);
      setLocalProfile(saved);
      setProfileSaved(true);
      window.setTimeout(() => setProfileSaved(false), 1600);
    } catch (error) {
      setProfileError(String(error));
    } finally {
      setProfileBusy(false);
    }
  }

  async function clearAvatar() {
    setProfileBusy(true);
    setProfileError(null);
    try {
      setLocalProfile(await core.clearLocalAvatar());
      setProfileSaved(true);
      window.setTimeout(() => setProfileSaved(false), 1600);
    } catch (error) {
      setProfileError(String(error));
    } finally {
      setProfileBusy(false);
    }
  }

  const layout = (
    <div className="locaryn-account-layout">
      <nav className="locaryn-account-nav" aria-label="Sections du compte">
        <button
          type="button"
          className={`locaryn-account-nav-item${section === "profile" ? " locaryn-active" : ""}`}
          onClick={() => setSection("profile")}
        >
          <span className="locaryn-account-nav-icon" aria-hidden="true">
            {avatarUrl ? <img src={avatarUrl} alt="" /> : initial}
          </span>
          <span className="locaryn-account-nav-text">
            <strong>{displayName}</strong>
            <small>Profil local</small>
          </span>
        </button>
        <button
          type="button"
          className={`locaryn-account-nav-item${section === "models" ? " locaryn-active" : ""}`}
          onClick={() => setSection("models")}
        >
          <span className="locaryn-account-nav-icon locaryn-account-nav-icon-models">
            <Icon name="models" size={15} />
          </span>
          <span className="locaryn-account-nav-text">
            <strong>Préférences des modèles</strong>
            <small>Petites tâches, voix et images</small>
          </span>
        </button>
        <button
          type="button"
          className={`locaryn-account-nav-item${section === "conversations" ? " locaryn-active" : ""}`}
          onClick={() => setSection("conversations")}
        >
          <span className="locaryn-account-nav-icon">
            <Icon name="chat" size={15} />
          </span>
          <span className="locaryn-account-nav-text">
            <strong>Conversations</strong>
            <small>Historique et conversations récentes</small>
          </span>
        </button>
        <button
          type="button"
          className={`locaryn-account-nav-item${section === "memory" ? " locaryn-active" : ""}`}
          onClick={() => setSection("memory")}
        >
          <span className="locaryn-account-nav-icon locaryn-account-nav-icon-memory">
            <Icon name="memory" size={15} />
          </span>
          <span className="locaryn-account-nav-text">
            <strong>Mémoire</strong>
            <small>Ce que Locaryn retient</small>
          </span>
        </button>
        <button
          type="button"
          className={`locaryn-account-nav-item${section === "archives" ? " locaryn-active" : ""}`}
          onClick={() => setSection("archives")}
        >
          <span className="locaryn-account-nav-icon locaryn-account-nav-icon-archive">
            <Icon name="archive" size={15} />
          </span>
          <span className="locaryn-account-nav-text">
            <strong>Archives</strong>
            <small>Conversations rangées</small>
          </span>
        </button>
      </nav>

      <div className="locaryn-account-content">
        {section === "archives" ? (
          <ArchivesView onOpenSession={onOpenSession} />
        ) : section === "models" ? (
          <ModelPreferencesSettings activeCapabilities={activeCapabilities} />
        ) : section === "memory" ? (
          <MemorySettings />
        ) : section === "conversations" ? (
          <ConversationHistorySettings
            projects={projects}
            sessionsByProject={sessionsByProject}
            standaloneSessions={standaloneSessions}
            onOpenSession={onOpenSession}
          />
        ) : (
          <>
            <div className="locaryn-card locaryn-account-profile-card">
              <div className="locaryn-account-profile-head">
                <div
                  className="locaryn-account-avatar"
                  role="img"
                  aria-label={`Avatar de ${displayName}`}
                >
                  {avatarUrl ? <img src={avatarUrl} alt="" /> : initial}
                </div>
                <div className="locaryn-account-profile-copy">
                  <span className="locaryn-account-eyebrow">ZONE COMPTE</span>
                  <h3>{displayName}</h3>
                  <p>
                    {isConnected
                      ? "Compte relié à un serveur Locaryn."
                      : "Compte local autonome — aucune connexion distante configurée."}
                  </p>
                </div>
                <span className="locaryn-account-local-badge">
                  <span className="locaryn-health-dot locaryn-health-ok" />
                  {isConnected ? "Distant" : "Local"}
                </span>
              </div>

              <div className="locaryn-account-profile-note">
                Cet espace regroupe votre identité, vos réglages de compte et vos conversations
                archivées. Aucun compte en ligne n'est nécessaire pour utiliser Locaryn.
              </div>

              <div className="locaryn-field">
                <label className="locaryn-field-label" htmlFor="account-username">
                  Nom affiché / Alias
                </label>
                <input
                  id="account-username"
                  className="locaryn-input"
                  value={username}
                  placeholder="Local"
                  maxLength={80}
                  disabled={!profileLoaded || profileBusy}
                  onChange={(e) => setUsername(e.target.value)}
                  onBlur={() => void saveProfileName()}
                />
                <p className="locaryn-field-hint">
                  Ce nom et votre avatar sont conservés sur cette machine et restaurés au prochain
                  démarrage.
                </p>
              </div>

              <div className="locaryn-field-actions locaryn-account-profile-actions">
                <button
                  type="button"
                  className="locaryn-btn-primary"
                  disabled={!profileLoaded || profileBusy}
                  onClick={() => void saveProfileName()}
                >
                  {profileBusy ? "Enregistrement…" : "Enregistrer le profil"}
                </button>
                {profileSaved && <span className="locaryn-account-saved">Profil enregistré</span>}
              </div>

              <div className="locaryn-field locaryn-account-avatar-field">
                <div className="locaryn-field-label">Avatar personnalisé</div>
                <p className="locaryn-field-hint">
                  Choisissez une image PNG, JPG, WEBP ou BMP. Locaryn en conserve une copie locale,
                  même si l'original est déplacé.
                </p>
                <div className="locaryn-account-avatar-actions">
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    disabled={!profileLoaded || profileBusy}
                    onClick={() => void chooseAvatar()}
                  >
                    {localProfile.avatar_path ? "Changer l'avatar" : "Choisir un avatar"}
                  </button>
                  {localProfile.avatar_path && (
                    <button
                      type="button"
                      className="locaryn-btn-ghost locaryn-btn-danger"
                      disabled={profileBusy}
                      onClick={() => void clearAvatar()}
                    >
                      Retirer l'avatar
                    </button>
                  )}
                </div>
              </div>

              {profileError && <div className="locaryn-vp-error">{profileError}</div>}
            </div>

            <div className="locaryn-card locaryn-account-remote-card">
              <h3>Connexion distante (facultative)</h3>
              <p className="locaryn-field-hint">
                Reliez ce profil à un serveur privé uniquement si vous souhaitez synchroniser ou
                exécuter des tâches à distance. Sinon, le mode local reste actif.
              </p>
              <div className="locaryn-field">
                <label className="locaryn-field-label" htmlFor="account-server-url">
                  URL du serveur Locaryn
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
                  Jeton d'accès
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

              <div className="locaryn-field-actions locaryn-account-actions">
                <button
                  type="button"
                  className="locaryn-btn-primary"
                  disabled={!serverUrl.trim()}
                  onClick={() => setIsConnected(true)}
                >
                  Enregistrer la connexion
                </button>
                {isConnected && (
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    onClick={() => {
                      setIsConnected(false);
                      setToken("");
                    }}
                  >
                    Revenir au mode local
                  </button>
                )}
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );

  if (embedded) {
    return <div className="locaryn-account-embedded">{layout}</div>;
  }

  return (
    <section className="locaryn-view-container locaryn-account-page">
      <div className="locaryn-view-header">
        <h2>Compte · {displayName}</h2>
        <p className="locaryn-view-desc">
          Votre espace de compte et de profil. En mode local, vos conversations, modèles et réglages
          restent sur cette machine.
        </p>
      </div>
      {layout}
    </section>
  );
}
