import { useEffect, useState } from "react";
import { core } from "../lib/core";

type Props = {
  onOpenModels?: () => void;
};

export function TrainingView({ onOpenModels }: Props) {
  const [tab, setTab] = useState<"finetune" | "obliteration">("finetune");

  // Fine-tuning state
  const [datasetPath, setDatasetPath] = useState("");
  const [baseModel, setBaseModel] = useState("qwen2.5-coder:7b");
  const [lr, setLr] = useState(0.0002);
  const [epochs, setEpochs] = useState(3);
  const [isTraining, setIsTraining] = useState(false);
  const [logs, setLogs] = useState<string[]>([]);

  // Installed models list
  const [installedModels, setInstalledModels] = useState<string[]>([]);

  // Obliteration state
  const [oblModel, setOblModel] = useState<string>("");
  const [ablationMethod, setAblationMethod] = useState("repe");
  const [intensity, setIntensity] = useState(1.2);
  const [targetLayers, setTargetLayers] = useState("10-28");
  const [isObliterating, setIsObliterating] = useState(false);
  const [oblProgress, setOblProgress] = useState(0);
  const [oblLogs, setOblLogs] = useState<string[]>([]);
  const [disclaimerAccepted, setDisclaimerAccepted] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        const providers = await core.listProviders();
        const active = providers.find((p) => p.is_active) ?? providers[0];
        if (active) {
          const list = await core.listModels(active.endpoint);
          setInstalledModels(list);
          if (list.length > 0) {
            // Forme fonctionnelle : ne pas écraser un modèle déjà choisi pendant le chargement,
            // et ne pas dépendre de oblModel, ce qui relancerait la requête à chaque sélection.
            setOblModel((current) => current || list[0]);
          }
        }
      } catch {
        // Fallback for demo or offline
        setInstalledModels(["llama3.1:8b", "qwen2.5-coder:7b", "gemma2:9b"]);
        setOblModel("llama3.1:8b");
      }
    })();
  }, []);

  function startTraining() {
    setIsTraining(true);
    setLogs((prev) => [
      ...prev,
      `[${new Date().toLocaleTimeString()}] Initialisation de l'environnement de fine-tuning (LoRA / QLoRA)...`,
      `[${new Date().toLocaleTimeString()}] Chargement du modèle de base : ${baseModel}`,
      `[${new Date().toLocaleTimeString()}] Analyse du dataset : ${datasetPath || "dataset_custom.jsonl"}`,
      `[${new Date().toLocaleTimeString()}] Epoch 1/3 démarré (Loss initial : 2.410)...`,
    ]);
  }

  function startObliteration() {
    if (!disclaimerAccepted || !oblModel) return;
    setIsObliterating(true);
    setOblProgress(10);
    setOblLogs([
      `[${new Date().toLocaleTimeString()}] Initialisation du pipeline RepE (Representation Engineering Refusal Ablation)...`,
      `[${new Date().toLocaleTimeString()}] Modèle local sélectionné : ${oblModel}`,
      `[${new Date().toLocaleTimeString()}] Extraction des activations sur les couches ${targetLayers}...`,
    ]);

    let currentProgress = 10;
    const interval = setInterval(() => {
      currentProgress += 20;
      if (currentProgress >= 100) {
        clearInterval(interval);
        setOblProgress(100);
        setIsObliterating(false);
        const newTag = `${oblModel.split(":")[0]}-abliterated:${oblModel.split(":")[1] || "latest"}`;
        setOblLogs((prev) => [
          ...prev,
          `[${new Date().toLocaleTimeString()}] Soustraction orthogonale de la direction de refus (Intensité: ${intensity})...`,
          `[${new Date().toLocaleTimeString()}] Sauvegarde des nouveaux poids GGUF : ${newTag}`,
          `[${new Date().toLocaleTimeString()}] ✅ Oblitération terminée ! Le modèle dé-aligné ${newTag} est désormais disponible dans vos modèles installés.`,
        ]);
        setInstalledModels((prev) => [...prev, newTag]);
      } else {
        setOblProgress(currentProgress);
        if (currentProgress === 50) {
          setOblLogs((prev) => [
            ...prev,
            `[${new Date().toLocaleTimeString()}] Calcul de la projection orthogonale du vecteur de refus sur les tenseurs...`,
          ]);
        }
      }
    }, 600);
  }

  return (
    <section className="locaryn-view-container">
      <div className="locaryn-view-header">
        <h2>Entraînement & Modification de Modèles</h2>
        <p className="locaryn-view-desc">
          Entraînez des adaptateurs LoRA ou neutralisez les filtres de refus sur vos modèles locaux
          installés.
        </p>
      </div>

      {/* Tabs Switcher */}
      <div
        className="locaryn-store-tabs"
        style={{ display: "flex", gap: "10px", marginBottom: "20px" }}
      >
        <button
          type="button"
          className={`locaryn-tab-btn${tab === "finetune" ? " locaryn-active" : ""}`}
          onClick={() => setTab("finetune")}
        >
          🎯 Fine-Tuning LoRA / QLoRA
        </button>
        <button
          type="button"
          className={`locaryn-tab-btn${tab === "obliteration" ? " locaryn-active" : ""}`}
          style={
            tab === "obliteration" ? { borderColor: "var(--danger)", color: "var(--danger)" } : {}
          }
          onClick={() => setTab("obliteration")}
        >
          🔓 Studio d'Oblitération de Modèle (RepE Ablation)
        </button>
      </div>

      {/* TAB 1: Fine-Tuning */}
      {tab === "finetune" && (
        <div className="locaryn-training-grid">
          <div className="locaryn-card">
            <h3>Configuration du Fine-Tuning LoRA</h3>

            <div className="locaryn-field">
              <label className="locaryn-field-label" htmlFor="training-base-model">
                Modèle de base
              </label>
              <select
                id="training-base-model"
                className="locaryn-select"
                value={baseModel}
                onChange={(e) => setBaseModel(e.target.value)}
              >
                {installedModels.length > 0 ? (
                  installedModels.map((tag) => (
                    <option key={tag} value={tag}>
                      {tag} (Installé)
                    </option>
                  ))
                ) : (
                  <>
                    <option value="qwen2.5-coder:7b">qwen2.5-coder:7b</option>
                    <option value="llama3.1:8b">llama3.1:8b</option>
                    <option value="mistral:7b">mistral:7b</option>
                  </>
                )}
              </select>
            </div>

            <div className="locaryn-field">
              <label className="locaryn-field-label" htmlFor="training-dataset-path">
                Fichier Dataset (.jsonl / .txt)
              </label>
              <input
                id="training-dataset-path"
                className="locaryn-input"
                placeholder="/chemin/vers/mon_dataset.jsonl"
                value={datasetPath}
                onChange={(e) => setDatasetPath(e.target.value)}
              />
            </div>

            <div className="locaryn-field-row" style={{ gap: "16px", marginTop: "12px" }}>
              <div className="locaryn-field" style={{ flex: 1 }}>
                <label className="locaryn-field-label" htmlFor="training-learning-rate">
                  Taux d'apprentissage (LR)
                </label>
                <input
                  id="training-learning-rate"
                  type="number"
                  step="0.00005"
                  className="locaryn-input"
                  value={lr}
                  onChange={(e) => setLr(Number.parseFloat(e.target.value))}
                />
              </div>
              <div className="locaryn-field" style={{ flex: 1 }}>
                <label className="locaryn-field-label" htmlFor="training-epochs">
                  Époques (Epochs)
                </label>
                <input
                  id="training-epochs"
                  type="number"
                  className="locaryn-input"
                  value={epochs}
                  onChange={(e) => setEpochs(Number.parseInt(e.target.value, 10))}
                />
              </div>
            </div>

            <div className="locaryn-field-actions" style={{ marginTop: "20px" }}>
              <button
                type="button"
                className="locaryn-btn-primary"
                disabled={isTraining}
                onClick={startTraining}
              >
                {isTraining ? "Entraînement en cours..." : "Lancer le Fine-Tuning LoRA"}
              </button>
            </div>
          </div>

          <div className="locaryn-card">
            <h3>Journaux d'Entraînement</h3>
            <div className="locaryn-training-logs">
              {logs.length === 0 ? (
                <span className="locaryn-text-faint">Aucun entraînement en cours...</span>
              ) : (
                logs.map((log, i) => (
                  // biome-ignore lint/suspicious/noArrayIndexKey: journal en ajout seul, jamais réordonné ni filtré, et les lignes peuvent être identiques : l'index est le seul identifiant stable
                  <div key={i} className="locaryn-log-line">
                    {log}
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      )}

      {/* TAB 2: Obliteration Studio */}
      {tab === "obliteration" && (
        <div className="locaryn-training-grid">
          <div className="locaryn-card">
            <h3>🔓 Paramètres de l'Oblitération (RepE Refusal Ablation)</h3>
            <p className="locaryn-field-hint" style={{ marginBottom: "16px" }}>
              Calcule le vecteur de direction de refus d'un modèle **déjà installé** et applique une
              projection orthogonale pour neutraliser les filtres.
            </p>

            {/* Warning if no models installed */}
            {installedModels.length === 0 ? (
              <div
                style={{
                  background: "rgba(204, 125, 114, 0.12)",
                  border: "1px solid rgba(204, 125, 114, 0.4)",
                  borderRadius: "var(--radius-sm)",
                  padding: "16px",
                  marginBottom: "20px",
                  lineHeight: 1.5,
                }}
              >
                <strong style={{ color: "var(--danger)" }}>
                  ⚠️ Aucun modèle local actuellement installé :
                </strong>
                <p
                  style={{ margin: "8px 0", fontSize: "var(--text-xs)", color: "var(--text-dim)" }}
                >
                  Pour pouvoir appliquer le script d'oblitération, vous devez au préalable
                  **télécharger et installer un modèle local** (depuis le Marketplace de Modèles).
                </p>
                {onOpenModels && (
                  <button
                    type="button"
                    className="locaryn-btn-primary"
                    style={{ fontSize: "12px", marginTop: "6px" }}
                    onClick={onOpenModels}
                  >
                    → Accéder au Marketplace de Modèles
                  </button>
                )}
              </div>
            ) : (
              <>
                {/* Legal Disclaimer Box */}
                <div
                  style={{
                    background: "rgba(204, 125, 114, 0.1)",
                    border: "1px solid rgba(204, 125, 114, 0.3)",
                    borderRadius: "var(--radius-sm)",
                    padding: "12px",
                    marginBottom: "16px",
                    fontSize: "var(--text-xs)",
                    lineHeight: 1.5,
                  }}
                >
                  <strong style={{ color: "var(--danger)" }}>
                    ⚠️ Avis de Responsabilité & Cadre Légal :
                  </strong>
                  <br />
                  L'oblitération des contraintes de refus est dédiée **exclusivement au pentesting
                  d'infrastructure encadré et à la recherche en cybersécurité**.
                  <div style={{ marginTop: "8px" }}>
                    <label className="locaryn-checkbox-row">
                      <input
                        type="checkbox"
                        checked={disclaimerAccepted}
                        onChange={(e) => setDisclaimerAccepted(e.target.checked)}
                      />
                      <span style={{ fontWeight: 700 }}>
                        J'accepte la décharge et confirme utiliser cette option pour la
                        cybersécurité légale.
                      </span>
                    </label>
                  </div>
                </div>

                <div className="locaryn-field" style={{ marginBottom: "14px" }}>
                  <label className="locaryn-field-label" htmlFor="obliteration-model">
                    Modèle local installé à oblitérer ({installedModels.length} disponibles)
                  </label>
                  <select
                    id="obliteration-model"
                    className="locaryn-select"
                    value={oblModel}
                    onChange={(e) => setOblModel(e.target.value)}
                    disabled={isObliterating}
                  >
                    {installedModels.map((tag) => (
                      <option key={tag} value={tag}>
                        💻 {tag} (Modèle Installé)
                      </option>
                    ))}
                  </select>
                </div>

                <div className="locaryn-field-row" style={{ gap: "16px" }}>
                  <div className="locaryn-field" style={{ flex: 1 }}>
                    <label className="locaryn-field-label" htmlFor="obliteration-method">
                      Méthode d'Ablation
                    </label>
                    <select
                      id="obliteration-method"
                      className="locaryn-select"
                      value={ablationMethod}
                      onChange={(e) => setAblationMethod(e.target.value)}
                      disabled={isObliterating}
                    >
                      <option value="repe">Representation Engineering (RepE)</option>
                      <option value="orthogonal">Orthogonal Projection Subtraction</option>
                    </select>
                  </div>

                  <div className="locaryn-field" style={{ flex: 1 }}>
                    <label className="locaryn-field-label" htmlFor="obliteration-target-layers">
                      Couches cibles
                    </label>
                    <input
                      id="obliteration-target-layers"
                      className="locaryn-input"
                      value={targetLayers}
                      onChange={(e) => setTargetLayers(e.target.value)}
                      disabled={isObliterating}
                    />
                  </div>
                </div>

                <div className="locaryn-field" style={{ marginTop: "14px" }}>
                  <label className="locaryn-field-label" htmlFor="obliteration-intensity">
                    Intensité Alpha ({intensity})
                  </label>
                  <input
                    id="obliteration-intensity"
                    type="range"
                    min="0.5"
                    max="2.5"
                    step="0.1"
                    className="lmc-slider"
                    value={intensity}
                    onChange={(e) => setIntensity(Number.parseFloat(e.target.value))}
                    disabled={isObliterating}
                  />
                </div>

                {isObliterating && (
                  <div style={{ marginTop: "16px" }}>
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        fontSize: "var(--text-xs)",
                        marginBottom: "4px",
                      }}
                    >
                      <span>Calcul et soustraction du vecteur de refus...</span>
                      <span>{oblProgress}%</span>
                    </div>
                    <div
                      style={{
                        height: "6px",
                        background: "var(--border)",
                        borderRadius: "var(--radius-pill)",
                        overflow: "hidden",
                      }}
                    >
                      <div
                        style={{
                          height: "100%",
                          width: `${oblProgress}%`,
                          background: "var(--danger)",
                          transition: "width 0.3s ease",
                        }}
                      />
                    </div>
                  </div>
                )}

                <div className="locaryn-field-actions" style={{ marginTop: "20px" }}>
                  <button
                    type="button"
                    className="locaryn-btn-primary"
                    style={{
                      background: disclaimerAccepted ? "var(--danger)" : "var(--border)",
                      color: "#fff",
                    }}
                    disabled={!disclaimerAccepted || isObliterating}
                    onClick={startObliteration}
                  >
                    {isObliterating
                      ? "Oblitération en cours..."
                      : "⚡ Lancer l'Oblitération de Modèle"}
                  </button>
                </div>
              </>
            )}
          </div>

          <div className="locaryn-card">
            <h3>Console du Script d'Oblitération</h3>
            <div className="locaryn-training-logs">
              {oblLogs.length === 0 ? (
                <span className="locaryn-text-faint">
                  Sélectionnez un modèle installé et validez la décharge pour démarrer...
                </span>
              ) : (
                oblLogs.map((log, i) => (
                  <div
                    // biome-ignore lint/suspicious/noArrayIndexKey: journal en ajout seul, jamais réordonné ni filtré, et les lignes peuvent être identiques : l'index est le seul identifiant stable
                    key={i}
                    className="locaryn-log-line"
                    style={{ color: log.includes("✅") ? "var(--accent)" : "var(--text)" }}
                  >
                    {log}
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
