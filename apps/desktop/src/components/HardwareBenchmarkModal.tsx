import { Icon } from "@locaryn/ui-core";
import { useState } from "react";
import { core } from "../lib/core";
import { MODEL_CATALOG } from "../lib/modelCatalog";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  onApplyFilter: (
    onlyRecommended: boolean,
    hw?: { total_ram_gb: number; total_vram_gb: number },
  ) => void;
};

export function HardwareBenchmarkModal({ isOpen, onClose, onApplyFilter }: Props) {
  const [ramGb, setRamGb] = useState<number>(16);
  const [vramGb, setVramGb] = useState<number>(8);
  const [cpuCores, setCpuCores] = useState<number>(8);
  const [preset, setPreset] = useState<string>("gaming");
  const [isScanning, setIsScanning] = useState(false);

  if (!isOpen) return null;

  function applyPreset(presetKey: string) {
    setPreset(presetKey);
    if (presetKey === "laptop") {
      setRamGb(8);
      setVramGb(2);
      setCpuCores(4);
    } else if (presetKey === "gaming") {
      setRamGb(16);
      setVramGb(8);
      setCpuCores(8);
    } else if (presetKey === "mac") {
      setRamGb(32);
      setVramGb(24);
      setCpuCores(10);
    } else if (presetKey === "server") {
      setRamGb(64);
      setVramGb(24);
      setCpuCores(16);
    }
  }

  async function runScanSimulation() {
    setIsScanning(true);
    try {
      const hw = await core.checkHardware();
      // Snap to closest sensible values for UI dropdowns
      setRamGb(Math.max(4, Math.ceil(hw.total_ram_gb / 4) * 4));
      const v = Math.round(hw.total_vram_gb);
      setVramGb(v <= 0 ? 0 : v);
      if (hw.cpu_cores) {
        setCpuCores(hw.cpu_cores);
      }
      setPreset("custom");
    } catch (e) {
      console.error(e);
    } finally {
      setIsScanning(false);
    }
  }

  let optimalCount = 0;
  let mediumCount = 0;
  let heavyCount = 0;

  for (const family of MODEL_CATALOG) {
    for (const variant of family.variants) {
      if (variant.storageGb <= vramGb || (vramGb >= 4 && variant.storageGb <= ramGb * 0.45)) {
        optimalCount++;
      } else if (variant.storageGb <= ramGb * 0.85) {
        mediumCount++;
      } else {
        heavyCount++;
      }
    }
  }

  return (
    <div
      className="locaryn-settings-backdrop"
      onClick={(e) => {
        // Seul un clic sur le fond ferme : un clic parti de la carte remonte jusqu'ici.
        if (e.target === e.currentTarget) onClose();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
    >
      <div
        className="locaryn-card"
        style={{
          width: "680px",
          maxHeight: "85vh",
          overflowY: "auto",
          margin: "40px auto",
          border: "1px solid var(--border-strong)",
          boxShadow: "0 16px 40px rgba(0,0,0,0.85)",
        }}
      >
        <div className="locaryn-field-head" style={{ marginBottom: "16px" }}>
          <div>
            <h3 style={{ margin: 0, display: "flex", alignItems: "center", gap: "8px" }}>
              <Icon name="chart" size={15} /> Analyseur de Performances & Compatibilité Matérielle
            </h3>
            <span style={{ fontSize: "var(--text-xs)", color: "var(--text-faint)" }}>
              Ajustez vos composants matériels pour identifier les modèles utilisables sur votre
              ordinateur.
            </span>
          </div>
          <button type="button" className="locaryn-icon-btn" onClick={onClose}>
            <Icon name="close" size={16} />
          </button>
        </div>

        {/* Presets Row */}
        <div style={{ marginBottom: "16px" }}>
          <span
            style={{
              fontSize: "var(--text-xs)",
              color: "var(--text-faint)",
              display: "block",
              marginBottom: "6px",
            }}
          >
            Sélectionner votre profil matériel :
          </span>
          <div style={{ display: "flex", gap: "8px", flexWrap: "wrap" }}>
            <button
              type="button"
              className={`locaryn-chip${preset === "laptop" ? " locaryn-chip-on" : ""}`}
              onClick={() => applyPreset("laptop")}
            >
              <Icon name="cpu" size={15} /> PC Portable (8 Go RAM, CPU 4c)
            </button>
            <button
              type="button"
              className={`locaryn-chip${preset === "gaming" ? " locaryn-chip-on" : ""}`}
              onClick={() => applyPreset("gaming")}
            >
              🎮 PC Gaming (16 Go RAM, 8 Go VRAM)
            </button>
            <button
              type="button"
              className={`locaryn-chip${preset === "mac" ? " locaryn-chip-on" : ""}`}
              onClick={() => applyPreset("mac")}
            >
              🍎 Mac M-Series (32 Go RAM Unifiée)
            </button>
            <button
              type="button"
              className={`locaryn-chip${preset === "server" ? " locaryn-chip-on" : ""}`}
              onClick={() => applyPreset("server")}
            >
              <Icon name="speed" size={15} /> Station / Serveur (64 Go RAM, 24 Go VRAM)
            </button>
          </div>
        </div>

        {/* Component Customization Controls */}
        <div
          style={{
            background: "var(--bg)",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius-sm)",
            padding: "16px",
            marginBottom: "20px",
          }}
        >
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              marginBottom: "12px",
            }}
          >
            <span style={{ fontWeight: 700, fontSize: "var(--text-sm)" }}>
              <Icon name="settings" size={15} /> Composants Matériels de votre Machine :
            </span>
            <button
              type="button"
              className="locaryn-btn-ghost"
              style={{ fontSize: "12px" }}
              onClick={runScanSimulation}
              disabled={isScanning}
            >
              {isScanning ? "Calcul..." : "🔄 Recalculer le Diagnostic"}
            </button>
          </div>

          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: "12px" }}>
            {/* RAM Input */}
            <div className="locaryn-box-card" style={{ padding: "12px" }}>
              <label className="locaryn-stat-label" htmlFor="ram-input">
                Mémoire RAM Système
              </label>
              <select
                id="ram-input"
                className="locaryn-select"
                style={{ width: "100%", marginTop: "6px", fontSize: "14px", fontWeight: 700 }}
                value={ramGb}
                onChange={(e) => {
                  setRamGb(Number(e.target.value));
                  setPreset("custom");
                }}
              >
                {Array.from(new Set([4, 8, 12, 16, 24, 32, 48, 64, 128, ramGb]))
                  .sort((a, b) => a - b)
                  .map((val) => (
                    <option key={`ram-${val}`} value={val}>
                      {val} Go RAM
                    </option>
                  ))}
              </select>
            </div>

            {/* VRAM Input */}
            <div className="locaryn-box-card" style={{ padding: "12px" }}>
              <label className="locaryn-stat-label" htmlFor="vram-input">
                VRAM Carte Graphique
              </label>
              <select
                id="vram-input"
                className="locaryn-select"
                style={{
                  width: "100%",
                  marginTop: "6px",
                  fontSize: "14px",
                  fontWeight: 700,
                  color: "var(--accent)",
                }}
                value={vramGb}
                onChange={(e) => {
                  setVramGb(Number(e.target.value));
                  setPreset("custom");
                }}
              >
                <option value={0}>0 Go (CPU Seul)</option>
                {Array.from(new Set([2, 4, 6, 8, 10, 12, 16, 20, 24, vramGb]))
                  .filter((v) => v > 0)
                  .sort((a, b) => a - b)
                  .map((val) => (
                    <option key={`vram-${val}`} value={val}>
                      {val} Go VRAM
                    </option>
                  ))}
              </select>
            </div>

            {/* CPU Cores Input */}
            <div className="locaryn-box-card" style={{ padding: "12px" }}>
              <label className="locaryn-stat-label" htmlFor="cpu-input">
                Cœurs CPU
              </label>
              <select
                id="cpu-input"
                className="locaryn-select"
                style={{ width: "100%", marginTop: "6px", fontSize: "14px", fontWeight: 700 }}
                value={cpuCores}
                onChange={(e) => {
                  setCpuCores(Number(e.target.value));
                  setPreset("custom");
                }}
              >
                {Array.from(new Set([2, 4, 6, 8, 10, 12, 14, 16, 24, 32, 64, cpuCores]))
                  .sort((a, b) => a - b)
                  .map((val) => (
                    <option key={`cpu-${val}`} value={val}>
                      {val} Cœurs CPU
                    </option>
                  ))}
              </select>
            </div>
          </div>
        </div>

        {/* Compatibility Diagnostic Breakdown */}
        <div style={{ marginBottom: "20px" }}>
          <h4 style={{ margin: "0 0 10px 0", fontSize: "var(--text-sm)" }}>
            <Icon name="target" size={15} /> Diagnostic d'Exécution avec cette Configuration :
          </h4>

          <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
            <div
              className="locaryn-box-variant-row"
              style={{
                background: "rgba(100, 200, 120, 0.1)",
                border: "1px solid rgba(100, 200, 120, 0.3)",
              }}
            >
              <div>
                <span style={{ fontWeight: 700, color: "#64c878" }}>
                  🟢 {optimalCount} Modèles Optimaux (Recommandés)
                </span>
                <span
                  style={{ fontSize: "var(--text-xs)", color: "var(--text-dim)", display: "block" }}
                >
                  Taille jusqu'à {vramGb > 0 ? vramGb : Math.round(ramGb * 0.45)} Go. Exécution GPU
                  ultra-rapide.
                </span>
              </div>
              <span
                className="locaryn-tag"
                style={{ background: "#64c878", color: "#000", fontWeight: 700 }}
              >
                Fluidité Maximale
              </span>
            </div>

            <div
              className="locaryn-box-variant-row"
              style={{
                background: "rgba(220, 180, 80, 0.1)",
                border: "1px solid rgba(220, 180, 80, 0.3)",
              }}
            >
              <div>
                <span style={{ fontWeight: 700, color: "#dcb450" }}>
                  🟡 {mediumCount} Modèles Exécutables avec Shared Memory
                </span>
                <span
                  style={{ fontSize: "var(--text-xs)", color: "var(--text-dim)", display: "block" }}
                >
                  Inférence possible via partage RAM/CPU jusqu'à {Math.round(ramGb * 0.85)} Go.
                </span>
              </div>
              <span
                className="locaryn-tag"
                style={{ background: "#dcb450", color: "#000", fontWeight: 700 }}
              >
                Vitesse Modérée
              </span>
            </div>

            <div
              className="locaryn-box-variant-row"
              style={{
                background: "rgba(204, 125, 114, 0.1)",
                border: "1px solid rgba(204, 125, 114, 0.3)",
              }}
            >
              <div>
                <span style={{ fontWeight: 700, color: "var(--danger)" }}>
                  🔴 {heavyCount} Modèles Trop Lourds pour cette Config
                </span>
                <span
                  style={{ fontSize: "var(--text-xs)", color: "var(--text-dim)", display: "block" }}
                >
                  Taille supérieure à {Math.round(ramGb * 0.85)} Go. Nécessite une extension de
                  mémoire.
                </span>
              </div>
              <span
                className="locaryn-tag"
                style={{ background: "var(--danger)", color: "#fff", fontWeight: 700 }}
              >
                Non Recommandé
              </span>
            </div>
          </div>
        </div>

        <div
          className="locaryn-field-actions"
          style={{
            marginTop: "24px",
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
          }}
        >
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => {
              onApplyFilter(false);
              onClose();
            }}
          >
            Afficher Tous les Modèles
          </button>

          <button
            type="button"
            className="locaryn-btn-primary"
            style={{ background: "var(--accent)", color: "#000" }}
            onClick={() => {
              onApplyFilter(true, { total_ram_gb: ramGb, total_vram_gb: vramGb });
              onClose();
            }}
          >
            <span className="locaryn-dot locaryn-dot-ok" /> Appliquer le filtre des modèles
            recommandés ({optimalCount})
          </button>
        </div>
      </div>
    </div>
  );
}
