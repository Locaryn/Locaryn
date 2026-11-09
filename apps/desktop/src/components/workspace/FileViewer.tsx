import { Icon, highlightCode, languageFromPath, renderMarkdown } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { documentApercu } from "../../lib/apercuHtml";
import { type WorkspaceFile, core, isTauri } from "../../lib/core";
import { executerFichier, genreExecution } from "../../lib/executer";
import { ecrirePressePapiers } from "../../lib/pressePapiers";

type Props = {
  sessionId: string;
  fichier: WorkspaceFile;
  /** Ouvert pour son code : le texte d'abord, même s'il a un aperçu. */
  source?: boolean;
  /** Afficher une page HTML dans le navigateur de la conversation. */
  onApercu: () => void;
  onRetour: () => void;
};

type Rendu = "markdown" | "html" | "svg" | null;

/** Ce qu'un aperçu sait montrer, d'après l'extension. */
function renduPour(chemin: string): Rendu {
  const ext = chemin.toLowerCase().split(".").pop() ?? "";
  if (["md", "markdown", "mdx"].includes(ext)) return "markdown";
  if (["html", "htm"].includes(ext)) return "html";
  if (ext === "svg") return "svg";
  return null;
}

function taille(octets: number): string {
  if (octets < 1024) return `${octets} o`;
  if (octets < 1024 * 1024) return `${(octets / 1024).toFixed(1)} Ko`;
  return `${(octets / 1024 / 1024).toFixed(1)} Mo`;
}

/**
 * Un fichier ouvert, comme dans les applications de bureau de Claude ou de
 * ChatGPT : le chemin en haut à gauche, « Aperçu » et « Source » pour passer
 * du rendu au texte, et à droite le bouton qui l'ouvre dans VS Code — sa
 * flèche propose le terminal, l'explorateur de fichiers et la copie du chemin.
 *
 * Une page HTML s'affiche dans le navigateur de la conversation, avec son
 * dossier (« Aperçu ») ; ici, on lit son code. Hors de l'application de
 * bureau, elle se rend dans un cadre isolé. Le Markdown et le SVG
 * s'affichent tels quels.
 */
export function FileViewer({ sessionId, fichier, source = false, onApercu, onRetour }: Props) {
  const rendu = renduPour(fichier.chemin);
  /** Une page HTML se prévisualise dans le navigateur, pas dans ce panneau. */
  const apercuAilleurs = rendu === "html" && isTauri;
  const modeInitial = (): "apercu" | "source" =>
    rendu && !source && !apercuAilleurs ? "apercu" : "source";
  const [mode, setMode] = useState<"apercu" | "source">(modeInitial);
  const [menu, setMenu] = useState(false);
  const [info, setInfo] = useState<string | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `modeInitial` ne dépend que du fichier et de `source`, suivis ici.
  useEffect(() => {
    setMode(modeInitial());
  }, [fichier.chemin, source]);

  useEffect(() => {
    if (!menu) return;
    const dehors = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) setMenu(false);
    };
    const echap = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenu(false);
    };
    document.addEventListener("mousedown", dehors);
    document.addEventListener("keydown", echap);
    return () => {
      document.removeEventListener("mousedown", dehors);
      document.removeEventListener("keydown", echap);
    };
  }, [menu]);

  function signaler(texte: string) {
    setInfo(texte);
    window.setTimeout(() => setInfo((t) => (t === texte ? null : t)), 2400);
  }

  async function ouvrirAvec(action: "vscode" | "terminal" | "explorer") {
    setMenu(false);
    try {
      await core.workspaceOpenWith(sessionId, fichier.chemin, action);
    } catch (e) {
      signaler(String(e).replace(/^Error:\s*/, ""));
    }
  }

  const executable = isTauri && (genreExecution(fichier.chemin) ?? "page") !== "page";

  async function executer() {
    try {
      await executerFichier(sessionId, fichier.chemin);
    } catch (e) {
      signaler(String(e).replace(/^Error:\s*/, ""));
    }
  }

  async function copierChemin() {
    setMenu(false);
    try {
      await ecrirePressePapiers(fichier.absolu ?? fichier.chemin);
      signaler("Chemin copié");
    } catch (e) {
      signaler(String(e));
    }
  }

  const morceaux = fichier.chemin.split("/");
  const contenu = fichier.contenu;

  return (
    <div className="lw-viewer">
      <div className="lw-viewer-bar">
        <button
          type="button"
          className="lw-icon-btn"
          onClick={onRetour}
          title="Retour au dossier"
          aria-label="Retour au dossier"
        >
          <Icon name="arrow-left" size={14} />
        </button>
        <span className="lw-viewer-path" title={fichier.absolu ?? fichier.chemin}>
          <Icon name="folder-simple" size={13} />
          {morceaux.map((m, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: un segment de chemin n'a pas d'autre identité que sa place.
            <span key={i} className={i === morceaux.length - 1 ? "is-dernier" : undefined}>
              {i > 0 && <span className="lw-viewer-sep">›</span>}
              {m}
            </span>
          ))}
        </span>

        <div className="lw-viewer-actions">
          {rendu && contenu !== null && (
            <div className="lw-seg" role="tablist" aria-label="Affichage">
              <button
                type="button"
                role="tab"
                aria-selected={mode === "apercu"}
                className={mode === "apercu" ? "is-actif" : undefined}
                title={apercuAilleurs ? "Afficher la page dans le navigateur" : undefined}
                onClick={() => (apercuAilleurs ? onApercu() : setMode("apercu"))}
              >
                Aperçu
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={mode === "source"}
                className={mode === "source" ? "is-actif" : undefined}
                onClick={() => setMode("source")}
              >
                Source
              </button>
            </div>
          )}

          {executable && (
            <button
              type="button"
              className="lw-icon-btn"
              title={
                genreExecution(fichier.chemin) === "projet"
                  ? "Lancer le serveur de développement et afficher l'aperçu"
                  : "Exécuter dans le terminal du panneau"
              }
              aria-label="Exécuter"
              onClick={() => void executer()}
            >
              <Icon name="lightning" size={14} />
            </button>
          )}

          <div className="lw-split" ref={menuRef}>
            <button
              type="button"
              className="lw-split-main"
              title="Ouvrir dans VS Code"
              aria-label="Ouvrir dans VS Code"
              onClick={() => void ouvrirAvec("vscode")}
            >
              <Icon name="code" size={14} />
            </button>
            <button
              type="button"
              className="lw-split-caret"
              aria-haspopup="menu"
              aria-expanded={menu}
              aria-label="Autres façons d'ouvrir"
              onClick={() => setMenu((m) => !m)}
            >
              <Icon name="caret-down" size={12} />
            </button>
            {menu && (
              <div className="lw-split-menu" role="menu">
                <button type="button" role="menuitem" onClick={() => void ouvrirAvec("vscode")}>
                  <Icon name="code" size={14} /> Ouvrir dans VS Code
                </button>
                <button type="button" role="menuitem" onClick={() => void ouvrirAvec("terminal")}>
                  <Icon name="terminal-window" size={14} /> Ouvrir dans le terminal
                </button>
                <button type="button" role="menuitem" onClick={() => void ouvrirAvec("explorer")}>
                  <Icon name="folder-open" size={14} /> Ouvrir l'emplacement
                </button>
                <button type="button" role="menuitem" onClick={() => void copierChemin()}>
                  <Icon name="copy" size={14} /> Copier le chemin
                </button>
              </div>
            )}
          </div>
        </div>
      </div>

      <div className="lw-viewer-meta">
        {taille(fichier.taille)}
        {fichier.tronque ? " · début seulement" : ""}
        {info && <span className="lw-viewer-info">{info}</span>}
      </div>

      {/* Ctrl+A après un clic ici : le contenu du fichier, pas toute la fenêtre. */}
      <div className="lw-viewer-body" data-select-scope>
        {contenu === null ? (
          <div className="lw-empty">
            <Icon name="file-text" size={22} />
            <p>Fichier binaire : il ne s'affiche pas ici.</p>
          </div>
        ) : mode === "source" || !rendu ? (
          <pre
            className="lw-file-code"
            // biome-ignore lint/security/noDangerouslySetInnerHtml: highlightCode échappe chaque morceau du fichier avant de l'envelopper dans ses <span>
            dangerouslySetInnerHTML={{
              __html: highlightCode(contenu, languageFromPath(fichier.chemin)),
            }}
          />
        ) : rendu === "markdown" ? (
          <div
            className="lw-viewer-md locaryn-markdown"
            // biome-ignore lint/security/noDangerouslySetInnerHtml: rendu Markdown échappé par renderMarkdown, comme dans la conversation
            dangerouslySetInnerHTML={{ __html: renderMarkdown(contenu) }}
          />
        ) : rendu === "html" ? (
          <iframe
            className="lw-viewer-frame"
            title={fichier.chemin}
            sandbox="allow-scripts"
            srcDoc={documentApercu(contenu)}
          />
        ) : (
          <div className="lw-viewer-svg">
            <img
              alt={fichier.chemin}
              src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(contenu)}`}
            />
          </div>
        )}
      </div>
    </div>
  );
}
