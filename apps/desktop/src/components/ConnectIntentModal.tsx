import { Icon } from "@locaryn/ui-core";
import { useState, useSyncExternalStore } from "react";
import { core } from "../lib/core";
import { consumePendingInstall, getPendingInstall, subscribeDeepLink } from "../lib/deepLink";
import { ModalShell } from "./ModalShell";

/**
 * Le pop-up de consentement du lien `locaryn://connect?…`.
 *
 * Le .exe que le morph Remote génère et le QR du téléphone portent ce lien :
 * l'ouvrir demande à l'application de se connecter à un serveur. La connexion
 * ne part jamais d'un clic sur le lien — elle part d'ici, après que la
 * personne a vu où elle va et dit oui. Un refus, Échap ou un clic dehors ne
 * touche à rien : le lien est simplement oublié.
 *
 * Le composant s'abonne lui-même au store d'intents, donc il peut monter tout
 * en haut de l'arbre — il réagit aussi bien à un lien reçu à froid (l'app
 * vient d'être ouverte par le .exe) qu'à chaud, même panneau de réglages
 * fermé. Un intent `install` en attente lui est invisible : il reste pour le
 * panneau des extensions.
 */
export function ConnectIntentModal() {
  // Le store ne change qu'à set/consume : la référence est stable, le hook
  // ne re-rend que pour un vrai nouvel intent.
  const snapshot = useSyncExternalStore(subscribeDeepLink, getPendingInstall, () => null);
  const connect = snapshot?.action === "connect" ? snapshot : null;

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Le mot de passe ne vient que d'un lien que l'utilisateur lui-même a
  // exporté ; il n'est jamais affiché, seulement réutilisé. Sinon, le champ
  // reste à remplir — comme sur l'écran de connexion classique. Le premier
  // rendu gagnant porte ces états : un nouvel intent remonte une modale
  // fraîche après consumption du précédent.
  const [password, setPassword] = useState(connect?.password ?? "");
  const [user, setUser] = useState(connect?.user ?? "");

  if (!connect) return null;

  // Alias pour aider TS : `connect` est capturé par les fonctions ci-dessous,
  // où le narrowing du guard ne s'applique plus.
  const ctx = connect;
  const needUser = !ctx.user;
  const needPassword = connect.password === undefined;
  const ready = (!needUser || user.trim().length > 0) && (!needPassword || password.length > 0);

  // Le bundle client est un secret : le daemon hôte ne le sert qu'à qui
  // possède déjà un compte. Le lien qui demande la connexion prouve aussi le
  // droit de la télécharger — les identifiants qu'il porte (ou que la personne
  // vient de saisir) partent en Basic sur le téléchargement, pas ailleurs.
  const basicUser = ctx.user?.trim() || user.trim();
  const basicPassword = connect.password !== undefined ? connect.password : password;
  const auth =
    basicUser && basicPassword ? { user: basicUser, password: basicPassword } : undefined;

  async function accept() {
    if (!ready) return;
    setBusy(true);
    setError(null);
    try {
      // Les certificats d'abord : la connexion en a besoin pour être sûre.
      // Un échec ici stoppe tout — mieux vaut pas de connexion qu'une
      // connexion qui croit être sûre sans l'être. L'erreur renvoie vers
      // l'installation manuelle, qui reste possible dans les réglages.
      if (ctx.cert) {
        await core.installClientCertificateFromUrl(ctx.cert, ctx.ca, auth);
      }
      await core.signIn(ctx.server, ctx.user?.trim() || user.trim(), password);
      consumePendingInstall();
      // Comme l'écran de connexion : l'app se ré-évalue au démarrage et
      // remonte la session fraîche partout.
      window.location.reload();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  function refuse() {
    consumePendingInstall();
  }

  return (
    <ModalShell
      onClose={refuse}
      className="locaryn-card"
      style={{ maxWidth: 440, width: "calc(100vw - 48px)" }}
      label="Demande de connexion à un serveur"
    >
      <div className="locaryn-gate-head">
        <Icon name="plugs-connected" size={16} />
        <h3>Se connecter à ce serveur&nbsp;?</h3>
      </div>
      <p>
        Un lien de connexion demande à cette application de dialoguer avec le serveur suivant. Vos
        conversations et vos modèles passeront par lui tant que la connexion sera active.
      </p>
      <div className="locaryn-kv-list" style={{ margin: "12px 0" }}>
        <div className="locaryn-kv">
          <span className="locaryn-kv-key">Serveur</span>
          <span className="locaryn-kv-val locaryn-kv-mono">{ctx.server}</span>
        </div>
        {!needUser && (
          <div className="locaryn-kv">
            <span className="locaryn-kv-key">Compte</span>
            <span className="locaryn-kv-val locaryn-kv-mono">{ctx.user}</span>
          </div>
        )}
        {ctx.cert && (
          <div className="locaryn-kv">
            <span className="locaryn-kv-key">Certificat</span>
            <span className="locaryn-kv-val">
              {ctx.ca ? "installé avec ce lien (client + autorité)" : "installé avec ce lien"}
            </span>
          </div>
        )}
      </div>
      {ctx.password !== undefined && (
        <p className="locaryn-field-hint" style={{ marginTop: 0 }}>
          Le mot de passe est enregistré dans ce lien — il ne sera ni affiché ni replacé ailleurs.
          Ne transmettez un tel lien qu'à vos propres machines.
        </p>
      )}
      {needUser && (
        <div style={{ marginTop: 10 }}>
          <label htmlFor="connect-intent-user" className="locaryn-field-label">
            Identifiant
          </label>
          <input
            id="connect-intent-user"
            className="locaryn-input"
            style={{ width: "100%", marginTop: 4 }}
            value={user}
            autoFocus
            disabled={busy}
            onChange={(e) => setUser(e.target.value)}
          />
        </div>
      )}
      {needPassword && (
        <div style={{ marginTop: 10 }}>
          <label htmlFor="connect-intent-password" className="locaryn-field-label">
            Mot de passe
          </label>
          <input
            id="connect-intent-password"
            type="password"
            className="locaryn-input"
            style={{ width: "100%", marginTop: 4 }}
            value={password}
            disabled={busy}
            onChange={(e) => setPassword(e.target.value)}
          />
        </div>
      )}
      {error && (
        <p style={{ color: "var(--danger, #e5484d)", fontSize: "0.85rem" }}>
          {error}
          {ctx.cert &&
            " — le certificat peut aussi s'installer à la main dans Réglages → Connexion."}
        </p>
      )}
      <div className="locaryn-gate-actions" style={{ marginTop: 16 }}>
        <button type="button" className="locaryn-btn-ghost" onClick={refuse} disabled={busy}>
          Refuser
        </button>
        <button
          type="button"
          className="locaryn-btn-primary"
          onClick={accept}
          disabled={busy || !ready}
        >
          {busy ? "Connexion…" : "Se connecter"}
        </button>
      </div>
    </ModalShell>
  );
}
