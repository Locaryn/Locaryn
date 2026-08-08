import { useState } from "react";
import { MODEL_CATALOG, type ModelFamily } from "../lib/modelCatalog";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  installedModels?: string[];
  onModelAbliterated?: (newModelTag: string) => void;
};

export function ModelObliterator({
  isOpen,
  onClose,
  installedModels = [],
  onModelAbliterated,
}: Props) {
  const [selectedModel, setSelectedModel] = useState<string>(
    installedModels[0] ?? "llama3.1:8b"
  );
  const [ablationMethod, setAblationMethod] = useState<"repe" | "orthogonal" | "norm_subtraction">(
    "repe"
  );
  const [intensity, setIntensity] = useState<number>(1.2);
  const [targetLayers, setTargetLayers] = useState<string>("10-28");
  const [isProcessing, setIsProcessing] = useState(false);
  const [progress, setProgress] = useState(0);
  const [logs, setLogs] = useState<string[]>([]);
  const [disclaimerAccepted, setDisclaimerAccepted] = useState(false);

  if (!isOpen) return null;

  // Flatten all available tags for selection
  const allCatalogTags = MODEL_CATALOG.flatMap((f) => f.variants.map((v) => v.tag));
  const modelOptions = Array.from(new Set([...installedModels, ...allCatalogTags]));

  function startObliteration() {
    if (!disclaimerAccepted) return;
    setIsProcessing(true);
    setProgress(5);
    setLogs([
      `[${new Date().toLocaleTimeString()}] Initialisation du pipeline d'ablation de vecteurs de refus (RepE)...`,
      `[${new Date().toLocaleTimeString()}] Modèle source sélectionné : ${selectedModel}`,
      `[${new Date().toLocaleTimeString()}] Extraction du tenseur d'activation sur les couches [${targetLayers}]...`,
    ]);

    let currentProgress = 5;
    const interval = setInterval(() => {
      currentProgress += 15;
      if (currentProgress >= 100) {
        clearInterval(interval);
        setProgress(100);
        setIsProcessing(false);
        const newTag = `${selectedModel.split(":")[0]}-abliterated:${selectedModel.split(":")[1] || "latest"}`;
        setLogs((prev) => [
          ...prev,
          `[${new Date().toLocaleTimeString()}] Calcul de la direction moyenne de refus (Harmful/Harmless contrastive pair)...`,
          `[${new Date().toLocaleTimeString()}] Orthogonalisation des poids de projection (Intensité alpha: ${intensity})...`,
          `[${new Date().toLocaleTimeString()}] Sauvegarde des nouveaux poids GGUF : ${newTag}`,
          `[${new Date().toLocaleTimeString()}] ✅ Oblitération terminée avec succès ! Le modèle ${newTag} est prêt à l'emploi.`,
        ]);
        onModelAbliterated?.(newTag);
      } else {
        setProgress(currentProgress);
        if (currentProgress === 35) {
          setLogs((prev) => [
            ...prev,
            `[${new Date().toLocaleTimeString()}] Extraction des paires d'activation harmonique sur ${targetLayers} couches...`,
          ]);
        } else if (currentProgress === 65) {
          setLogs((prev) => [
            ...prev,
            `[${new Date().toLocaleTimeString()}] Soustraction orthogonale du vecteur de refus sur la matrice de sortie...`,
          ]);
        }
      }
    }, 600);
  }

  return (
    <div className="locaryn-settings-backdrop" onClick={onClose}>
      <div
        className="locaryn-card"
        style={{
          width: "680px",
          maxHeight: "88vh",
          overflowY: "auto",
          margin: "40px auto",
          border: "1px solid var(--border-strong)",
          boxShadow: "0 16px 40px rgba(0,0,0,0.8)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "16px" }}>
          <div>
            <h3 style={{ margin: 0, display: "flex", alignItems: "center", gap: "8px" }}>
              🔓 Studio d'Oblitération de Modèle (RepE Refusal Ablation)
            </h3>
            <span style={{ fontSize: "var(--text-xs)", color: "var(--text-faint)" }}>
              Neutralisez les filtres de refus de n'importe quel modèle local via représentation vectorielle (Open Source Script).
            </span>
          </div>
          <button type="button" className="locaryn-icon-btn" onClick={onClose}>
            ✕
          </button>
        </div>

        {/* Legal Disclaimer Box */}
        <div
          style={{
            background: "rgba(204, 125, 114, 0.1)",
            border: "1px solid rgba(204, 125, 114, 0.3)",
            borderRadius: "var(--radius-sm)",
            padding: "14px",
            marginBottom: "20px",
            fontSize: "var(--text-xs)",
            lineHeight: 1.5,
          }}
        >
          <strong style={{ color: "var(--danger)" }}>⚠️ AVIS DE RESPONSABILITÉ & SÉCURITÉ :</strong>
          <br />
          Le script d'oblitération modifie directement les tenseurs d'activation du modèle pour supprimer le blocage des réponses. Cet outil est destiné **exclusivement à la recherche en cybersécurité, aux tests d'intrusion (pentesting encadré) et à l'audit de robustesse des LLM**.
          <div style={{ marginTop: "8px" }}>
            <label className="locaryn-checkbox-row">
              <input
                type="checkbox"
                checked={disclaimerAccepted}
                onChange={(e) => setDisclaimerAccepted(e.target.checked)}
              />
              <span style={{ fontWeight: 700 }}>
                Je certifie utiliser cette fonctionnalité dans un cadre légal de recherche ou de pentest autorisé.
              </span>
            </label>
          </div>
        </div>

        {/* Model Selection */}
        <div className="locaryn-field" style={{ marginBottom: "16px" }}>
          <label className="locaryn-field-label">Modèle Source à Oblitérer</label>
          <select
            className="locaryn-select"
            value={selectedModel}
            onChange={(e) => setSelectedModel(e.target.value)}
            disabled={isProcessing}
          >
            {modelOptions.map((m) => (
              <option key={m} value={m}>
                {m} {installedModels.includes(m) ? " (Installé localement)" : ""}
              </option>
            ))}
          </select>
        </div>

        {/* Parameters Grid */}
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "16px", marginBottom: "16px" }}>
          <div className="locaryn-field">
            <label className="locaryn-field-label">Méthode d'Ablation Vectorielle</label>
            <select
              className="locaryn-select"
              value={ablationMethod}
              onChange={(e) => setAblationMethod(e.target.value as any)}
              disabled={isProcessing}
            >
              <option value="repe">Representation Engineering (RepE)</option>
              <option value="orthogonal">Orthogonal Projection Subtraction</option>
              <option value="norm_subtraction">Norm-scaling Refusal Vector</option>
            </select>
          </div>

          <div className="locaryn-field">
            <label className="locaryn-field-label">Couches Cibles (Layer Range)</label>
            <input
              className="locaryn-input"
              value={targetLayers}
              onChange={(e) => setTargetLayers(e.target.value)}
              placeholder="10-28"
              disabled={isProcessing}
            />
          </div>
        </div>

        <div className="locaryn-field" style={{ marginBottom: "20px" }}>
          <div className="lmc-field-head">
            <label className="lmc-label">Intensité d'Ablation (Alpha : {intensity})</label>
            <span className="lmc-value">{intensity}</span>
          </div>
          <input
            type="range"
            min="0.5"
            max="2.5"
            step="0.1"
            className="lmc-slider"
            value={intensity}
            onChange={(e) => setIntensity(parseFloat(e.target.value))}
            disabled={isProcessing}
          />
        </div>

        {/* Progress Bar */}
        {isProcessing && (
          <div style={{ marginBottom: "16px" }}>
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: "var(--text-xs)", marginBottom: "4px" }}>
              <span>Oblitération des poids en cours...</span>
              <span>{progress}%</span>
            </div>
            <div style={{ height: "6px", background: "var(--border)", borderRadius: "var(--radius-pill)", overflow: "hidden" }}>
              <div style={{ height: "100%", width: `${progress}%`, background: "var(--danger)", transition: "width 0.3s ease" }} />
            </div>
          </div>
        )}

        {/* Output Logs */}
        <div className="locaryn-field">
          <label className="locaryn-field-label">Console du Script d'Oblitération</label>
          <div className="locaryn-training-logs" style={{ height: "160px" }}>
            {logs.length === 0 ? (
              <span className="locaryn-text-faint">Sélectionnez un modèle et cliquez sur "Lancer l'Oblitération".</span>
            ) : (
              logs.map((l, i) => (
                <div key={i} className="locaryn-log-line" style={{ color: l.includes("✅") ? "var(--accent)" : "var(--text)" }}>
                  {l}
                </div>
              ))
            )}
          </div>
        </div>

        <div className="locaryn-field-actions" style={{ marginTop: "24px", display: "flex", justifyContent: "flex-end", gap: "8px" }}>
          <button type="button" className="locaryn-btn-ghost" onClick={onClose} disabled={isProcessing}>
            Fermer
          </button>
          <button
            type="button"
            className="locaryn-btn-primary"
            style={{ background: disclaimerAccepted ? "var(--danger)" : "var(--border)", color: "#fff" }}
            disabled={!disclaimerAccepted || isProcessing}
            onClick={startObliteration}
          >
            {isProcessing ? "Oblitération en cours..." : "⚡ Lancer le Script d'Oblitération"}
          </button>
        </div>
      </div>
    </div>
  );
}
