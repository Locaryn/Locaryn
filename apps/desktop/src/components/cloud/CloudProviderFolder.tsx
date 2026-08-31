import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  type CloudModel,
  type CloudProvider,
  type CloudProviderStatus,
  type InstalledExtension,
  core,
} from "../../lib/core";
import { DynamicPluginWidget } from "../extensions/DynamicPluginWidget";
import { getSlotContributions } from "../extensions/SlotRegistry";

/**
 * Les dossiers de fournisseurs distants.
 *
 * Un morph qui déclare un `cloud_provider` fait apparaître un dossier là où
 * l'utilisateur cherche ses modèles : dans « Mes modèles », à la place d'une
 * carte de modèle, et dans le sélecteur du chat. C'est le même geste que pour
 * un modèle installé — on clique, et on choisit — sauf que derrière il n'y a
 * rien à télécharger.
 *
 * Ce fichier tient les trois morceaux : la tuile, la page qui s'ouvre derrière,
 * et le tableau de bord de secours quand l'extension n'apporte pas le sien.
 */

/** Les fournisseurs distants actifs, relus à la demande. */
export function useCloudProviders(): {
  providers: CloudProvider[];
  reload: () => void;
} {
  const [providers, setProviders] = useState<CloudProvider[]>([]);
  /* Monté ou non : une réponse qui arrive après la fermeture de l'écran ne
     doit pas écrire dans un composant démonté. */
  const mounted = useRef(true);

  const reload = useCallback(() => {
    core
      .cloudProviders()
      .then((list) => {
        if (mounted.current) setProviders(list);
      })
      .catch(() => {
        // Aucun fournisseur distant : l'écran montre les modèles locaux, ce
        // qu'il faisait avant que les catalogues distants existent.
      });
  }, []);

  useEffect(() => {
    mounted.current = true;
    reload();
    return () => {
      mounted.current = false;
    };
  }, [reload]);

  return { providers, reload };
}

/** Une taille lisible pour une fenêtre de contexte. */
export function formatContext(tokens: number): string {
  if (tokens <= 0) return "—";
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(tokens % 1_000_000 ? 1 : 0)} M`;
  if (tokens >= 1000) return `${Math.round(tokens / 1000)} k`;
  return String(tokens);
}

/** Le prix d'un million de jetons, ou « gratuit ». */
export function formatPrice(perMillion: number | null): string {
  if (perMillion === null) return "—";
  if (perMillion === 0) return "gratuit";
  return perMillion < 1
    ? `${perMillion.toFixed(2)} $`
    : `${perMillion.toFixed(perMillion < 10 ? 2 : 0)} $`;
}

// ============================================================================
// La tuile
// ============================================================================

/**
 * Le dossier, à la place d'une carte de modèle.
 *
 * Il prend exactement la même place qu'un modèle installé : c'est ce qui fait
 * qu'on le trouve sans l'avoir cherché. Ce qu'il montre — clé posée ou non,
 * nombre de modèles, modèle actif — répond aux trois questions qu'on se pose
 * avant de cliquer.
 */
export function CloudProviderTile({
  provider,
  onOpen,
}: {
  provider: CloudProvider;
  onOpen: () => void;
}) {
  return (
    <button
      type="button"
      className="locaryn-box-card locaryn-cloud-folder"
      onClick={onOpen}
      title={`Ouvrir le dossier ${provider.label}`}
    >
      <div className="locaryn-cloud-folder-head">
        <span className="locaryn-cloud-folder-icon" aria-hidden="true">
          <Icon name="cloud" size={22} />
        </span>
        <div style={{ minWidth: 0 }}>
          <span className="locaryn-box-brand" style={{ fontSize: 10 }}>
            Fournisseur distant · {provider.extension_name}
          </span>
          <h3 className="locaryn-box-name" style={{ fontSize: 14, margin: "4px 0 0" }}>
            {provider.label}
          </h3>
        </div>
        <Icon name="forward" size={16} />
      </div>

      <p className="locaryn-cloud-folder-desc">
        {provider.active_model
          ? `Modèle actif : ${provider.active_model}`
          : provider.has_key
            ? "Clé enregistrée. Ouvrez pour choisir un modèle."
            : "Collez votre clé pour utiliser vos modèles payants ici."}
      </p>

      <div className="locaryn-cloud-folder-tags">
        <span className={`locaryn-tag${provider.has_key ? " locaryn-tag-installed" : ""}`}>
          {provider.has_key ? "Clé enregistrée" : "Pas de clé"}
        </span>
        {provider.model_count > 0 && (
          <span className="locaryn-tag">{provider.model_count} modèles</span>
        )}
      </div>
    </button>
  );
}

// ============================================================================
// La page
// ============================================================================

/**
 * La page ouverte derrière le dossier.
 *
 * Si l'extension apporte son propre écran (slot `models.folder`), c'est le
 * sien qui s'affiche — un fournisseur sait mieux que l'application ce qu'il a
 * à montrer. Sinon l'application dessine le tableau de bord ci-dessous, pour
 * qu'un manifeste minimal reste utilisable.
 */
export function CloudProviderScreen({
  provider,
  extensions,
  onBack,
  onChanged,
}: {
  provider: CloudProvider;
  extensions: InstalledExtension[];
  onBack: () => void;
  onChanged?: () => void;
}) {
  const contribution = getSlotContributions(extensions, "models.folder").find(
    (c) => c.extensionId === provider.extension_id || c.value === provider.id,
  );

  return (
    <section className="locaryn-cloud-screen">
      <div className="locaryn-cloud-screen-head">
        <button type="button" className="locaryn-icon-btn" onClick={onBack} title="Revenir">
          <Icon name="back" size={16} />
        </button>
        <div>
          <h2 style={{ margin: 0, fontSize: 16 }}>{provider.label}</h2>
          <p className="locaryn-view-desc" style={{ margin: "2px 0 0" }}>
            Apporté par {provider.extension_name} · {provider.api_url}
          </p>
        </div>
      </div>

      {contribution ? (
        <DynamicPluginWidget
          contribution={contribution}
          context={{ providerId: provider.id }}
          className="locaryn-cloud-screen-body"
        />
      ) : (
        <CloudProviderDashboard provider={provider} onChanged={onChanged} />
      )}
    </section>
  );
}

/**
 * L'état d'une passerelle qui tourne sur la machine.
 *
 * Une passerelle éteinte est le premier cas d'échec de ce dossier : le
 * catalogue est vide, le choix d'un modèle échoue, et rien à l'écran ne dit
 * pourquoi. Cette carte le dit, et propose le seul geste utile — la démarrer,
 * avec la commande que son manifeste déclare.
 */
export function CloudGatewayCard({
  provider,
  onRunning,
}: {
  provider: CloudProvider;
  onRunning?: () => void;
}) {
  const [status, setStatus] = useState<CloudProviderStatus | null>(null);
  const [starting, setStarting] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /* Le rappel change d'identité à chaque rendu du parent. Le mettre dans les
     dépendances de la sonde relançait la sonde à chaque rendu, qui relançait
     le chargement du catalogue, qui redessinait le parent : la liste restait
     désactivée pour toujours, et aucun modèle n'était choisissable. */
  const onRunningRef = useRef(onRunning);
  onRunningRef.current = onRunning;
  /* Prévenir au passage à « en marche », pas à chaque sonde : sinon le
     catalogue serait relu toutes les secondes pour rien. */
  const wasRunning = useRef(false);

  const announce = useCallback((next: CloudProviderStatus) => {
    setStatus(next);
    if (next.running && !wasRunning.current) onRunningRef.current?.();
    wasRunning.current = next.running;
  }, []);

  const probe = useCallback(async () => {
    try {
      announce(await core.cloudProviderStatus(provider.id));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [provider.id, announce]);

  useEffect(() => {
    void probe();
  }, [probe]);

  async function start() {
    setStarting(true);
    setError(null);
    try {
      announce(await core.cloudProviderStart(provider.id));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setStarting(false);
    }
  }

  async function install() {
    setInstalling(true);
    setError(null);
    try {
      await core.cloudProviderInstall(provider.id);
      announce(await core.cloudProviderStatus(provider.id));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setInstalling(false);
    }
  }

  if (!provider.is_local) return null;
  const running = status?.running === true;
  /* Tant que l'état n'est pas revenu, on croit le manifeste : afficher
     « pas installée » sur une passerelle qui l'est ferait proposer une
     installation inutile. */
  const installed = status?.installed ?? provider.installed;

  return (
    <div className="locaryn-card locaryn-cloud-gateway">
      <div className="locaryn-field-head" style={{ marginBottom: 8 }}>
        <h3 style={{ margin: 0, fontSize: 13 }}>La passerelle sur votre machine</h3>
        <span className={`locaryn-tag${running ? " locaryn-tag-installed" : ""}`}>
          {running ? "En marche" : "Éteinte"}
        </span>
      </div>
      <p className="locaryn-cloud-help">
        {status?.detail ?? `Vérification de ${provider.label} sur ${provider.api_url}…`}
      </p>
      <div className="locaryn-cloud-key-row">
        {!running && !installed && provider.can_install && (
          <button
            type="button"
            className="locaryn-btn-primary"
            onClick={() => void install()}
            disabled={installing || starting}
          >
            {installing ? "Installation…" : `Installer ${provider.label}`}
          </button>
        )}
        {!running && provider.can_start && (
          <button
            type="button"
            className="locaryn-btn-primary"
            onClick={() => void start()}
            disabled={starting || installing}
          >
            {starting
              ? "Démarrage…"
              : installed
                ? "Démarrer la passerelle"
                : "Installer et démarrer"}
          </button>
        )}
        <button type="button" className="locaryn-chip" onClick={() => void probe()}>
          <Icon name="refresh" size={14} /> Vérifier
        </button>
        {provider.dashboard_url && (
          <button
            type="button"
            className="locaryn-chip"
            onClick={() => void core.cloudProviderOpenDashboard(provider.id).catch(() => {})}
            title="Ouvrir le tableau de bord dans le navigateur"
          >
            Ouvrir dans le navigateur
          </button>
        )}
      </div>
      {error && (
        <div className="locaryn-cloud-error" style={{ marginTop: 12 }}>
          {error}
        </div>
      )}

      {/* Le tableau de bord lui-même. C'est là que l'utilisateur connecte ses
          fournisseurs et colle leurs clés : cette page appartient à la
          passerelle, on ne la réécrit pas, on la montre. */}
      {running && provider.dashboard_url && (
        <iframe
          className="locaryn-cloud-frame"
          src={provider.dashboard_url}
          title={`Tableau de bord ${provider.label}`}
        />
      )}
    </div>
  );
}

// ============================================================================
// Le tableau de bord de secours
// ============================================================================

/**
 * Ce que l'application montre quand l'extension n'apporte pas d'écran.
 *
 * Volontairement complet : un manifeste de dix lignes doit suffire à rendre un
 * fournisseur utilisable. Une extension qui veut mieux le remplace.
 */
export function CloudProviderDashboard({
  provider,
  onChanged,
}: {
  provider: CloudProvider;
  onChanged?: () => void;
}) {
  const [models, setModels] = useState<CloudModel[]>([]);
  const [query, setQuery] = useState("");
  const [keyInput, setKeyInput] = useState("");
  const [hasKey, setHasKey] = useState(provider.has_key);
  const [activeModel, setActiveModel] = useState<string | null>(provider.active_model);
  const [busy, setBusy] = useState<null | "models" | "key" | "select">(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const loadModels = useCallback(
    async (refresh: boolean) => {
      setBusy("models");
      setError(null);
      try {
        setModels(await core.cloudProviderModels(provider.id, refresh));
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(null);
      }
    },
    [provider.id],
  );

  useEffect(() => {
    void loadModels(false);
  }, [loadModels]);

  async function saveKey() {
    setBusy("key");
    setError(null);
    try {
      await core.cloudProviderSetKey(provider.id, keyInput);
      setHasKey(true);
      setKeyInput("");
      setNotice("Clé enregistrée dans le trousseau du système.");
      onChanged?.();
      // Une clé change ce que le catalogue renvoie : modèles réservés,
      // tarifs négociés. Le relire tout de suite évite un écran périmé.
      await loadModels(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  async function clearKey() {
    setError(null);
    try {
      await core.cloudProviderClearKey(provider.id);
      setHasKey(false);
      setActiveModel(null);
      setNotice("Clé effacée.");
      onChanged?.();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function select(model: string) {
    setBusy("select");
    setError(null);
    try {
      await core.cloudProviderSelect(provider.id, model);
      setActiveModel(model);
      setNotice(`${model} est maintenant le modèle de la conversation.`);
      onChanged?.();
      window.dispatchEvent(
        new CustomEvent("locaryn:cloud-model-selected", {
          detail: { provider: provider.id, model },
        }),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  const filtered = models.filter((m) => {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return `${m.id} ${m.name} ${m.description}`.toLowerCase().includes(q);
  });

  return (
    <div className="locaryn-cloud-dashboard">
      {/* Une passerelle locale d'abord : éteinte, le reste de l'écran ne peut
          rien faire. */}
      <CloudGatewayCard provider={provider} onRunning={() => void loadModels(false)} />

      {/* La clé ensuite : sans elle, le choix d'un modèle est refusé. */}
      <div className="locaryn-card locaryn-cloud-key">
        <div className="locaryn-field-head" style={{ marginBottom: 8 }}>
          <h3 style={{ margin: 0, fontSize: 13 }}>Votre clé {provider.label}</h3>
          <span className={`locaryn-tag${hasKey ? " locaryn-tag-installed" : ""}`}>
            {hasKey ? "Enregistrée" : "Absente"}
          </span>
        </div>
        <p className="locaryn-cloud-help">
          Elle est gardée dans le trousseau du système et n'en ressort jamais — ni vers cette
          extension, ni vers un fichier de configuration. Vous payez vos jetons directement chez{" "}
          {provider.label}.
        </p>
        <div className="locaryn-cloud-key-row">
          <input
            className="locaryn-input"
            type="password"
            autoComplete="off"
            placeholder={provider.key_hint ?? "Collez votre clé"}
            value={keyInput}
            onChange={(e) => setKeyInput(e.target.value)}
          />
          <button
            type="button"
            className="locaryn-btn-primary"
            onClick={() => void saveKey()}
            disabled={busy !== null || keyInput.trim().length === 0}
          >
            {busy === "key" ? "…" : "Enregistrer"}
          </button>
          {hasKey && (
            <button type="button" className="locaryn-chip" onClick={() => void clearKey()}>
              Effacer
            </button>
          )}
        </div>
        <div className="locaryn-cloud-links">
          {provider.keys_url && (
            <a href={provider.keys_url} target="_blank" rel="noreferrer">
              Créer une clé
            </a>
          )}
          {provider.docs_url && (
            <a href={provider.docs_url} target="_blank" rel="noreferrer">
              Documentation
            </a>
          )}
        </div>
      </div>

      {error && <div className="locaryn-cloud-error">{error}</div>}
      {notice && !error && <div className="locaryn-cloud-notice">{notice}</div>}

      <div className="locaryn-cloud-toolbar">
        <input
          className="locaryn-input"
          placeholder={`Rechercher parmi ${models.length} modèles…`}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button
          type="button"
          className="locaryn-chip"
          onClick={() => void loadModels(true)}
          disabled={busy === "models"}
          title="Relire la liste chez le fournisseur"
        >
          <Icon name="refresh" size={14} /> {busy === "models" ? "…" : "Actualiser"}
        </button>
      </div>

      <div className="locaryn-cloud-models">
        {busy === "models" && models.length === 0 && (
          <div className="locaryn-cloud-empty">Lecture du catalogue…</div>
        )}
        {busy !== "models" && filtered.length === 0 && (
          <div className="locaryn-cloud-empty">Aucun modèle ne correspond.</div>
        )}
        {filtered.map((model) => (
          <div
            key={model.id}
            className={`locaryn-cloud-model${activeModel === model.id ? " locaryn-active" : ""}`}
          >
            <div style={{ minWidth: 0 }}>
              <div className="locaryn-cloud-model-name">{model.name}</div>
              <div className="locaryn-cloud-model-id">{model.id}</div>
              <div className="locaryn-cloud-model-facts">
                <span>{formatContext(model.context_length)} jetons</span>
                <span>
                  {formatPrice(model.prompt_price_per_m)} /{" "}
                  {formatPrice(model.completion_price_per_m)} par M
                </span>
                {model.supports_tools && <span>outils</span>}
                {model.modality.includes("image") && <span>vision</span>}
              </div>
            </div>
            <button
              type="button"
              className={activeModel === model.id ? "locaryn-chip locaryn-chip-on" : "locaryn-chip"}
              onClick={() => void select(model.id)}
              disabled={busy !== null}
            >
              {activeModel === model.id ? "Actif" : "Utiliser"}
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
