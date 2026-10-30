import { Icon, type IconName } from "@locaryn/ui-core";
import { useEffect, useMemo, useRef, useState } from "react";
import { type CatalogEntry, core } from "../lib/core";
import { LANGUAGES, type Lang, useI18n } from "../lib/i18n";
import { type ModelTier, type TierPlan, loadTierPlan } from "../lib/onboardingModels";
import { NEEDS, type Suggestion, suggest, suggestForHardware } from "../lib/onboardingNeeds";
import { plainSpeed } from "../lib/plainLanguage";
import { PRESENTATION_KEY } from "../lib/presentation";
import {
  type Need,
  ONBOARDING_VERSION,
  type UiMode,
  updateProfile,
  useProfile,
} from "../lib/profile";
import { TXT, type Texts } from "./onboardingTexts";
import "../styles/onboarding.css";

type StepId = "language" | "welcome" | "level" | "models" | "needs" | "suggest" | "ready";

interface Props {
  /** Lance le téléchargement d'un modèle (étiquette du catalogue). */
  onInstallModel: (tag: string) => Promise<void>;
  /** Ouvre la fenêtre d'installation d'une extension, pré-remplie. */
  onInstallExtension: (source: string) => void;
}

/** Vrai quand la machine préfère moins de mouvement : on n'attend plus l'animation. */
function reducedMotion(): boolean {
  return (
    typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

/** Une réponse à cliquer : l'animation de sélection vit dans le CSS. */
function Choice(props: {
  selected: boolean;
  onClick: () => void;
  icon: IconName;
  title: string;
  hint?: string;
  badge?: string;
  warn?: boolean;
  children?: React.ReactNode;
  index: number;
}) {
  return (
    <button
      type="button"
      className="lo-ob-choice"
      data-selected={props.selected}
      style={{ ["--i" as string]: props.index }}
      aria-pressed={props.selected}
      onClick={props.onClick}
    >
      <span className="lo-ob-choice-icon">
        <Icon name={props.icon} size={22} />
      </span>
      <span className="lo-ob-choice-text">
        <span className="lo-ob-choice-title">{props.title}</span>
        {props.hint && <span className="lo-ob-choice-hint">{props.hint}</span>}
        {props.children}
      </span>
      {props.badge && (
        <span className="lo-ob-badge" data-warn={props.warn === true}>
          {props.badge}
        </span>
      )}
      <span className="lo-ob-tick" aria-hidden="true">
        <Icon name="check" size={14} />
      </span>
    </button>
  );
}

function LanguageStep({ tx }: { tx: Texts }) {
  const { lang, setLang } = useI18n();
  return (
    <>
      <h2 className="lo-ob-title">{tx.languageTitle}</h2>
      <p className="lo-ob-body">{tx.languageBody}</p>
      <div className="lo-ob-choices">
        {LANGUAGES.map((l, i) => (
          <Choice
            key={l.id}
            index={i}
            selected={lang === l.id}
            onClick={() => setLang(l.id as Lang)}
            icon="translate"
            title={`${l.flag}  ${l.label}`}
          />
        ))}
      </div>
    </>
  );
}

function WelcomeStep({ tx }: { tx: Texts }) {
  const icons: IconName[] = ["cpu", "lock", "plugs"];
  return (
    <>
      <h2 className="lo-ob-title">{tx.welcomeTitle}</h2>
      <p className="lo-ob-body">{tx.welcomeBody}</p>
      <ul className="lo-ob-points">
        {tx.welcomePoints.map((p, i) => (
          <li key={p.title} style={{ ["--i" as string]: i }}>
            <span className="lo-ob-point-icon">
              <Icon name={icons[i] ?? "sparkle"} size={20} />
            </span>
            <span>
              <strong>{p.title}</strong>
              <span className="lo-ob-point-text">{p.body}</span>
            </span>
          </li>
        ))}
      </ul>
    </>
  );
}

function LevelStep({
  tx,
  picked,
  onPick,
}: { tx: Texts; picked: UiMode | null; onPick: (m: UiMode) => void }) {
  return (
    <>
      <h2 className="lo-ob-title">{tx.levelTitle}</h2>
      <p className="lo-ob-body">{tx.levelBody}</p>
      <div className="lo-ob-choices">
        <Choice
          index={0}
          selected={picked === "expert"}
          onClick={() => onPick("expert")}
          icon="terminal-window"
          title={tx.levelExpert}
          hint={tx.levelExpertHint}
        />
        <Choice
          index={1}
          selected={picked === "simple"}
          onClick={() => onPick("simple")}
          icon="sparkle"
          title={tx.levelSimple}
          hint={tx.levelSimpleHint}
        />
      </div>
      <p className="lo-ob-note">{tx.levelNote}</p>
    </>
  );
}

function ModelsStep({
  tx,
  lang,
  plan,
  onPick,
}: {
  tx: Texts;
  lang: Lang;
  plan: TierPlan | null;
  onPick: (tier: ModelTier) => void;
}) {
  const [picked, setPicked] = useState<ModelTier | null>(null);
  const icons: Record<ModelTier, IconName> = {
    fast: "lightning",
    balanced: "gauge",
    smart: "brain",
  };

  const choose = (tier: ModelTier) => {
    if (picked) return;
    setPicked(tier);
    window.setTimeout(() => onPick(tier), reducedMotion() ? 0 : 650);
  };

  return (
    <>
      <h2 className="lo-ob-title">{tx.modelsTitle}</h2>
      <p className="lo-ob-body">{tx.modelsBody}</p>
      {!plan ? (
        <p className="lo-ob-note lo-ob-loading">{tx.modelsLoading}</p>
      ) : (
        <div className="lo-ob-choices">
          {plan.choices.map((m, i) => {
            const t = tx.tiers[m.tier];
            const speed = m.tokensPerSecond === null ? null : plainSpeed(m.tokensPerSecond, lang);
            return (
              <Choice
                key={m.tier}
                index={i}
                selected={picked === m.tier}
                onClick={() => choose(m.tier)}
                icon={icons[m.tier]}
                title={t.name}
                hint={`${t.hint} · ${m.storageGb.toFixed(1)} ${tx.gb}`}
                badge={
                  m.tooHeavy
                    ? tx.tooHeavy
                    : m.tier === plan.recommended
                      ? tx.recommended
                      : undefined
                }
                warn={m.tooHeavy}
              >
                {speed && <span className="lo-ob-speed">{speed}</span>}
              </Choice>
            );
          })}
        </div>
      )}
      <p className="lo-ob-note">{tx.modelsNote}</p>
    </>
  );
}

function NeedsStep(props: { tx: Texts; lang: Lang; needs: Need[]; onToggle: (n: Need) => void }) {
  const { tx, lang, needs, onToggle } = props;
  return (
    <>
      <h2 className="lo-ob-title">{tx.needsTitle}</h2>
      <p className="lo-ob-body">{tx.needsBody}</p>
      <div className="lo-ob-choices">
        {NEEDS.map((n, i) => (
          <Choice
            key={n.id}
            index={i}
            selected={needs.includes(n.id)}
            onClick={() => onToggle(n.id)}
            icon={n.icon}
            title={n[lang].label}
            hint={n[lang].hint}
          />
        ))}
      </div>
    </>
  );
}

function SuggestStep(props: {
  tx: Texts;
  items: Suggestion[];
  onInstall: (e: CatalogEntry) => void;
}) {
  const { tx, items, onInstall } = props;
  return (
    <>
      <h2 className="lo-ob-title">{tx.suggestTitle}</h2>
      <p className="lo-ob-body">{tx.suggestBody}</p>
      <div className="lo-ob-choices">
        {items.map((s, i) => (
          <div className="lo-ob-suggest" key={s.entry.id} style={{ ["--i" as string]: i }}>
            <span className="lo-ob-choice-text">
              <span className="lo-ob-choice-title" data-no-translate>
                {s.entry.display_name || s.entry.name}
              </span>
              <span className="lo-ob-choice-hint">{s.entry.description ?? ""}</span>
              {s.need === "npu" && <span className="lo-ob-reason">{tx.npuReason}</span>}
            </span>
            <button
              type="button"
              className="lo-ob-btn lo-ob-btn-primary"
              onClick={() => onInstall(s.entry)}
            >
              {tx.install}
            </button>
          </div>
        ))}
      </div>
    </>
  );
}

function ReadyStep({ tx, needs }: { tx: Texts; needs: Need[] }) {
  return (
    <>
      <div className="lo-ob-done" aria-hidden="true">
        <Icon name="check" size={32} />
      </div>
      <h2 className="lo-ob-title">{tx.readyTitle}</h2>
      <p className="lo-ob-body">{tx.readyBody}</p>
      {needs.includes("phone") && <p className="lo-ob-note">{tx.readyPhone}</p>}
      <p className="lo-ob-note">{tx.readyAdvanced}</p>
    </>
  );
}

/**
 * L'accueil du premier lancement : langue, présentation brève, niveau, puis —
 * pour qui débute — le choix d'un modèle et de ses besoins. Chaque réponse
 * change l'application (voir `lib/profile.ts`).
 */
export function Onboarding({ onInstallModel, onInstallExtension }: Props) {
  const profile = useProfile();
  const { lang } = useI18n();
  const tx = TXT[lang];
  const [index, setIndex] = useState(0);
  const [back, setBack] = useState(false);
  const [mode, setMode] = useState<UiMode | null>(null);
  const [needs, setNeeds] = useState<Need[]>([]);
  const [entries, setEntries] = useState<CatalogEntry[]>([]);
  const [plan, setPlan] = useState<TierPlan | null>(null);
  const fetched = useRef(false);

  // La machine est lue dès l'ouverture : le palier conseillé et un éventuel NPU
  // en dépendent, et la lecture prend un instant.
  useEffect(() => {
    let off = false;
    loadTierPlan().then((p) => !off && setPlan(p));
    return () => {
      off = true;
    };
  }, []);

  const suggestions = useMemo(() => {
    const byNeed = suggest(entries, needs);
    return [...byNeed, ...suggestForHardware(entries, plan?.hardware?.npu_name, byNeed)];
  }, [entries, needs, plan]);
  const steps: StepId[] = useMemo(() => {
    const base: StepId[] = ["language", "welcome", "level"];
    if (mode !== "simple") return base;
    return [
      ...base,
      "models",
      "needs",
      ...(suggestions.length ? (["suggest"] as StepId[]) : []),
      "ready",
    ];
  }, [mode, suggestions.length]);
  const step = steps[index] ?? "ready";

  // Le catalogue se charge pendant qu'on répond aux besoins.
  useEffect(() => {
    if (step !== "needs" || fetched.current) return;
    fetched.current = true;
    core
      .browseExtensionCatalog({})
      .then((s) => (s.entries.length ? s : core.refreshExtensionCatalog()))
      .then((s) => setEntries(s.entries))
      .catch((e) => console.warn("[Accueil] catalogue d'extensions illisible :", e));
  }, [step]);

  if (profile.onboarded) return null;

  // Pas de borne haute : un minuteur lancé avant que la liste des étapes ne
  // s'allonge (choix du niveau) doit pouvoir avancer ; `step` retombe sur la fin.
  const go = (delta: number) => {
    setBack(delta < 0);
    setIndex((i) => Math.max(0, i + delta));
  };

  const finish = (chosen: UiMode, chosenNeeds: Need[]) => {
    if (chosen === "simple") {
      // L'interface simplifiée a son propre accueil : pas de second tour.
      try {
        window.localStorage.setItem(PRESENTATION_KEY, "1");
      } catch (e) {
        console.warn("[Accueil] présentation non mémorisée :", e);
      }
    }
    updateProfile({
      onboarded: true,
      onboardingVersion: ONBOARDING_VERSION,
      mode: chosen,
      needs: chosenNeeds,
    });
  };

  const pickLevel = (m: UiMode) => {
    setMode(m);
    // L'interface suit tout de suite la réponse : on la voit changer derrière.
    updateProfile({ mode: m });
    window.setTimeout(
      () => (m === "expert" ? finish("expert", []) : go(1)),
      reducedMotion() ? 0 : 650,
    );
  };

  const pickModel = (tier: ModelTier) => {
    const model = plan?.choices.find((m) => m.tier === tier);
    if (model) onInstallModel(model.tag).catch((e) => console.error("[Accueil] installation :", e));
    go(1);
  };

  const toggleNeed = (n: Need) =>
    setNeeds((cur) => (cur.includes(n) ? cur.filter((x) => x !== n) : [...cur, n]));

  const installSuggestion = (e: CatalogEntry) => {
    finish("simple", needs);
    onInstallExtension(e.install_source);
  };

  const last = step === "ready";
  const autoAdvance = step === "level" || step === "models";
  // Après le choix du modèle on ne revient pas en arrière : le téléchargement est lancé.
  const canBack = index > 0 && step !== "models" && step !== "needs";

  return (
    <div className="lo-ob" role="dialog" aria-modal="true" aria-label={tx.aria}>
      <div className="lo-ob-card">
        <div className="lo-ob-progress" aria-hidden="true">
          {steps.map((s, i) => (
            <i key={s} data-state={i < index ? "done" : i === index ? "now" : "todo"} />
          ))}
        </div>
        <div className="lo-ob-step" key={step} data-dir={back ? "back" : "fwd"}>
          {step === "language" && <LanguageStep tx={tx} />}
          {step === "welcome" && <WelcomeStep tx={tx} />}
          {step === "level" && <LevelStep tx={tx} picked={mode} onPick={pickLevel} />}
          {step === "models" && <ModelsStep tx={tx} lang={lang} plan={plan} onPick={pickModel} />}
          {step === "needs" && (
            <NeedsStep tx={tx} lang={lang} needs={needs} onToggle={toggleNeed} />
          )}
          {step === "suggest" && (
            <SuggestStep tx={tx} items={suggestions} onInstall={installSuggestion} />
          )}
          {step === "ready" && <ReadyStep tx={tx} needs={needs} />}
        </div>
        <div className="lo-ob-actions">
          {canBack ? (
            <button type="button" className="lo-ob-btn" onClick={() => go(-1)}>
              {tx.back}
            </button>
          ) : (
            <span />
          )}
          {step === "models" && (
            <button type="button" className="lo-ob-btn" onClick={() => go(1)}>
              {tx.skip}
            </button>
          )}
          {!autoAdvance && (
            <button
              type="button"
              className="lo-ob-btn lo-ob-btn-primary"
              onClick={() => (last ? finish("simple", needs) : go(1))}
            >
              {last ? tx.start : step === "suggest" ? tx.later : tx.next}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
