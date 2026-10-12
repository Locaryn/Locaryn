import { ContextRing, formatTokens } from "@locaryn/ui-core";
import type { ContextPart } from "../../lib/core";

/** Ce que la boucle d'agent a mesuré au dernier tour. */
export type ContextUsage = {
  window: number;
  parts: ContextPart[];
  deferredTokens: number;
  deferredCount: number;
};

const POSTES: { key: ContextPart["key"]; libelle: string; teinte: string }[] = [
  { key: "messages", libelle: "Messages", teinte: "var(--ctx-messages)" },
  { key: "system", libelle: "Prompt système", teinte: "var(--ctx-system)" },
  { key: "tools", libelle: "Outils intégrés", teinte: "var(--ctx-tools)" },
  { key: "mcp_tools", libelle: "Outils des connecteurs", teinte: "var(--ctx-mcp)" },
];

/**
 * Le détail de la fenêtre de contexte : tout ce que le modèle reçoit, pas
 * seulement les messages — le prompt système et les définitions d'outils
 * pèsent souvent plus que la conversation elle-même.
 */
export function ContextDetails({
  usage,
  fenetre,
  estimeMessages,
}: {
  usage: ContextUsage | null;
  /** La fenêtre appliquée, ou `null` quand rien ne la dit (aucun modèle chargé) :
   *  sans maximum connu, un anneau aux proportions inventées mentirait. */
  fenetre: number | null;
  /** Faute de mesure (conversation rouverte), l'estimation des seuls messages. */
  estimeMessages: number;
}) {
  const mesures: ContextPart[] = usage?.parts ?? [{ key: "messages", tokens: estimeMessages }];
  const parts = POSTES.flatMap((p) => {
    const m = mesures.find((x) => x.key === p.key);
    return m ? [{ key: p.key, label: p.libelle, tokens: m.tokens, color: p.teinte }] : [];
  });
  const utilise = mesures.reduce((acc, m) => acc + m.tokens, 0);
  return (
    <div
      className="locaryn-ctx-details"
      role="dialog"
      aria-label="Détail de la fenêtre de contexte"
    >
      <p className="locaryn-ctx-details-tete">Fenêtre de contexte</p>
      {fenetre === null ? (
        <p className="locaryn-ctx-details-note">
          Maximum inconnu tant qu'aucun modèle n'est chargé : la place dépend du modèle qui sera
          chargé. Les jetons utilisés restent comptés, ~{formatTokens(utilise)} ici.
        </p>
      ) : (
        <>
          <ContextRing
            parts={parts}
            window={fenetre}
            extra={
              usage && usage.deferredCount > 0
                ? {
                    label: `Outils différés (${usage.deferredCount})`,
                    tokens: usage.deferredTokens,
                  }
                : null
            }
          />
          <p className="locaryn-ctx-details-note">
            {usage
              ? "Maintenir la jauge pour compresser la conversation."
              : "Seuls les messages sont estimés : la mesure complète arrive au prochain message."}
          </p>
        </>
      )}
    </div>
  );
}
