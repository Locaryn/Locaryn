import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { type MigrationProgress, type StorageInfo, core, formatBytes } from "../lib/core";
import { pickFolder } from "../lib/dialog";

type Props = {
  /** Jump to the model marketplace, where weights are actually managed. */
  onOpenMarketplace?: () => void;
};

/** Below this, a drive is close enough to full that installing a model will
 *  fail. Worth flagging before the user starts a 20 GB download. */
const LOW_SPACE_BYTES = 10 * 1024 ** 3;

/**
 * Where Locaryn keeps its bulky data, and how to move it elsewhere.
 *
 * Model weights and engine binaries reach tens of gigabytes; on a machine
 * whose system drive is nearly full, being able to relocate them is the
 * difference between a usable app and a broken one. This panel shows what
 * costs what, which volume has room, and performs the move.
 */
export function StorageSettings({ onOpenMarketplace }: Props) {
  const [info, setInfo] = useState<StorageInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [progress, setProgress] = useState<MigrationProgress | null>(null);
  const [pendingRoot, setPendingRoot] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  /** Guards against a late refresh landing after the component unmounts. */
  const alive = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const next = await core.storageInfo();
      if (alive.current) setInfo(next);
    } catch (e) {
      if (alive.current) setError(String(e));
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    void refresh();
    // Migration runs on a background thread and reports through this event.
    const un = listen<MigrationProgress>("storage-migration", (ev) => {
      if (!alive.current) return;
      setProgress(ev.payload.done ? null : ev.payload);
    });
    return () => {
      alive.current = false;
      void un.then((f) => f());
    };
  }, [refresh]);

  async function handleChooseFolder() {
    setError(null);
    setNotice(null);
    const picked = await pickFolder();
    if (!picked) return;
    if (info && picked === info.root) {
      setNotice("C'est déjà le dossier utilisé.");
      return;
    }
    setPendingRoot(picked);
  }

  async function applyRoot(moveData: boolean) {
    if (!pendingRoot) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const next = await core.setStorageRoot(pendingRoot, moveData);
      if (!alive.current) return;
      setInfo(next);
      setPendingRoot(null);
      setNotice(
        moveData
          ? "Données déplacées. Le nouveau dossier est actif."
          : "Nouveau dossier enregistré. Les anciens fichiers sont restés sur place — les modèles déjà téléchargés n'apparaîtront plus tant qu'ils ne sont pas déplacés.",
      );
      // The engine processes hold paths resolved at startup.
      await refresh();
    } catch (e) {
      if (alive.current) setError(String(e));
    } finally {
      if (alive.current) {
        setBusy(false);
        setProgress(null);
      }
    }
  }

  async function handleCleanTemp() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const freed = await core.cleanTemp();
      if (!alive.current) return;
      setNotice(
        freed > 0 ? `${formatBytes(freed)} libérés.` : "Aucun fichier temporaire à supprimer.",
      );
      await refresh();
    } catch (e) {
      if (alive.current) setError(String(e));
    } finally {
      if (alive.current) setBusy(false);
    }
  }

  const pct =
    progress && progress.total_bytes > 0
      ? Math.min(100, (progress.moved_bytes / progress.total_bytes) * 100)
      : 0;

  return (
    <div className="locaryn-field">
      <label className="locaryn-field-label">Dossier de stockage</label>
      <p className="locaryn-field-hint">
        Les poids de modèles, les moteurs et les fichiers temporaires vivent ici. Placez-le sur un
        disque avec de l'espace : une seule famille de modèles dépasse souvent 20 Go.
      </p>

      <div className="locaryn-store-root">
        <code className="locaryn-store-path">{info?.root ?? "…"}</code>
        <span className="locaryn-store-tag">
          {info?.configured ? "personnalisé" : "emplacement par défaut"}
        </span>
      </div>

      <div className="locaryn-field-actions" style={{ marginTop: 12 }}>
        <button
          type="button"
          className="locaryn-btn-primary"
          disabled={busy}
          onClick={handleChooseFolder}
        >
          Changer de dossier…
        </button>
        <button
          type="button"
          className="locaryn-btn-ghost"
          disabled={busy}
          onClick={() => core.openModelsFolder().catch(() => {})}
        >
          Ouvrir le dossier des modèles
        </button>
        {onOpenMarketplace && (
          <button type="button" className="locaryn-btn-ghost" onClick={onOpenMarketplace}>
            Gérer les modèles
          </button>
        )}
      </div>

      {/* ── Confirmation: moving 40 GB is not a click to make silently ── */}
      {pendingRoot && (
        <div className="locaryn-store-confirm">
          <div className="locaryn-store-confirm-head">Déplacer vers</div>
          <code className="locaryn-store-path">{pendingRoot}</code>
          <p className="locaryn-field-hint" style={{ marginTop: 8 }}>
            {info ? formatBytes(info.total_bytes) : "—"} à transférer. Sur le même disque c'est
            instantané ; vers un autre disque, comptez plusieurs minutes.
          </p>
          <div className="locaryn-field-actions" style={{ marginTop: 10 }}>
            <button
              type="button"
              className="locaryn-btn-primary"
              disabled={busy}
              onClick={() => applyRoot(true)}
            >
              Déplacer les données
            </button>
            <button
              type="button"
              className="locaryn-btn-ghost"
              disabled={busy}
              onClick={() => applyRoot(false)}
            >
              Changer sans déplacer
            </button>
            <button
              type="button"
              className="locaryn-btn-ghost"
              disabled={busy}
              onClick={() => setPendingRoot(null)}
            >
              Annuler
            </button>
          </div>
        </div>
      )}

      {progress && (
        <div className="locaryn-store-progress">
          <div className="locaryn-store-progress-head">
            <span>{progress.phase}</span>
            <span className="locaryn-kv-mono">
              {formatBytes(progress.moved_bytes)} / {formatBytes(progress.total_bytes)}
            </span>
          </div>
          <div className="locaryn-footer-progress-track">
            <div className="locaryn-footer-progress-fill" style={{ width: `${pct}%` }} />
          </div>
          <div className="locaryn-store-progress-file">{progress.current_file}</div>
        </div>
      )}

      {error && <div className="locaryn-store-error">{error}</div>}
      {notice && !error && <div className="locaryn-store-notice">{notice}</div>}

      {/* ── What occupies the space ─────────────────────────────────── */}
      <label className="locaryn-field-label" style={{ marginTop: 24 }}>
        Occupation
      </label>
      <div className="locaryn-kv-list" style={{ marginTop: 8 }}>
        {(info?.entries ?? []).map((e) => (
          <div className="locaryn-kv" key={e.key}>
            <span className="locaryn-kv-key">
              {e.label}
              {e.outside_root && (
                <span
                  className="locaryn-store-flag"
                  title={`Hors du dossier de stockage : ${e.path}`}
                >
                  hors dossier
                </span>
              )}
            </span>
            <span className="locaryn-kv-val locaryn-kv-mono">
              {e.exists ? formatBytes(e.size_bytes) : "—"}
            </span>
          </div>
        ))}
        <div className="locaryn-kv">
          <span className="locaryn-kv-key">
            Base SQLite
            <span className="locaryn-store-tag" style={{ marginLeft: 8 }}>
              reste en place
            </span>
          </span>
          <span className="locaryn-kv-val locaryn-kv-mono">
            {info ? formatBytes(info.db_bytes) : "—"}
          </span>
        </div>
      </div>
      <p className="locaryn-field-hint" style={{ marginTop: 8 }}>
        La base ({info?.db_path ?? "—"}) ne suit pas le dossier de stockage : elle pèse quelques
        mégaoctets et reste ouverte pendant que l'application tourne, la déplacer à chaud risquerait
        de la corrompre.
      </p>

      <div className="locaryn-field-actions" style={{ marginTop: 12 }}>
        <button
          type="button"
          className="locaryn-btn-ghost"
          disabled={busy}
          onClick={handleCleanTemp}
        >
          Nettoyer les fichiers temporaires
        </button>
      </div>

      {/* ── Where there is room ─────────────────────────────────────── */}
      <label className="locaryn-field-label" style={{ marginTop: 24 }}>
        Disques
      </label>
      <div className="locaryn-store-drives">
        {(info?.drives ?? []).map((d) => {
          const used = d.total_bytes - d.free_bytes;
          const usedPct = d.total_bytes > 0 ? (used / d.total_bytes) * 100 : 0;
          const low = d.free_bytes < LOW_SPACE_BYTES;
          return (
            <div className="locaryn-store-drive" key={d.mount}>
              <div className="locaryn-store-drive-head">
                <span className="locaryn-kv-mono">{d.mount}</span>
                {d.is_current && <span className="locaryn-store-tag">utilisé</span>}
                <span className={`locaryn-store-drive-free${low ? " locaryn-store-low" : ""}`}>
                  {formatBytes(d.free_bytes)} libres
                </span>
              </div>
              <div className="locaryn-footer-progress-track">
                <div
                  className={`locaryn-footer-progress-fill${low ? " locaryn-store-low-fill" : ""}`}
                  style={{ width: `${usedPct}%` }}
                />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
