import { useEffect, useMemo, useState } from "react";
import { type MediaModel, type MediaResult, type PhoneExtension, api } from "../lib/core";
import { notifyMediaComplete } from "../lib/notifications";
import { DynamicPluginWidget } from "./extensions/DynamicPluginWidget";
import { type ResolvedSlotContribution, getSlotContributions } from "./extensions/SlotRegistry";

type Props = {
  onBack: () => void;
  extensions?: PhoneExtension[];
};

type Tab = "audio" | string;

/**
 * The host only provides generic Studio plumbing. The image generator is an
 * extension custom element, loaded from its manifest and backed by its MCP
 * server; Locaryn has no native image form here.
 */
export function Studio({ onBack, extensions = [] }: Props) {
  const tabs = useMemo(() => {
    const used = new Set<string>(["audio"]);
    const pluginTabs = getSlotContributions(extensions, "studio.tabs").flatMap((contribution) => {
      if (used.has(contribution.id)) return [];
      used.add(contribution.id);
      return [{ id: contribution.id, label: contribution.label || contribution.id, contribution }];
    });
    return [{ id: "audio" as Tab, label: "Voix", contribution: undefined }, ...pluginTabs];
  }, [extensions]);
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
            className={`lo-tab ${tab === candidate.id ? "lo-tab-active" : ""}`}
            onClick={() => setTab(candidate.id)}
          >
            {candidate.label}
          </button>
        ))}
      </div>
      <div className="lo-studio">
        {current?.contribution ? (
          current.contribution.type === "custom-element" ||
          current.contribution.type === "script" ? (
            <DynamicPluginWidget contribution={current.contribution} />
          ) : (
            <CustomStudioTab tabInfo={current.contribution} />
          )
        ) : (
          <AudioGen />
        )}
      </div>
    </div>
  );
}

function AudioGen() {
  const [models, setModels] = useState<MediaModel[] | null>(null);
  const [model, setModel] = useState("");
  const [text, setText] = useState("");
  const [speed, setSpeed] = useState<"0.8" | "1.0" | "1.2">("1.0");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<MediaResult | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const list = await api.listMediaModels("audio");
        setModels(list);
        const ready = list.find((candidate) => candidate.ready);
        if (ready) setModel(ready.name);
      } catch (cause) {
        setError(String(cause));
      }
    })();
  }, []);

  async function generate() {
    if (!text.trim() || busy) return;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const generated = await api.generateAudio({ model, text: text.trim(), speed: Number(speed) });
      setResult(generated);
      notifyMediaComplete("audio", generated.name);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <p className="lo-sub">Les images sont fournies par l'extension image-gen et son outil MCP.</p>
      <label className="lo-label" htmlFor="au-model">
        Voix de synthèse
      </label>
      <select
        id="au-model"
        className="lo-select"
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
        Texte à prononcer
      </label>
      <textarea
        id="au-text"
        className="lo-input lo-textarea"
        placeholder="Bonjour !"
        value={text}
        onChange={(event) => setText(event.target.value)}
        rows={4}
      />
      <div className="lo-chips" style={{ margin: "4px 0" }}>
        {(["0.8", "1.0", "1.2"] as const).map((candidate) => (
          <button
            key={candidate}
            type="button"
            className={`lo-chip ${speed === candidate ? "lo-chip-active" : ""}`}
            onClick={() => setSpeed(candidate)}
          >
            {candidate === "0.8"
              ? "Lent (0.8x)"
              : candidate === "1.0"
                ? "Normal (1.0x)"
                : "Rapide (1.2x)"}
          </button>
        ))}
      </div>
      <button
        type="button"
        className="lo-btn"
        disabled={busy || !text.trim()}
        onClick={() => void generate()}
      >
        {busy ? "Synthèse en cours…" : "Générer la voix"}
      </button>
      {busy && <p className="lo-sub">Synthèse en cours sur {model}…</p>}
      {error && <p className="lo-error">{error}</p>}
      {result && (
        <audio
          className="lo-result"
          controls
          src={`data:${result.mime};base64,${result.data_base64}`}
          style={{ width: "100%" }}
        >
          <track kind="captions" />
        </audio>
      )}
    </>
  );
}

function CustomStudioTab({ tabInfo }: { tabInfo: ResolvedSlotContribution }) {
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [output, setOutput] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleRun() {
    if (!input.trim() || busy) return;
    setBusy(true);
    setError(null);
    setOutput(null);
    try {
      setOutput(await api.runComposerTool(tabInfo.value || tabInfo.id, input.trim()));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <div className="lo-card" style={{ flexDirection: "column", alignItems: "stretch" }}>
        <span className="lo-card-title">{tabInfo.label || tabInfo.id}</span>
        <span className="lo-hint">Apporté par {tabInfo.extensionName}</span>
      </div>
      <label className="lo-label" htmlFor={`extension-input-${tabInfo.id}`}>
        Consigne
      </label>
      <textarea
        id={`extension-input-${tabInfo.id}`}
        className="lo-input lo-textarea"
        value={input}
        onChange={(event) => setInput(event.target.value)}
        rows={4}
      />
      <button
        type="button"
        className="lo-btn"
        disabled={busy || !input.trim()}
        onClick={() => void handleRun()}
      >
        {busy ? "Traitement en cours…" : `Exécuter ${tabInfo.label || tabInfo.id}`}
      </button>
      {error && <p className="lo-error">{error}</p>}
      {output && <pre className="lo-code-block">{output}</pre>}
    </div>
  );
}
