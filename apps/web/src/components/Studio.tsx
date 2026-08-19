import { useEffect, useMemo, useState } from "react";
import { type MediaResult, type PhoneExtension, api } from "../lib/core";
import {
  PluginWidget,
  type WebPluginContribution,
  getWebStudioContributions,
} from "./PluginWidget";

type Props = {
  onBack: () => void;
  extensions?: PhoneExtension[];
};

type Tab = "audio" | string;

/**
 * The host only supplies generic Studio plumbing. Image generation is owned by
 * plugin-image-gen and arrives here as a custom element declared by the
 * extension manifest; there is no native image endpoint or image form here.
 */
export function Studio({ onBack, extensions = [] }: Props) {
  const pluginTabs = useMemo(() => getWebStudioContributions(extensions), [extensions]);
  const tabs = useMemo(
    () => [
      { id: "audio" as Tab, label: "Voix", contribution: undefined },
      ...pluginTabs.map((contribution) => ({
        id: contribution.id,
        label: contribution.label || contribution.id,
        contribution,
      })),
    ],
    [pluginTabs],
  );
  const [tab, setTab] = useState<Tab>(tabs[0]?.id ?? "audio");

  useEffect(() => {
    if (!tabs.some((candidate) => candidate.id === tab)) setTab(tabs[0]?.id ?? "audio");
  }, [tab, tabs]);

  const current = tabs.find((candidate) => candidate.id === tab);

  return (
    <div className="lo-screen">
      <div className="lo-bar">
        <button type="button" className="lo-back" onClick={onBack}>
          ← Chat
        </button>
        <span>Studio</span>
      </div>

      <div className="lo-tabs">
        {tabs.map((candidate) => (
          <button
            key={candidate.id}
            type="button"
            className={`lo-tab${tab === candidate.id ? " lo-tab-on" : ""}`}
            onClick={() => setTab(candidate.id)}
          >
            {candidate.label}
          </button>
        ))}
      </div>

      <div className="lo-studio">
        {current?.contribution ? (
          <PluginWidget contribution={current.contribution as WebPluginContribution} />
        ) : (
          <AudioGen />
        )}
      </div>
    </div>
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
      .then((list) => {
        if (cancelled) return;
        setModels(Array.isArray(list) ? list : []);
        if (list.length && !model) setModel(list[0].name);
      })
      .catch((cause) => !cancelled && setError(String(cause)));
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
      setResult(
        await api.generateAudio({
          model,
          text: text.trim(),
          speed: Number(speed),
        }),
      );
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <p className="lo-sub">
        Les extensions ajoutent leurs propres outils. La voix ci-dessous est le module générique
        conservé par Locaryn.
      </p>
      <label className="lo-label" htmlFor="au-model">
        Voix
      </label>
      <select
        id="au-model"
        className="lo-input"
        value={model}
        onChange={(event) => setModel(event.target.value)}
        disabled={busy}
      >
        {models?.map((candidate) => (
          <option key={candidate.name} value={candidate.name}>
            {candidate.name}
          </option>
        ))}
      </select>

      <label className="lo-label" htmlFor="au-text">
        Texte à dire
      </label>
      <textarea
        id="au-text"
        className="lo-input lo-textarea"
        placeholder="Bonjour !"
        value={text}
        onChange={(event) => setText(event.target.value)}
        rows={4}
      />

      <div className="lo-row">
        {(["0.8", "1.0", "1.2"] as const).map((candidate) => (
          <button
            key={candidate}
            type="button"
            className={`lo-chip${speed === candidate ? " lo-chip-on" : ""}`}
            onClick={() => setSpeed(candidate)}
          >
            {candidate === "0.8" ? "Lent" : candidate === "1.0" ? "Normal" : "Rapide"}
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
