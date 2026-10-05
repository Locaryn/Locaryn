import { useEffect, useState } from "react";
import { core } from "../lib/core";

/**
 * Le dossier où travaille une conversation : celui de son projet, ou, pour
 * une conversation libre, le dossier temporaire que le service lui a créé.
 * `fallback` sert tant qu'il n'a pas répondu, ou s'il ne répond pas.
 */
export function useSessionWorkspace(
  sessionId: string | null | undefined,
  fallback: string | null,
): string | null {
  const [folder, setFolder] = useState<string | null>(fallback);
  useEffect(() => {
    if (!sessionId) {
      setFolder(fallback);
      return;
    }
    let alive = true;
    core
      .sessionWorkspace(sessionId)
      .then((p) => {
        if (alive) setFolder(p);
      })
      .catch((e: unknown) => {
        console.warn("dossier de la conversation introuvable :", e);
        if (alive) setFolder(fallback);
      });
    return () => {
      alive = false;
    };
  }, [sessionId, fallback]);
  return folder;
}
