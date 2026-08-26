import { Icon, type IconName } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import {
  type CatalogSource,
  ECOSYSTEM_LABELS,
  type ExtensionEcosystem,
  type ExtensionPermission,
  type ExtensionSourcePreview,
  type InstalledExtension,
  PERMISSION_LABELS,
  core,
} from "../lib/core";
import { pickAnyFile, pickFolder } from "../lib/dialog";
import { ExtensionPermissionsModal } from "./ExtensionPermissionsModal";

type Props = {
  /** "extension" = dépôt / dossier / ZIP ; "marketplace" = dépôt marketplace.json. */
  kind?: "extension" | "marketplace";
  /** Source pré-remplie dans le champ (deep link `locaryn://install?src=…`). */
  initialSource?: string;
  onClose: () => void;
  /** Extension installée et (le cas échéant) activée. `enable` = activée.
   *  Attendu avant la fermeture : le panneau parent rafraîchit sa liste. */
  onExtensionInstalled?: (ext: InstalledExtension, enable: boolean) => void | Promise<void>;
  onMarketplaceAdded?: (sources: CatalogSource[]) => void | Promise<void>;
};

/**
 * Fenêtre d'ajout d'une extension depuis un dépôt GitHub, un dossier local ou
 * une archive ZIP (ou d'une marketplace Claude Code). Partagée entre le
 * panneau Extensions et le panneau Connecteurs — la source accepte
 * `owner/repo`, une URL GitHub, `github:owner/repo@tag`, un chemin local ou
 * un `.zip`, le format étant détecté automatiquement côté Rust.
 *
 * L'installation passe par la fenêtre d'autorisations quand le manifeste en
 * déclare ; `onExtensionInstalled` n'est appelé qu'une fois les permissions
 * enregistrées (et l'extension activée si demandé).
 */
export function ExtensionInstallDialog({
  kind = "extension",
  initialSource,
  onClose,
  onExtensionInstalled,
  onMarketplaceAdded,
}: Props) {
  const [spec, setSpec] = useState(initialSource ?? "");
  // Erreur affichée À L'INTÉRIEUR de la fenêtre : un `setError` du panneau
  // parent passerait sous le fond de la modale, donc invisible.
  const [dialogError, setDialogError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState<InstalledExtension | null>(null);
  const [pendingGrants, setPendingGrants] = useState<Set<ExtensionPermission>>(new Set());
  const inputRef = useRef<HTMLInputElement | null>(null);
  // Aperçu du manifeste (fetch léger côté Rust, pas de téléchargement du
  // paquet). Débouncé ; un changement de source invalide la réponse en vol.
  // `previewError` explique pourquoi l'aperçu est absent (dépôt privé, source
  // non lisible…) au lieu d'une carte qui disparaît en silence.
  const [preview, setPreview] = useState<ExtensionSourcePreview | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const previewIdRef = useRef(0);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Aperçu automatique de la source tapée : nom, version, écosystème et
  // permissions demandées, affichés dans la carte de confirmation.
  useEffect(() => {
    if (kind !== "extension") {
      setPreview(null);
      setPreviewing(false);
      setPreviewError(null);
      return;
    }
    const s = spec.trim();
    if (!s) {
      setPreview(null);
      setPreviewing(false);
      setPreviewError(null);
      return;
    }
    const id = ++previewIdRef.current;
    setPreviewing(true);
    setPreviewError(null);
    const t = setTimeout(async () => {
      try {
        const p = await core.previewExtensionSource(s);
        if (previewIdRef.current === id) setPreview(p);
      } catch (e) {
        if (previewIdRef.current === id) {
          setPreview(null);
          setPreviewError(
            String(e)
              .replace(/^Error:\s*/, "")
              .slice(0, 140),
          );
        }
      } finally {
        if (previewIdRef.current === id) setPreviewing(false);
      }
    }, 500);
    return () => clearTimeout(t);
  }, [spec, kind]);

  // Échap ferme la fenêtre de permissions d'abord, sinon la fenêtre entière.
  useEffect(() => {
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key !== "Escape") return;
      if (pending) setPending(null);
      else onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pending, onClose]);

  async function submit() {
    const s = spec.trim();
    if (!s) return;
    setBusy(true);
    setDialogError(null);
    try {
      if (kind === "marketplace") {
        const sources = await core.addCatalogSource(s);
        await onMarketplaceAdded?.(sources);
        onClose();
        return;
      }
      const ext = await core.installExtension(s);
      if (ext.permissions.length === 0) {
        await core.setExtensionEnabled(ext.id, true);
        await onExtensionInstalled?.(ext, true);
        onClose();
      } else {
        setPending(ext);
        setPendingGrants(new Set(ext.permissions.map((p) => p.permission)));
      }
    } catch (e) {
      setDialogError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <div className="locaryn-settings-backdrop">
        {/* A real button, so dismissing by clicking away is reachable from
            the keyboard and from a screen reader instead of being an
            onClick nailed to a div. */}
        <button
          type="button"
          className="locaryn-backdrop-dismiss"
          aria-label="Fermer"
          onClick={onClose}
        />
        <dialog
          open
          className="locaryn-card locaryn-modal-card"
          aria-modal="true"
          aria-label="Installer une extension"
          style={{
            width: 520,
            margin: "100px auto",
            padding: 20,
            position: "relative",
            zIndex: 10,
          }}
          onClick={(e) => e.stopPropagation()}
          onKeyDown={(e) => e.stopPropagation()}
        >
          <h3 style={{ marginBottom: 4 }}>
            {kind === "marketplace"
              ? "Ajouter une marketplace"
              : "Installer une extension (dépôt, dossier ou ZIP)"}
          </h3>
          <p className="locaryn-field-hint" style={{ marginBottom: 14 }}>
            {kind === "marketplace" ? (
              <>
                Dépôt GitHub contenant <code>.claude-plugin/marketplace.json</code>.
              </>
            ) : (
              <>
                Accepte <code>owner/repo</code>, une URL GitHub (y compris{" "}
                <code>/tree/branche/sous-dossier</code>), <code>github:owner/repo@tag</code>, un
                dossier local ou une archive <code>.zip</code>. Le format est détecté
                automatiquement : plugin Locaryn, plugin Claude Code, extension Gemini CLI, paquet
                OpenCode ou <code>.mcp.json</code> seul.
              </>
            )}
          </p>
          {dialogError && (
            <p className="locaryn-field-hint" style={{ color: "var(--danger)", marginBottom: 10 }}>
              {dialogError}
            </p>
          )}
          <div className="locaryn-field">
            <input
              className="locaryn-input"
              ref={inputRef}
              placeholder={
                kind === "marketplace"
                  ? "anthropics/claude-code"
                  : "owner/repo  ·  https://github.com/…  ·  ./mon-plugin"
              }
              value={spec}
              onChange={(ev) => {
                setSpec(ev.target.value);
                setDialogError(null);
              }}
              onKeyDown={(ev) => {
                if (ev.key === "Enter") submit();
              }}
            />
            {kind === "extension" && (
              <>
                <div style={{ display: "flex", gap: 8, marginTop: 8, flexWrap: "wrap" }}>
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    style={{ fontSize: 12 }}
                    onClick={async () => {
                      if (!navigator.clipboard?.readText) {
                        setDialogError(
                          "Presse-papiers inaccessible — collez le lien à la main (Ctrl+V).",
                        );
                        return;
                      }
                      try {
                        const text = await navigator.clipboard.readText();
                        if (text.trim()) {
                          setSpec(text.trim());
                          setDialogError(null);
                          inputRef.current?.focus();
                        }
                      } catch {
                        setDialogError(
                          "Presse-papiers illisible (permission refusée) — collez le lien à la main (Ctrl+V).",
                        );
                      }
                    }}
                  >
                    Coller
                  </button>
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    style={{ fontSize: 12 }}
                    onClick={async () => {
                      const p = await pickFolder();
                      if (p) {
                        setSpec(p);
                        inputRef.current?.focus();
                      }
                    }}
                  >
                    Choisir un dossier…
                  </button>
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    style={{ fontSize: 12 }}
                    onClick={async () => {
                      const p = await pickAnyFile("Archive ZIP", ["zip"]);
                      if (p) {
                        setSpec(p);
                        inputRef.current?.focus();
                      }
                    }}
                  >
                    Choisir une archive ZIP…
                  </button>
                </div>
                <div style={{ marginTop: 12 }}>
                  <p className="locaryn-field-hint" style={{ marginBottom: 6 }}>
                    Suggestions certifiées Locaryn & populaires :
                  </p>
                  <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
                    {(
                      [
                        {
                          icon: "image",
                          label: "Images",
                          repo: "Locaryn/morph-image",
                        },
                        { icon: "video", label: "Vidéo", repo: "Locaryn/morph-video-gen" },
                        { icon: "cube", label: "3D", repo: "Locaryn/morph-3d-gen" },
                        { icon: "mic", label: "Synthèse vocale", repo: "Locaryn/morph-voice-tts" },
                        { icon: "music", label: "Musique", repo: "Locaryn/morph-music-gen" },
                        {
                          icon: "search",
                          label: "Vision et OCR",
                          repo: "Locaryn/morph-vision-ocr",
                        },
                        {
                          icon: "models",
                          label: "Documents et RAG",
                          repo: "Locaryn/morph-rag-qa",
                        },
                        {
                          icon: "translate",
                          label: "Traduction",
                          repo: "Locaryn/morph-translation",
                        },
                        {
                          icon: "chart",
                          label: "Analyse de texte",
                          repo: "Locaryn/morph-text-analysis",
                        },
                        { icon: "server", label: "SSH", repo: "Locaryn/morph-ssh" },
                        {
                          icon: "cloud",
                          label: "Tunnel Remote",
                          repo: "Locaryn/morph-travel-tunnel",
                        },
                        {
                          icon: "shield",
                          label: "Atelier LoRA",
                          repo: "Locaryn/morph-model-training",
                        },
                      ] as { icon: IconName; label: string; repo: string }[]
                    ).map((preset) => (
                      <button
                        key={preset.repo}
                        type="button"
                        className="locaryn-chip"
                        style={{ fontSize: 11, padding: "2px 8px" }}
                        onClick={() => {
                          setSpec(preset.repo);
                          inputRef.current?.focus();
                        }}
                      >
                        <Icon name={preset.icon} size={12} /> {preset.label}
                      </button>
                    ))}
                  </div>
                </div>
              </>
            )}
          </div>
          {(preview || previewing || previewError) && kind === "extension" && (
            <div className="locaryn-card" style={{ marginTop: 12, padding: "10px 12px" }}>
              {previewing && !preview ? (
                <p className="locaryn-field-hint" style={{ margin: 0 }}>
                  Analyse de la source…
                </p>
              ) : preview ? (
                <>
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                      gap: 8,
                    }}
                  >
                    <strong style={{ fontSize: 13 }}>{preview.name}</strong>
                    <span className="locaryn-tag">
                      {ECOSYSTEM_LABELS[preview.ecosystem as ExtensionEcosystem] ??
                        preview.ecosystem}
                    </span>
                  </div>
                  {preview.version && (
                    <p className="locaryn-field-hint" style={{ margin: "2px 0 0" }}>
                      v{preview.version}
                    </p>
                  )}
                  {preview.description && (
                    <p className="locaryn-box-desc" style={{ marginTop: 6 }}>
                      {preview.description}
                    </p>
                  )}
                  <p className="locaryn-field-hint" style={{ marginTop: 6 }}>
                    Manifeste : <code>{preview.manifest_file}</code>
                    {preview.author ? ` · ${preview.author}` : ""}
                  </p>
                  {preview.requested_permissions.length > 0 && (
                    <p className="locaryn-field-hint" style={{ marginTop: 6 }}>
                      Permissions demandées :{" "}
                      {preview.requested_permissions
                        .map((p) => PERMISSION_LABELS[p as ExtensionPermission] ?? p)
                        .join(", ")}
                    </p>
                  )}
                  {preview.mcp_servers.length > 0 && (
                    <div style={{ marginTop: 6 }}>
                      <p className="locaryn-field-hint" style={{ margin: "0 0 4px" }}>
                        Serveurs MCP déclarés ({preview.mcp_servers.length}) :
                      </p>
                      {preview.mcp_servers.map((s) => (
                        <div
                          key={s.name}
                          style={{
                            display: "flex",
                            alignItems: "baseline",
                            gap: 6,
                            fontSize: 12,
                            marginBottom: 2,
                          }}
                        >
                          <code style={{ fontSize: 12, flexShrink: 0 }}>{s.name}</code>
                          {s.command ? (
                            <span className="locaryn-field-hint" style={{ wordBreak: "break-all" }}>
                              — {s.command}
                            </span>
                          ) : s.url ? (
                            <span className="locaryn-field-hint" style={{ wordBreak: "break-all" }}>
                              — {s.url}
                            </span>
                          ) : null}
                        </div>
                      ))}
                    </div>
                  )}
                </>
              ) : previewError ? (
                <p className="locaryn-field-hint" style={{ margin: 0, color: "var(--text-faint)" }}>
                  Aperçu indisponible pour cette source : {previewError}
                </p>
              ) : null}
            </div>
          )}
          <div style={{ display: "flex", gap: 8, justifyContent: "flex-end", marginTop: 16 }}>
            <button type="button" className="locaryn-btn-ghost" onClick={onClose}>
              Annuler
            </button>
            <button
              type="button"
              className="locaryn-btn-primary"
              disabled={busy || !spec.trim()}
              onClick={submit}
            >
              {busy ? "…" : kind === "marketplace" ? "Ajouter" : "Installer"}
            </button>
          </div>
        </dialog>
      </div>

      {pending && (
        <ExtensionPermissionsModal
          extension={pending}
          initialGrants={pendingGrants}
          onDone={async (ext, enable) => {
            // Attendre le callback AVANT de fermer : un échec (ex. panneau
            // parent injoignable) garde la modale ouverte, erreur visible.
            await onExtensionInstalled?.(ext, enable);
            setPending(null);
            onClose();
          }}
        />
      )}
    </>
  );
}
