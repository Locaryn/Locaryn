import { useEffect, useRef, useState } from "react";
import { attachTerminalText, selectionIn } from "../../lib/attachText";
import { core } from "../../lib/core";

type TermLine = { stream: "stdout" | "stderr" | "cmd" | "meta"; text: string };

type Props = {
  /** Dossier où les commandes s'exécutent ; `null` = celui du service. */
  cwd: string | null;
  /** Message d'accueil, première ligne du terminal. */
  greeting?: string;
};

/**
 * Une ligne de commande : saisie, historique (flèches), sortie en continu.
 *
 * Partagée par le panneau du bas et les onglets « Terminal » de l'espace de
 * travail : chacun a son historique et sa sortie.
 */
export function TerminalConsole({ cwd, greeting = "Terminal Locaryn" }: Props) {
  const [lines, setLines] = useState<TermLine[]>([{ stream: "meta", text: greeting }]);
  const [cmd, setCmd] = useState("");
  const [running, setRunning] = useState(false);
  const [history, setHistory] = useState<string[]>([]);
  const [histIdx, setHistIdx] = useState(-1);
  const scrollRef = useRef<HTMLDivElement>(null);
  /** La sélection en cours dans la sortie, et où poser le bouton. */
  const [selection, setSelection] = useState<{ texte: string; x: number; y: number } | null>(null);

  function suivreSelection() {
    const zone = scrollRef.current;
    const texte = selectionIn(zone);
    const range = texte ? window.getSelection()?.getRangeAt(0) : null;
    if (!zone || !texte || !range) {
      setSelection(null);
      return;
    }
    const r = range.getBoundingClientRect();
    const z = zone.getBoundingClientRect();
    setSelection({
      texte,
      x: Math.min(Math.max(r.left - z.left, 0), z.width - 170),
      y: Math.max(r.top - z.top + zone.scrollTop - 36, zone.scrollTop + 4),
    });
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: `lines` n'est pas lu ici, il déclenche : c'est son changement qui signale qu'il y a du nouveau à suivre.
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines]);

  async function run() {
    const command = cmd.trim();
    if (!command || running) return;
    if (command === "clear" || command === "cls") {
      setCmd("");
      setLines([]);
      return;
    }
    setCmd("");
    setHistory((h) => [...h, command]);
    setHistIdx(-1);
    setRunning(true);
    setLines((prev) => [...prev, { stream: "cmd", text: `$ ${command}` }]);
    try {
      await core.runTerminal(command, cwd, (ev) => {
        setLines((prev) => [
          ...prev,
          ev.type === "line"
            ? { stream: ev.stream, text: ev.text }
            : { stream: "meta", text: `— code ${ev.code ?? "?"}` },
        ]);
      });
    } catch (e) {
      setLines((prev) => [...prev, { stream: "stderr", text: String(e) }]);
    } finally {
      setRunning(false);
    }
  }

  // Flèches haut et bas : l'historique des commandes.
  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      void run();
    } else if (e.key === "ArrowUp" && history.length > 0) {
      e.preventDefault();
      const idx = histIdx < 0 ? history.length - 1 : Math.max(0, histIdx - 1);
      setHistIdx(idx);
      setCmd(history[idx]);
    } else if (e.key === "ArrowDown" && histIdx >= 0) {
      e.preventDefault();
      const idx = histIdx + 1;
      if (idx >= history.length) {
        setHistIdx(-1);
        setCmd("");
      } else {
        setHistIdx(idx);
        setCmd(history[idx]);
      }
    }
  }

  return (
    <div className="locaryn-terminal">
      <div
        className="locaryn-term-scroll"
        ref={scrollRef}
        onMouseUp={suivreSelection}
        onKeyUp={suivreSelection}
        onScroll={() => setSelection(null)}
      >
        {selection && (
          <button
            type="button"
            className="locaryn-term-attach"
            style={{ left: selection.x, top: selection.y }}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => {
              attachTerminalText(selection.texte);
              window.getSelection()?.removeAllRanges();
              setSelection(null);
            }}
          >
            Joindre au message
          </button>
        )}
        {lines.map((l, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: flux du terminal : les lignes ne sont qu'ajoutées en fin, et deux lignes identiques sont courantes.
          <div key={i} className={`locaryn-term-line locaryn-term-${l.stream}`}>
            {l.text}
          </div>
        ))}
      </div>
      <div className="locaryn-term-input-row">
        <span className="locaryn-term-prompt">{running ? "…" : ">"}</span>
        <input
          className="locaryn-term-input"
          value={cmd}
          disabled={running}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          placeholder={cwd ? `commande dans ${cwd}` : "commande…"}
          aria-label="Commande"
          onChange={(e) => setCmd(e.target.value)}
          onKeyDown={onKeyDown}
        />
      </div>
    </div>
  );
}
