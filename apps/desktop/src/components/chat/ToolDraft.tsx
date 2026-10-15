import { Icon, highlightCode, languageFromPath } from "@locaryn/ui-core";
import { useEffect, useRef } from "react";

export type Brouillon = {
  index: number;
  tool: string;
  path: string | null;
  chars: number;
  tail: string;
};

function taille(n: number): string {
  if (n < 1000) return `${n} caractères`;
  return `${(n / 1000).toFixed(1)} k caractères`;
}

/**
 * Un appel d'outil pendant qu'il s'écrit : « Écriture de index.html… », la
 * taille déjà produite et la fin du contenu, en direct. Sans elle, un fichier
 * de plusieurs milliers de jetons laissait la réponse figée des minutes.
 */
export function ToolDraft({ brouillon }: { brouillon: Brouillon }) {
  const codeRef = useRef<HTMLPreElement>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: on suit la fin à chaque nouveau fragment
  useEffect(() => {
    const el = codeRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [brouillon.tail]);

  const nom = brouillon.path?.split(/[\/]/).pop();
  const titre = nom ? `Écriture de ${nom}` : `Préparation de ${brouillon.tool}`;
  return (
    <div className="locaryn-tool-draft" aria-live="polite">
      <div className="locaryn-tool-draft-head">
        <span className="locaryn-tool-draft-pulse" aria-hidden="true" />
        <Icon name={nom ? "file-text" : "wrench"} size={13} />
        <span className="locaryn-tool-draft-titre">{titre}…</span>
        <span className="locaryn-tool-draft-taille">{taille(brouillon.chars)}</span>
      </div>
      {brouillon.tail && (
        <pre
          ref={codeRef}
          className="locaryn-tool-draft-code"
          // biome-ignore lint/security/noDangerouslySetInnerHtml: highlightCode échappe chaque morceau avant de l'envelopper
          dangerouslySetInnerHTML={{
            __html: highlightCode(brouillon.tail, languageFromPath(brouillon.path ?? "")),
          }}
        />
      )}
    </div>
  );
}
