import { capabilityLabel } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { CATALOGUE, type Capability, type PhoneExtension, api } from "../lib/core";
import { Screen } from "./Screen";

type Props = {
  onBack: () => void;
  /** Une extension a bougé : les capacités changent, donc l'interface aussi. */
  onChanged: () => void;
};

/**
 * Les extensions du serveur, sur leur propre écran.
 *
 * Elles s'installent sur la machine d'en face : c'est elle qui télécharge le
 * dépôt. Le téléphone ne fait que désigner lequel — il n'a aucun fichier à
 * fournir, et n'a pas à en avoir.
 */
export function Extensions({ onBack, onChanged }: Props) {
  const [installed, setInstalled] = useState<PhoneExtension[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  /** La liste canonique du serveur : les labels vivants, sans recompiler. */
  const [canonique, setCanonique] = useState<Capability[]>([]);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const [exts, caps] = await Promise.all([
        api.listExtensions(),
        // Le serveur fait foi ; la copie embarquée ne sert que de repli.
        api
          .listCapabilities()
          .catch(() => []),
      ]);
      setInstalled(exts);
      setCanonique(caps);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function act(key: string, run: () => Promise<unknown>) {
    setBusy(key);
    setError(null);
    try {
      await run();
      await reload();
      onChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  const byName = new Map((installed ?? []).map((e) => [e.name, e]));

  return (
    <Screen title="Extensions" onBack={onBack}>
      <p className="lo-hint">
        Elles s'installent sur le serveur et valent pour tous ses appareils. Ce qu'elles apportent
        apparaît ensuite dans le Studio et dans les réponses du modèle.
      </p>

      {installed === null && !error && <p className="lo-sub">Chargement…</p>}
      {error && <p className="lo-error">{error}</p>}

      <ul className="lo-cards">
        {CATALOGUE.map((c) => {
          const name = c.repo.split("/")[1];
          const on = byName.get(name);
          const working = busy === c.repo;
          return (
            <li key={c.repo} className="lo-card">
              <div className="lo-card-text">
                <span className="lo-card-title">{c.label}</span>
                <span className="lo-hint">{c.note}</span>
                {on && on.capabilities.length > 0 && (
                  <span className="lo-hint">
                    Capacités :{" "}
                    {on.capabilities
                      .map((id) => canonique.find((c) => c.id === id)?.label ?? capabilityLabel(id))
                      .join(" · ")}
                  </span>
                )}
                {on && !on.enabled && <span className="lo-tag">désactivée</span>}
              </div>
              <div className="lo-card-actions">
                {on ? (
                  <>
                    <button
                      type="button"
                      className="lo-btn-small"
                      disabled={working}
                      onClick={() => act(c.repo, () => api.setExtensionEnabled(name, !on.enabled))}
                    >
                      {on.enabled ? "Désactiver" : "Activer"}
                    </button>
                    <button
                      type="button"
                      className="lo-btn-small"
                      disabled={working}
                      onClick={() => act(c.repo, () => api.removeExtension(name))}
                    >
                      Retirer
                    </button>
                  </>
                ) : (
                  <button
                    type="button"
                    className="lo-btn-small lo-btn-small-on"
                    disabled={working}
                    onClick={() => act(c.repo, () => api.installExtension(c.repo))}
                  >
                    {working ? "Installation…" : "Installer"}
                  </button>
                )}
              </div>
            </li>
          );
        })}
      </ul>
    </Screen>
  );
}
