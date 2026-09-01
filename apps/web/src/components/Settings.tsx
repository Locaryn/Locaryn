import { Icon, type IconName } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type TokenInfo, type WebStatus, api } from "../lib/core";

/** Les choix d'expiration d'une clé API, dans l'ordre où on les lit. */
const EXPIRATIONS: { value: number | null; label: string }[] = [
  { value: null, label: "Jamais" },
  { value: 7, label: "7 jours" },
  { value: 30, label: "30 jours" },
  { value: 90, label: "90 jours" },
];

function dateCourte(iso: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "—" : d.toLocaleDateString();
}
import {
  ACCENT_PRESETS,
  type ReglageTheme,
  type ThemeMode,
  appliquerTheme,
  lireTheme,
} from "../lib/theme";
import { Screen } from "./Screen";

/** Les trois réglages de thème, dans l'ordre où ils se lisent. */
const MODES_THEME: { value: ThemeMode; label: string; icon: IconName }[] = [
  { value: "system", label: "Système", icon: "monitor" },
  { value: "light", label: "Clair", icon: "sun" },
  { value: "dark", label: "Sombre", icon: "moon" },
];

type Props = {
  status: WebStatus;
  onBack: () => void;
  onSignedOut: (s: WebStatus) => void;
  onMemory: () => void;
};

/**
 * Réglages — le même écran que sur le téléphone : le serveur, ce qu'il
 * retient de vous, et votre profil (identifiant, mot de passe). Le reste —
 * modèles, extensions — se décide sur la machine d'en face, et se regarde
 * depuis le menu principal.
 */
export function Settings({ status, onBack, onSignedOut, onMemory }: Props) {
  const [me, setMe] = useState<{ username: string; role: string; local?: boolean } | null>(null);
  const [theme, setTheme] = useState<ReglageTheme>(() => lireTheme());
  const [current, setCurrent] = useState("");
  const [nouveau, setNouveau] = useState("");
  const [confirme, setConfirme] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);
  // Les deux circuits, séparés dès la lecture : un écran par circuit.
  const [apiKeys, setApiKeys] = useState<TokenInfo[]>([]);
  const [devices, setDevices] = useState<TokenInfo[]>([]);
  const [newKeyLabel, setNewKeyLabel] = useState("");
  const [newKeyExpiry, setNewKeyExpiry] = useState<number | null>(null);
  const [freshToken, setFreshToken] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const reload = useCallback(async () => {
    try {
      setMe(await api.me());
    } catch {
      // Le mode local (sans compte) n'a pas de profil à montrer.
    }
    try {
      const tokens = await api.listTokens();
      setApiKeys(tokens.filter((t) => t.kind === "api"));
      setDevices(tokens.filter((t) => t.kind === "session"));
    } catch {
      // Pas de compte : les listes restent vides, les sections ne s'affichent pas.
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function changePassword() {
    if (busy) return;
    setError(null);
    setDone(false);
    if (nouveau.length < 8) {
      setError("Le nouveau mot de passe doit faire 8 caractères au minimum.");
      return;
    }
    if (nouveau !== confirme) {
      setError("La confirmation ne correspond pas au nouveau mot de passe.");
      return;
    }
    setBusy(true);
    try {
      await api.changePassword(current, nouveau);
      setCurrent("");
      setNouveau("");
      setConfirme("");
      setDone(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Screen title="Réglages" onBack={onBack}>
      {status.signed_in && me && (
        <section className="lo-section">
          <h2 className="lo-section-title">Profil</h2>
          <p className="lo-hint">
            {me.username}
            {me.local
              ? " — compte local"
              : ` — ${me.role === "admin" ? "administrateur" : "membre"}`}
          </p>
          {!me.local && (
            <div className="lo-stack">
              <label className="lo-label" htmlFor="pw-current">
                Mot de passe actuel
              </label>
              <input
                id="pw-current"
                type="password"
                className="lo-input"
                value={current}
                onChange={(e) => setCurrent(e.target.value)}
              />
              <label className="lo-label" htmlFor="pw-nouveau">
                Nouveau mot de passe
              </label>
              <input
                id="pw-nouveau"
                type="password"
                className="lo-input"
                value={nouveau}
                onChange={(e) => setNouveau(e.target.value)}
              />
              <label className="lo-label" htmlFor="pw-confirme">
                Confirmation
              </label>
              <input
                id="pw-confirme"
                type="password"
                className="lo-input"
                value={confirme}
                onChange={(e) => setConfirme(e.target.value)}
              />
              <button
                type="button"
                className="lo-btn"
                disabled={busy || !current || !nouveau || !confirme}
                onClick={() => void changePassword()}
              >
                {busy ? "Enregistrement…" : "Changer le mot de passe"}
              </button>
              {done && <p className="lo-sub">Mot de passe changé.</p>}
              {error && <p className="lo-error">{error}</p>}
            </div>
          )}
        </section>
      )}

      <section className="lo-section">
        <h2 className="lo-section-title">Serveur</h2>
        <p className="lo-hint">{status.server_name ?? "Aucun serveur enregistré"}</p>
      </section>

      {/* La déconnexion vit ici, pas dans la barre du chat : un bouton qui ne
          sert qu'à quitter sa session n'a rien à faire à côté de la saisie. */}
      <section className="lo-section">
        <h2 className="lo-section-title">Session</h2>
        <button
          type="button"
          className="lo-btn-ghost"
          onClick={() => void api.signOut().then(onSignedOut)}
        >
          Se déconnecter
        </button>
      </section>

      <section className="lo-section">
        <h2 className="lo-section-title">Thème</h2>
        <p className="lo-hint">
          Le navigateur décide par défaut. En clair, l'accent s'assombrit tout seul pour rester
          lisible.
        </p>
        <div className="lo-segmented" style={{ marginTop: 12 }} role="group">
          {MODES_THEME.map((m) => (
            <button
              key={m.value}
              type="button"
              className={`lo-segment${theme.mode === m.value ? " lo-segment-on" : ""}`}
              aria-pressed={theme.mode === m.value}
              onClick={() => {
                const suivant = { ...theme, mode: m.value };
                setTheme(suivant);
                appliquerTheme(suivant, true);
              }}
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
          La teinte unique de l'interface — la même palette que sur l'ordinateur.
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
                onClick={() => {
                  const suivant = { ...theme, hex: p.hex };
                  setTheme(suivant);
                  appliquerTheme(suivant, true);
                }}
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
      </section>

      {/* ---- Circuit A : clés API développeur ---------------------------- */}
      {status.signed_in && me && !me.local && (
        <section className="lo-section">
          <h2 className="lo-section-title">Clés API / Développeur</h2>
          <p className="lo-hint">
            Pour les extensions d'IDE, scripts et intégrations tierces. La clé ne s'affiche qu'une
            fois à sa création — copiez-la immédiatement.
          </p>

          {freshToken && (
            <div className="lo-stack" style={{ margin: "12px 0" }}>
              <label className="lo-label" htmlFor="fresh-token">
                Votre nouvelle clé (visible une seule fois)
              </label>
              <div style={{ display: "flex", gap: 8 }}>
                <input
                  id="fresh-token"
                  readOnly
                  className="lo-input"
                  value={freshToken}
                  onFocus={(e) => e.currentTarget.select()}
                />
                <button
                  type="button"
                  className="lo-btn"
                  onClick={() => {
                    void navigator.clipboard.writeText(freshToken).then(() => {
                      setCopied(true);
                      setTimeout(() => setCopied(false), 2000);
                    });
                  }}
                >
                  {copied ? "Copié ✓" : "Copier"}
                </button>
              </div>
              <p className="lo-hint">Ce texte disparaîtra au rechargement de la page.</p>
            </div>
          )}

          <div className="lo-stack">
            <label className="lo-label" htmlFor="key-label">
              Nom de la clé
            </label>
            <input
              id="key-label"
              className="lo-input"
              placeholder="VS Code, script de déploiement…"
              value={newKeyLabel}
              onChange={(e) => setNewKeyLabel(e.target.value)}
            />
            <label className="lo-label" htmlFor="key-expiry">
              Expiration
            </label>
            <select
              id="key-expiry"
              className="lo-input"
              value={newKeyExpiry ?? ""}
              onChange={(e) =>
                setNewKeyExpiry(e.target.value === "" ? null : Number(e.target.value))
              }
            >
              {EXPIRATIONS.map((o) => (
                <option key={o.label} value={o.value ?? ""}>
                  {o.label}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="lo-btn"
              disabled={busy || !newKeyLabel.trim()}
              onClick={async () => {
                setBusy(true);
                setError(null);
                try {
                  const created = await api.createToken(newKeyLabel.trim(), newKeyExpiry);
                  setFreshToken(created.token);
                  setNewKeyLabel("");
                  await reload();
                } catch (e) {
                  setError(String(e));
                } finally {
                  setBusy(false);
                }
              }}
            >
              {busy ? "Création…" : "Créer une clé"}
            </button>
          </div>

          {apiKeys.length > 0 && (
            <div style={{ marginTop: 12 }}>
              {apiKeys.map((k) => (
                <div
                  key={k.id}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                    padding: "8px 0",
                    opacity: k.revoked_at ? 0.5 : 1,
                  }}
                >
                  <Icon name="key" size={15} />
                  <span style={{ flex: 1 }}>
                    <strong>{k.label ?? "Sans nom"}</strong>
                    <span className="lo-hint">
                      {" "}
                      ····{k.hint} · créée le {dateCourte(k.created_at)}
                      {k.expires_at
                        ? ` · expire le ${dateCourte(k.expires_at)}`
                        : " · sans expiration"}
                      {k.revoked_at ? " · révoquée" : ""}
                    </span>
                  </span>
                  {!k.revoked_at && (
                    <button
                      type="button"
                      className="lo-btn-ghost"
                      onClick={async () => {
                        await api.revokeToken(k.id).catch(() => undefined);
                        await reload();
                      }}
                    >
                      Révoquer
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
        </section>
      )}

      {/* ---- Circuit B : appareils connectés ------------------------------ */}
      {status.signed_in && me && !me.local && devices.length > 0 && (
        <section className="lo-section">
          <h2 className="lo-section-title">Appareils connectés</h2>
          <p className="lo-hint">
            Sessions ouvertes par connexion ou appairage QR. Déconnecter un appareil révoque sa
            session immédiatement.
          </p>
          <div style={{ marginTop: 8 }}>
            {devices.map((d) => (
              <div
                key={d.id}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 10,
                  padding: "8px 0",
                  opacity: d.revoked_at ? 0.5 : 1,
                }}
              >
                <Icon name="devices" size={15} />
                <span style={{ flex: 1 }}>
                  <strong>{d.label ?? "Appareil"}</strong>
                  <span className="lo-hint">
                    {" "}
                    · dernier usage {dateCourte(d.last_used_at ?? d.created_at)}
                    {d.expires_at ? ` · session jusqu'au ${dateCourte(d.expires_at)}` : ""}
                    {d.revoked_at ? " · déconnecté" : ""}
                  </span>
                </span>
                {!d.revoked_at && (
                  <button
                    type="button"
                    className="lo-btn-ghost"
                    onClick={async () => {
                      await api.revokeToken(d.id).catch(() => undefined);
                      await reload();
                    }}
                  >
                    Déconnecter
                  </button>
                )}
              </div>
            ))}
          </div>
        </section>
      )}

      {status.signed_in && (
        <section className="lo-section">
          <h2 className="lo-section-title">Personnalisation</h2>
          <button type="button" className="lo-row-nav" onClick={onMemory}>
            <span className="lo-row-text">
              <span className="lo-row-label">Mémoire</span>
              <span className="lo-hint">Ce que le serveur retient de vous</span>
            </span>
            <span className="lo-row-go">›</span>
          </button>
        </section>
      )}
    </Screen>
  );
}
