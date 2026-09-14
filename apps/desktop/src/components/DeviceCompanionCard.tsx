import { Icon } from "@locaryn/ui-core";
import { useState } from "react";
import { type ExtensionPermission, type InstalledExtension, core } from "../lib/core";
import { ExtensionPermissionsModal } from "./ExtensionPermissionsModal";

type Props = {
  /** L'extension du serveur, marquée `device_install_pending`. */
  extension: InstalledExtension;
};

/**
 * Un compagnon d'appareil du serveur, pas encore installé sur ce poste.
 *
 * L'application ne sait rien de ce qu'il fait : elle dit seulement qu'il doit
 * tourner ici pour agir sur ce poste, et propose de l'installer — avec ses
 * autorisations, décidées comme pour toute installation. Une fois activé, son
 * propre panneau prend la place de cette carte.
 */
export function DeviceCompanionCard({ extension }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [toAuthorize, setToAuthorize] = useState<InstalledExtension | null>(null);

  async function install() {
    if (!extension.source) {
      setError("Le serveur n'indique pas d'où installer cette extension.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      setToAuthorize(await core.installExtensionOnDevice(extension.source));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="locaryn-card" style={{ padding: 20, maxWidth: 640 }}>
      <div
        className="locaryn-field-label"
        style={{ display: "flex", alignItems: "center", gap: 8 }}
      >
        <Icon name="extensions" size={16} />
        {extension.display_name || extension.name} doit aussi être installé ici
      </div>
      <p className="locaryn-field-hint" style={{ marginTop: 8 }}>
        Le serveur auquel ce poste est connecté utilise cette extension, et elle agit sur chaque
        machine : elle a besoin de tourner sur celle-ci pour s'en occuper.
        {extension.description ? ` ${extension.description}` : ""}
      </p>
      <div style={{ display: "flex", gap: 8, marginTop: 16, flexWrap: "wrap" }}>
        <button
          type="button"
          className="locaryn-btn-primary"
          style={{ minHeight: 40 }}
          disabled={busy}
          onClick={() => void install()}
        >
          {busy ? "Installation…" : "Installer sur cet appareil"}
        </button>
      </div>
      {error && (
        <div className="locaryn-vp-error" style={{ marginTop: 12 }}>
          {error}
        </div>
      )}

      {toAuthorize && (
        <ExtensionPermissionsModal
          extension={toAuthorize}
          initialGrants={
            new Set<ExtensionPermission>(toAuthorize.permissions.map((p) => p.permission))
          }
          onDone={() => {
            setToAuthorize(null);
            window.dispatchEvent(new Event("locaryn:extensions-changed"));
          }}
        />
      )}
    </div>
  );
}
