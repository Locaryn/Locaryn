import { useEffect, useState } from "react";
import { type MediaResult, api } from "../lib/core";

type Props = {
  onBack: () => void;
};

type Tab = "image" | "audio";

/**
 * Creation studio — image and speech, generated on the machine at the other
 * end. The page holds a prompt and a model picker; the pixels and the
 * waveforms are made where the weights live, and come back as base64.
 */
export function Studio({ onBack }: Props) {
  const [tab, setTab] = useState<Tab>("image");
  return (
    <div className="lo-screen">
      <div className="lo-bar">
        <button type="button" className="lo-back" onClick={onBack}>
          ← Chat
        </button>
        <span>Créer</span>
      </div>

      <div className="lo-tabs">
        <button
          type="button"
          className={`lo-tab${tab === "image" ? " lo-tab-on" : ""}`}
          onClick={() => setTab("image")}
        >
          Image
        </button>
        <button
          type="button"
          className={`lo-tab${tab === "audio" ? " lo-tab-on" : ""}`}
          onClick={() => setTab("audio")}
        >
          Voix
        </button>
      </div>

      <div className="lo-studio">{tab === "image" ? <ImageGen /> : <AudioGen />}</div>
    </div>
  );
}

function ImageGen() {
  const [models, setModels] = useState<{ name: string }[] | null>(null);
  const [model, setModel] = useState("");
  const [prompt, setPrompt] = useState("");
  const [size, setSize] = useState<"512" | "1024">("1024");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<MediaResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .listMediaModels("image")
      .then((m) => {
        if (cancelled) return;
        const list = Array.isArray(m) ? m : [];
        setModels(list);
        if (list.length && !model) setModel(list[0].name);
      })
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [model]);

  async function generate() {
    if (!prompt.trim() || busy) return;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const w = size === "512" ? 512 : 1024;
      const r = await api.generateImage({
        model,
        prompt: prompt.trim(),
        width: w,
        height: w,
      });
      setResult(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <label className="lo-label" htmlFor="img-model">
        Modèle
      </label>
      <select
        id="img-model"
        className="lo-input"
        value={model}
        onChange={(e) => setModel(e.target.value)}
        disabled={busy}
      >
        {models?.map((m) => (
          <option key={m.name} value={m.name}>
            {m.name}
          </option>
        ))}
      </select>

      <label className="lo-label" htmlFor="img-prompt">
        Description
      </label>
      <textarea
        id="img-prompt"
        className="lo-input lo-textarea"
        placeholder="Un phare sur une falaise, au coucher du soleil…"
        value={prompt}
        onChange={(e) => setPrompt(e.target.value)}
        rows={4}
      />

      <div className="lo-row">
        {(["512", "1024"] as const).map((s) => (
          <button
            key={s}
            type="button"
            className={`lo-chip${size === s ? " lo-chip-on" : ""}`}
            onClick={() => setSize(s)}
          >
            {s} × {s}
          </button>
        ))}
      </div>

      <button type="button" className="lo-btn" disabled={busy || !prompt.trim()} onClick={generate}>
        {busy ? "Génération…" : "Générer l'image"}
      </button>

      {busy && (
        <p className="lo-sub">
          Le modèle tourne sur {model} — comptez quelques dizaines de secondes.
        </p>
      )}
      {error && <p className="lo-error">{error}</p>}
      {result && (
        <img
          className="lo-result"
          src={`data:${result.mime};base64,${result.data_base64}`}
          alt={prompt}
        />
      )}
    </>
  );
}

function AudioGen() {
  const [models, setModels] = useState<{ name: string }[] | null>(null);
  const [model, setModel] = useState("");
  const [text, setText] = useState("");
  const [speed, setSpeed] = useState("1.0");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<MediaResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .listMediaModels("audio")
      .then((m) => {
        if (cancelled) return;
        const list = Array.isArray(m) ? m : [];
        setModels(list);
        if (list.length && !model) setModel(list[0].name);
      })
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [model]);

  async function generate() {
    if (!text.trim() || busy) return;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const r = await api.generateAudio({
        model,
        text: text.trim(),
        speed: Number(speed),
      });
      setResult(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <label className="lo-label" htmlFor="au-model">
        Voix
      </label>
      <select
        id="au-model"
        className="lo-input"
        value={model}
        onChange={(e) => setModel(e.target.value)}
        disabled={busy}
      >
        {models?.map((m) => (
          <option key={m.name} value={m.name}>
            {m.name}
          </option>
        ))}
      </select>

      <label className="lo-label" htmlFor="au-text">
        Texte à dire
      </label>
      <textarea
        id="au-text"
        className="lo-input lo-textarea"
        placeholder="Bonjour ! Je parle avec la voix d'un modèle qui tourne sur votre machine."
        value={text}
        onChange={(e) => setText(e.target.value)}
        rows={4}
      />

      <div className="lo-row">
        {(["0.8", "1.0", "1.2"] as const).map((s) => (
          <button
            key={s}
            type="button"
            className={`lo-chip${speed === s ? " lo-chip-on" : ""}`}
            onClick={() => setSpeed(s)}
          >
            {s === "0.8" ? "Lent" : s === "1.0" ? "Normal" : "Rapide"}
          </button>
        ))}
      </div>

      <button type="button" className="lo-btn" disabled={busy || !text.trim()} onClick={generate}>
        {busy ? "Synthèse…" : "Générer la voix"}
      </button>

      {busy && <p className="lo-sub">Synthèse en cours sur {model}…</p>}
      {error && <p className="lo-error">{error}</p>}
      {result && (
        <audio
          className="lo-result"
          controls
          src={`data:${result.mime};base64,${result.data_base64}`}
        >
          <track kind="captions" />
        </audio>
      )}
    </>
  );
}
