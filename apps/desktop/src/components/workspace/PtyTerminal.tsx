import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef, useState } from "react";
import { core } from "../../lib/core";
import { enregistrerTerminal, oublierTerminal } from "../../lib/terminaux";

type Props = {
  /** Dossier où le shell démarre ; `null` = celui du service. */
  cwd: string | null;
  /** L'identifiant de l'onglet, pour que « Exécuter » retrouve ce terminal. */
  ongletId?: string;
  /** L'onglet est affiché : le terminal se redimensionne et prend le focus. */
  visible?: boolean;
};

/** Une couleur du thème de l'application, ou `repli` hors navigateur. */
function jeton(nom: string, repli: string): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue(nom).trim();
  return v || repli;
}

/**
 * Un vrai terminal : le shell de la machine dans un pseudo-terminal, affiché par
 * xterm.js. Les couleurs, l'historique du shell, `cd` qui dure, les programmes
 * qui posent une question — tout fonctionne comme dans une fenêtre de terminal.
 */
export function PtyTerminal({ cwd, ongletId, visible = true }: Props) {
  const zone = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const ajusteRef = useRef<FitAddon | null>(null);
  const idRef = useRef<string | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [termine, setTermine] = useState<number | null | undefined>(undefined);
  /** Relance : un nouveau shell quand le précédent s'est terminé. */
  const [session, setSession] = useState(0);
  // Le dossier de la conversation arrive un instant après l'ouverture de
  // l'onglet : le shell attend qu'il se stabilise pour démarrer au bon endroit,
  // puis ne bouge plus (un `cd` de la personne ne doit pas être défait).
  const [depart, setDepart] = useState<{ cwd: string | null } | null>(null);
  const demarre = useRef(false);
  useEffect(() => {
    if (demarre.current) return;
    const t = window.setTimeout(() => setDepart({ cwd }), 250);
    return () => window.clearTimeout(t);
  }, [cwd]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `session` n'est pas lu, son changement relance le shell (bouton « Relancer »).
  useEffect(() => {
    const conteneur = zone.current;
    if (!conteneur || !depart) return;
    demarre.current = true;
    const dossier = depart.cwd;
    let vivant = true;
    setErreur(null);
    setTermine(undefined);

    const terminal = new Terminal({
      cursorBlink: true,
      fontFamily: jeton("--font-mono", "ui-monospace, Consolas, monospace"),
      fontSize: 12.5,
      lineHeight: 1.25,
      scrollback: 5000,
      allowProposedApi: false,
      theme: {
        background: jeton("--bg", "#0e1012"),
        foreground: jeton("--text", "#e6e8e6"),
        cursor: jeton("--accent", "#5fa37e"),
        selectionBackground: "rgba(95, 163, 126, 0.30)",
      },
    });
    const ajuste = new FitAddon();
    terminal.loadAddon(ajuste);
    terminal.open(conteneur);
    terminalRef.current = terminal;
    ajusteRef.current = ajuste;
    try {
      ajuste.fit();
    } catch (e) {
      console.warn("[terminal] ajustement initial impossible :", e);
    }

    let retire = () => {};
    core
      .ptyOpen(dossier, terminal.rows, terminal.cols, (ev) => {
        if (!vivant) return;
        if (ev.type === "data") terminal.write(ev.text);
        else setTermine(ev.code);
      })
      .then((id) => {
        if (!vivant) {
          core.ptyClose(id).catch((e) => console.warn("[terminal] fermeture :", e));
          return;
        }
        idRef.current = id;
        if (ongletId) {
          retire = enregistrerTerminal(ongletId, {
            ecrire: (texte) => {
              core.ptyWrite(id, texte).catch((e) => console.warn("[terminal] écriture :", e));
            },
          });
        }
      })
      .catch((e) => {
        if (vivant) setErreur(String(e).replace(/^Error:\s*/, ""));
      });

    const frappe = terminal.onData((data) => {
      const id = idRef.current;
      if (id) core.ptyWrite(id, data).catch((e) => console.warn("[terminal] frappe :", e));
    });
    const taille = terminal.onResize(({ rows, cols }) => {
      const id = idRef.current;
      if (id) core.ptyResize(id, rows, cols).catch((e) => console.warn("[terminal] taille :", e));
    });
    const observe = new ResizeObserver(() => {
      // Un onglet caché a une taille nulle : l'ajuster écraserait le terminal.
      if (conteneur.clientWidth === 0 || conteneur.clientHeight === 0) return;
      try {
        ajuste.fit();
      } catch (e) {
        console.warn("[terminal] ajustement impossible :", e);
      }
    });
    observe.observe(conteneur);

    return () => {
      vivant = false;
      observe.disconnect();
      frappe.dispose();
      taille.dispose();
      retire();
      if (ongletId) oublierTerminal(ongletId);
      const id = idRef.current;
      idRef.current = null;
      if (id) core.ptyClose(id).catch((e) => console.warn("[terminal] fermeture :", e));
      terminal.dispose();
      terminalRef.current = null;
      ajusteRef.current = null;
    };
  }, [session, ongletId, depart]);

  // L'onglet revient au premier plan : retrouver sa taille et le focus.
  useEffect(() => {
    if (!visible) return;
    const t = window.setTimeout(() => {
      try {
        ajusteRef.current?.fit();
      } catch (e) {
        console.warn("[terminal] ajustement impossible :", e);
      }
      terminalRef.current?.focus();
    }, 30);
    return () => window.clearTimeout(t);
  }, [visible]);

  return (
    <div className="locaryn-pty">
      <div className="locaryn-pty-zone" ref={zone} />
      {erreur && (
        <div className="locaryn-pty-etat" role="alert">
          {erreur}
        </div>
      )}
      {termine !== undefined && !erreur && (
        <div className="locaryn-pty-etat" role="status">
          Le shell s'est terminé{termine !== null ? ` (code ${termine})` : ""}.
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => setSession((n) => n + 1)}
          >
            Relancer
          </button>
        </div>
      )}
    </div>
  );
}
