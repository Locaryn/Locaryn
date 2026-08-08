import { useEffect, useState } from "react";
import { core, type RegionEditResult } from "../lib/core";
import { pickImageFile } from "../lib/dialog";
import { toImageUrl } from "../lib/imageJobs";
import { looksLikeImageModel } from "../lib/modelRegistry";

type Props = { installedModels: string[] };

/** Colours offered as swatches; any hex can still be typed. */
const SWATCHES = [
  { hex: "#633E26", name: "Marron" },
  { hex: "#1C2A5C", name: "Bleu marine" },
  { hex: "#8C1E20", name: "Rouge" },
  { hex: "#24512F", name: "Vert forêt" },
  { hex: "#222224", name: "Noir" },
  { hex: "#A88224", name: "Moutarde" },
  { hex: "#E8E4DC", name: "Écru" },
];

/**
 * Change one named part of an image and nothing else.
 *
 * The preview step is the point of this screen. The region is chosen by a
 * language model from a plain description, and a description that reads
 * perfectly well to a human can still select the wrong thing — seeing the
 * selection tinted, before anything is modified, is the only reliable check.
 */
export function RegionEditPanel({ installedModels }: Props) {
  const [image, setImage] = useState<string | null>(null);
  const [target, setTarget] = useState("");
  const [mode, setMode] = useState<"recolor" | "replace">("recolor");
  const [colour, setColour] = useState(SWATCHES[0].hex);
  const [prompt, setPrompt] = useState("");
  const [model, setModel] = useState("");
  const [busy, setBusy] = useState<null | string>(null);
  const [progress, setProgress] = useState(0);
  const [preview, setPreview] = useState<RegionEditResult | null>(null);
  const [result, setResult] = useState<RegionEditResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [outputDir, setOutputDir] = useState("");

  const imageModels = installedModels.filter(looksLikeImageModel);

  useEffect(() => {
    core.appInfo()
      .then((i) => setOutputDir(`${i.data_dir}/edited_images`))
      .catch(() => setOutputDir(""));
  }, []);
  useEffect(() => {
    if (!model && imageModels.length > 0) setModel(imageModels[0]);
  }, [imageModels, model]);

  // A new description invalidates the selection that was shown for the old one.
  useEffect(() => {
    setPreview(null);
  }, [target, image]);

  async function run(kind: "preview" | "recolor" | "replace") {
    if (!image) {
      setError("Choisissez une image.");
      return;
    }
    if (!target.trim()) {
      setError("Décrivez la zone à modifier, par exemple « le t-shirt ».");
      return;
    }
    setError(null);
    setBusy(kind);
    setProgress(0);
    try {
      const res = await core.editRegion(
        {
          image,
          target: target.trim(),
          mode: kind,
          color: kind === "recolor" ? colour : undefined,
          prompt: kind === "replace" ? prompt.trim() : undefined,
          model: kind === "replace" ? model : undefined,
          outputDir,
        },
        (pct) => setProgress(pct),
      );
      if (kind === "preview") {
        setPreview(res);
        setResult(null);
      } else {
        setResult(res);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  const scattered = preview && preview.pieces >= 5 && preview.largest < 0.45;

  return (
    <div className="lochor-region">
      <div className="lochor-region-col">
        <label className="lochor-field-label">Image</label>
        <div className="lochor-field-actions" style={{ marginTop: 8 }}>
          <button
            type="button"
            className="lochor-btn-ghost"
            disabled={!!busy}
            onClick={async () => {
              const p = await pickImageFile();
              if (p) {
                setImage(p);
                setResult(null);
              }
            }}
          >
            {image ? "Changer d'image" : "Choisir une image"}
          </button>
          {image && <span className="lochor-region-path">{image.split(/[/\\]/).pop()}</span>}
        </div>

        <label className="lochor-field-label" style={{ marginTop: 20 }}>
          Zone à modifier
        </label>
        <p className="lochor-field-hint">
          Décrivez-la en mots simples : « le t-shirt », « l'étagère en bois ». Visez un
          seul élément — une description qui en désigne plusieurs sélectionne mal.
        </p>
        <input
          className="lochor-input"
          style={{ marginTop: 8 }}
          placeholder="le t-shirt"
          value={target}
          disabled={!!busy}
          onChange={(e) => setTarget(e.target.value)}
        />
        <button
          type="button"
          className="lochor-btn-ghost"
          style={{ marginTop: 10 }}
          disabled={!!busy || !image || !target.trim()}
          onClick={() => run("preview")}
        >
          {busy === "preview" ? "Analyse…" : "Voir la sélection"}
        </button>

        <label className="lochor-field-label" style={{ marginTop: 24 }}>
          Que faire de cette zone
        </label>
        <div className="lochor-imgset-row" style={{ marginTop: 8 }}>
          <button
            type="button"
            className={`perf-ctx-btn${mode === "recolor" ? " perf-ctx-btn-active" : ""}`}
            onClick={() => setMode("recolor")}
          >
            Changer la couleur
          </button>
          <button
            type="button"
            className={`perf-ctx-btn${mode === "replace" ? " perf-ctx-btn-active" : ""}`}
            onClick={() => setMode("replace")}
          >
            Remplacer
          </button>
        </div>

        {mode === "recolor" ? (
          <>
            <p className="lochor-field-hint" style={{ marginTop: 10 }}>
              La matière, les plis et les impressions sont conservés : seule la teinte
              change, et elle correspond exactement à la couleur choisie.
            </p>
            <div className="lochor-region-swatches">
              {SWATCHES.map((s) => (
                <button
                  key={s.hex}
                  type="button"
                  title={s.name}
                  aria-label={s.name}
                  className={`lochor-region-swatch${colour === s.hex ? " lochor-active" : ""}`}
                  style={{ background: s.hex }}
                  onClick={() => setColour(s.hex)}
                />
              ))}
              <input
                className="lochor-input lochor-region-hex"
                value={colour}
                onChange={(e) => setColour(e.target.value)}
                spellCheck={false}
              />
            </div>
          </>
        ) : (
          <>
            <p className="lochor-field-hint" style={{ marginTop: 10 }}>
              La zone est redessinée par le modèle. Seule la découpe autour d'elle lui
              est envoyée, donc le détail ne dépend pas de la taille de la photo.
            </p>
            <input
              className="lochor-input"
              style={{ marginTop: 8 }}
              placeholder="un pull en laine torsadée"
              value={prompt}
              disabled={!!busy}
              onChange={(e) => setPrompt(e.target.value)}
            />
            <select
              className="lochor-input"
              style={{ marginTop: 8 }}
              value={model}
              disabled={!!busy}
              onChange={(e) => setModel(e.target.value)}
            >
              {imageModels.length === 0 && <option value="">Aucun modèle d'image installé</option>}
              {imageModels.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          </>
        )}

        <button
          type="button"
          className="lochor-btn-primary"
          style={{ marginTop: 16 }}
          disabled={
            !!busy || !image || !target.trim() ||
            (mode === "replace" && (!prompt.trim() || !model))
          }
          onClick={() => run(mode)}
        >
          {busy && busy !== "preview" ? `Traitement… ${progress}%` : "Appliquer"}
        </button>

        {error && <div className="lochor-vp-error">{error}</div>}
      </div>

      <div className="lochor-region-col">
        {preview && (
          <>
            <label className="lochor-field-label">Sélection</label>
            <img className="lochor-region-img" src={toImageUrl(preview.path)} alt="sélection" />
            <div className="lochor-kv-list" style={{ marginTop: 8 }}>
              <div className="lochor-kv">
                <span className="lochor-kv-key">Part de l'image</span>
                <span className="lochor-kv-val lochor-kv-mono">
                  {preview.coverage.toFixed(1)} %
                </span>
              </div>
              <div className="lochor-kv">
                <span className="lochor-kv-key">Morceaux</span>
                <span className="lochor-kv-val lochor-kv-mono">{preview.pieces}</span>
              </div>
            </div>
            {scattered ? (
              <p className="lochor-vp-warn">
                La sélection est éclatée en {preview.pieces} morceaux : la description
                désigne probablement plusieurs objets. Reformulez en visant un seul
                élément, sinon l'application sera refusée.
              </p>
            ) : (
              <p className="lochor-field-hint">
                Sélection cohérente. Si ce n'est pas la bonne zone, reformulez avant
                d'appliquer.
              </p>
            )}
          </>
        )}
        {result && (
          <>
            <label className="lochor-field-label" style={{ marginTop: preview ? 20 : 0 }}>
              Résultat
            </label>
            <img className="lochor-region-img" src={toImageUrl(result.path)} alt="résultat" />
            <p className="lochor-field-hint">{result.path}</p>
          </>
        )}
        {!preview && !result && (
          <div className="lochor-run-empty">
            <div className="lochor-run-empty-title">Aperçu</div>
            <div className="lochor-run-empty-sub">
              Choisissez une image, décrivez la zone, puis « Voir la sélection » pour
              vérifier ce qui sera modifié avant de lancer quoi que ce soit.
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
