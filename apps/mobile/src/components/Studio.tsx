import { Icon } from "@locaryn/ui-core";
import { useEffect, useMemo, useState } from "react";
import { type MediaModel, type MediaResult, type PhoneExtension, api } from "../lib/core";
import { notifyMediaComplete } from "../lib/notifications";
import { getSlotContributions } from "./extensions/SlotRegistry";

type Props = {
  onBack: () => void;
  /** Extensions actives : leurs `studio_tabs` s'ajoutent aux onglets, sans
   *  jamais recouvrir un onglet natif. */
  extensions?: PhoneExtension[];
};

type Tab = "image" | "audio" | (string & {});

/**
 * Creation studio — image and speech, generated on the machine at the other
 * end. The phone holds a prompt and a model picker; the pixels and the
 * waveforms are made where the weights live, and come back as base64.
 */
export function Studio({ onBack, extensions = [] }: Props) {
  const [tab, setTab] = useState<Tab>("image");

  // Le socle d'abord : les onglets natifs. Puis ceux que les extensions
  // actives déclarent, sans jamais recouvrir un id natif.
  const onglets = useMemo(() => {
    const natifs: { id: Tab; label: string; icon?: string | null }[] = [
      { id: "image", label: "Image" },
      { id: "audio", label: "Voix" },
    ];
    const pris = new Set<string>(natifs.map((t) => t.id));

    // Slots studio.tabs
    const slotTabs = getSlotContributions(extensions, "studio.tabs");
    const depuisSlots = slotTabs.flatMap((slot) => {
      if (pris.has(slot.id)) return [];
      pris.add(slot.id);
      return [
        {
          id: slot.id as Tab,
          label: slot.label || slot.id,
          icon: slot.icon,
          source: slot.extensionName,
          tool: slot.value || slot.id,
        },
      ];
    });

    const depuisExtensions = extensions.flatMap((ext) =>
      (ext.ui?.studio_tabs ?? []).flatMap((t) => {
        if (pris.has(t.id)) return [];
        pris.add(t.id);
        return [
          {
            id: t.id as Tab,
            label: t.label,
            icon: t.icon,
            source: ext.display_name || ext.name,
            tool: t.id,
          },
        ];
      }),
    );

    return [...natifs, ...depuisSlots, ...depuisExtensions] as {
      id: Tab;
      label: string;
      icon?: string | null;
      source?: string;
      tool?: string;
    }[];
  }, [extensions]);

  const ongletCourant = onglets.find((t) => t.id === tab);

  return (
    <div className="lo-screen">
      <div className="lo-bar">
        <button type="button" className="lo-back" onClick={onBack}>
          ← Chat
        </button>
        <span>Créer</span>
      </div>

      <div className="lo-tabs">
        {onglets.map((t) => (
          <button
            key={t.id}
            type="button"
            className={`lo-tab ${tab === t.id ? "lo-tab-active" : ""}`}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>

      <div className="lo-studio">
        {tab === "image" ? (
          <ImageGen />
        ) : tab === "audio" ? (
          <AudioGen />
        ) : ongletCourant ? (
          <CustomStudioTab tabInfo={ongletCourant} />
        ) : (
          <div className="lo-card">
            <p className="lo-hint">Onglet inconnu.</p>
          </div>
        )}
      </div>
    </div>
  );
}

function ImageGen() {
  const [models, setModels] = useState<MediaModel[] | null>(null);
  const [model, setModel] = useState("");
  const [prompt, setPrompt] = useState("");
  const [aspect, setAspect] = useState<"1:1" | "16:9" | "9:16">("1:1");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<MediaResult | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const list = await api.listMediaModels("image");
        setModels(list);
        const ready = list.find((m) => m.ready);
        if (ready) setModel(ready.name);
      } catch (e) {
        setError(String(e));
      }
    })();
  }, []);

  async function generate() {
    if (!prompt.trim() || busy) return;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const dimensions =
        aspect === "16:9"
          ? { width: 1024, height: 576 }
          : aspect === "9:16"
            ? { width: 576, height: 1024 }
            : { width: 1024, height: 1024 };
      const res = await api.generateImage({ model, prompt: prompt.trim(), ...dimensions });
      setResult(res);
      notifyMediaComplete("image", res.name);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function keep() {
    if (!result) return;
    try {
      const nom = await api.saveImage(result);
      setNotice(`${nom} enregistrée.`);
      window.setTimeout(() => setNotice(null), 2500);
    } catch (e) {
      setNotice(String(e));
    }
  }

  return (
    <>
      {notice && (
        <div className="lo-toast">
          <p className="lo-notice">{notice}</p>
        </div>
      )}

      <label className="lo-label" htmlFor="im-model">
        Modèle de diffusion
      </label>
      <select
        id="im-model"
        className="lo-select"
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

      <label className="lo-label" htmlFor="im-prompt">
        Description de l'image (Prompt)
      </label>
      <textarea
        id="im-prompt"
        className="lo-input lo-textarea"
        placeholder="Un paysage lumineux en aquarelle, texture de papier grain fin…"
        value={prompt}
        onChange={(e) => setPrompt(e.target.value)}
        rows={3}
      />

      <div className="lo-chips" style={{ margin: "4px 0" }}>
        {(["1:1", "16:9", "9:16"] as const).map((a) => (
          <button
            key={a}
            type="button"
            className={`lo-chip ${aspect === a ? "lo-chip-active" : ""}`}
            onClick={() => setAspect(a)}
          >
            {a === "1:1" ? "Carré (1:1)" : a === "16:9" ? "Paysage (16:9)" : "Portrait (9:16)"}
          </button>
        ))}
      </div>

      <button
        type="button"
        className="lo-btn"
        disabled={busy || !prompt.trim() || !model}
        onClick={generate}
      >
        {busy ? "Génération en cours…" : "Créer l'image"}
      </button>

      {busy && <p className="lo-sub">Calcul sur le serveur avec {model}…</p>}
      {error && <p className="lo-error">{error}</p>}
      {result && (
        <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 8 }}>
          <img
            className="lo-result"
            src={`data:${result.mime};base64,${result.data_base64}`}
            alt={prompt}
            style={{
              width: "100%",
              borderRadius: "var(--radius)",
              maxHeight: 380,
              objectFit: "contain",
            }}
          />
          <button type="button" className="lo-btn-small" onClick={keep}>
            Enregistrer dans la galerie
          </button>
        </div>
      )}
    </>
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
        const ready = list.find((m) => m.ready);
        if (ready) setModel(ready.name);
      } catch (e) {
        setError(String(e));
      }
    })();
  }, []);

  async function generate() {
    if (!text.trim() || busy) return;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const res = await api.generateAudio({
        model,
        text: text.trim(),
        speed: Number.parseFloat(speed),
      });
      setResult(res);
      notifyMediaComplete("audio", res.name);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <label className="lo-label" htmlFor="au-model">
        Voix de synthèse
      </label>
      <select
        id="au-model"
        className="lo-select"
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
        Texte à prononcer
      </label>
      <textarea
        id="au-text"
        className="lo-input lo-textarea"
        placeholder="Bonjour ! Je parle avec la voix d'un modèle qui tourne sur votre machine."
        value={text}
        onChange={(e) => setText(e.target.value)}
        rows={4}
      />

      <div className="lo-chips" style={{ margin: "4px 0" }}>
        {(["0.8", "1.0", "1.2"] as const).map((s) => (
          <button
            key={s}
            type="button"
            className={`lo-chip ${speed === s ? "lo-chip-active" : ""}`}
            onClick={() => setSpeed(s)}
          >
            {s === "0.8" ? "Lent (0.8x)" : s === "1.0" ? "Normal (1.0x)" : "Rapide (1.2x)"}
          </button>
        ))}
      </div>

      <button type="button" className="lo-btn" disabled={busy || !text.trim()} onClick={generate}>
        {busy ? "Synthèse en cours…" : "Générer la voix"}
      </button>

      {busy && <p className="lo-sub">Synthèse en cours sur {model}…</p>}
      {error && <p className="lo-error">{error}</p>}
      {result && (
        <div style={{ marginTop: 12 }}>
          <audio
            className="lo-result"
            controls
            src={`data:${result.mime};base64,${result.data_base64}`}
            style={{ width: "100%" }}
          >
            <track kind="captions" />
          </audio>
        </div>
      )}
    </>
  );
}

function CustomStudioTab({
  tabInfo,
}: {
  tabInfo: { id: string; label: string; source?: string; tool?: string };
}) {
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
      const toolName = tabInfo.tool || tabInfo.id;
      const res = await api.runComposerTool(toolName, input.trim());
      setOutput(res);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <div className="lo-card" style={{ flexDirection: "column", alignItems: "stretch" }}>
        <span className="lo-card-title">{tabInfo.label}</span>
        {tabInfo.source && <span className="lo-hint">Apporté par {tabInfo.source}</span>}
      </div>

      <div>
        <label className="lo-label">Consigne ou paramètres pour {tabInfo.label}</label>
        <textarea
          className="lo-input lo-textarea"
          placeholder={`Saisissez vos instructions pour ${tabInfo.label}…`}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          rows={4}
        />
      </div>

      <button type="button" className="lo-btn" disabled={busy || !input.trim()} onClick={handleRun}>
        {busy ? "Traitement en cours…" : `Exécuter ${tabInfo.label}`}
      </button>

      {busy && <p className="lo-sub">Traitement sur le serveur…</p>}
      {error && <p className="lo-error">{error}</p>}
      {output && (
        <div
          style={{
            padding: 12,
            background: "rgba(0, 0, 0, 0.3)",
            borderRadius: "var(--radius)",
            border: "1px solid var(--border)",
          }}
        >
          <span style={{ fontSize: 12, fontWeight: 700, color: "var(--accent)" }}>
            Résultat produit
          </span>
          <pre className="lo-code-block" style={{ marginTop: 6 }}>
            {output}
          </pre>
        </div>
      )}
    </div>
  );
}
