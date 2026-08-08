import { useState } from "react";
import {
  type ExtensionPermission,
  type InstalledExtension,
  PERMISSION_LABELS,
  core,
} from "../lib/core";

type Props = {
  extension: InstalledExtension;
  /** Cases cochées au départ (tout pour une installation, l'existant pour une édition). */
  initialGrants: Set<ExtensionPermission>;
  /** Appelé une fois les permissions enregistrées ; `enable` = activer
   *  l'extension. Attendu avant que la modale ne se ferme. Le parent ferme
   *  la modale via son propre rendu conditionnel ; Échap est géré par le
   *  parent (ou le dialogue) qui l'affiche. */
  onDone: (ext: InstalledExtension, enable: boolean) => void | Promise<void>;
};

/**
 * La fenêtre d'autorisations d'une extension. Partagée entre l'installation
 * depuis le catalogue, l'installation depuis la fenêtre d'ajout (dépôt /
 * dossier / ZIP) et l'édition des permissions déjà accordées.
 *
 * Refuser une permission n'empêche pas le chargement : l'extension tourne
 * sans la fonctionnalité concernée.
 */
export function ExtensionPermissionsModal({ extension, initialGrants, onDone }: Props) {
  const [grants, setGrants] = useState<Set<ExtensionPermission>>(() => new Set(initialGrants));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function save(enable: boolean) {
    setBusy(true);
    setError(null);
    try {
      await core.setExtensionPermissions(extension.id, [...grants]);
      if (enable) await core.setExtensionEnabled(extension.id, true);
      // Le callback peut être asynchrone (rafraîchissement du panneau parent) :
      // attendu ici pour qu'un échec soit attrapé et affiché, pas perdu.
      await onDone(extension, enable);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="lochor-settings-backdrop">
      <dialog
        open
        className="lochor-card lochor-modal-card"
        aria-modal="true"
        aria-label={`Autorisations demandées par ${extension.name}`}
        style={{ width: 520, margin: "80px auto", padding: 20 }}
      >
        <h3 style={{ marginBottom: 4 }}>{extension.name} demande des autorisations</h3>
        <p className="lochor-field-hint" style={{ marginBottom: 14 }}>
          Refusez ce qui n'est pas nécessaire : l'extension se charge quand même, sans la
          fonctionnalité concernée.
        </p>
        {error && (
          <p className="lochor-field-hint" style={{ color: "var(--danger)", marginBottom: 10 }}>
            {error}
          </p>
        )}

        {extension.permissions.map((p) => (
          <label
            key={p.permission}
            style={{
              display: "flex",
              gap: 10,
              alignItems: "flex-start",
              padding: "10px 0",
              borderBottom: "1px solid var(--border)",
              cursor: "pointer",
            }}
          >
            <input
              type="checkbox"
              checked={grants.has(p.permission)}
              onChange={(ev) => {
                const next = new Set(grants);
                if (ev.target.checked) next.add(p.permission);
                else next.delete(p.permission);
                setGrants(next);
              }}
            />
            <span>
              <strong style={{ fontSize: 13 }}>{PERMISSION_LABELS[p.permission]}</strong>
              {p.reason && (
                <span className="lochor-field-hint" style={{ display: "block" }}>
                  {p.reason}
                </span>
              )}
            </span>
          </label>
        ))}

        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end", marginTop: 16 }}>
          <button
            type="button"
            className="lochor-btn-ghost"
            disabled={busy}
            onClick={() => save(false)}
          >
            Enregistrer sans activer
          </button>
          <button
            type="button"
            className="lochor-btn-primary"
            disabled={busy}
            onClick={() => save(true)}
          >
            {busy ? "…" : "Autoriser et activer"}
          </button>
        </div>
      </dialog>
    </div>
  );
}
