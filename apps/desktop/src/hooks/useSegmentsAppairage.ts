import { useEffect, useMemo, useState } from "react";
import {
  type ResolvedSlotContribution,
  getSlotContributions,
} from "../components/extensions/SlotRegistry";
import { type InstalledExtension, core } from "../lib/core";

/**
 * Le point d'extension de l'appairage : une extension y déclare ses propres
 * modes d'accès (nom, phrase, panneau). L'application n'en connaît qu'un.
 */
export const SLOT_APPAIRAGE = "settings.server.pairing";

export interface SegmentAppairage {
  id: string;
  label: string;
  description: string;
  apport?: ResolvedSlotContribution;
}

/** Le segment natif, et le seul que l'application connaisse d'elle-même. */
export const SEGMENT_LOCAL: SegmentAppairage = {
  id: "local",
  label: "Réseau local",
  description: "Les appareils sur le même Wi‑Fi que cette machine, après avoir scanné le code.",
};

/** Le réseau local, puis les segments que les extensions actives apportent. */
export function useSegmentsAppairage(): SegmentAppairage[] {
  const [extensions, setExtensions] = useState<InstalledExtension[]>([]);

  useEffect(() => {
    let annule = false;
    const lire = () => {
      core
        .listExtensions()
        .then((l) => {
          if (!annule) setExtensions(l);
        })
        .catch((e: unknown) => {
          console.warn("extensions illisibles : modes d'accès réduits au local", e);
          if (!annule) setExtensions([]);
        });
    };
    lire();
    window.addEventListener("locaryn:extensions-changed", lire);
    return () => {
      annule = true;
      window.removeEventListener("locaryn:extensions-changed", lire);
    };
  }, []);

  return useMemo(
    () => [
      SEGMENT_LOCAL,
      ...getSlotContributions(extensions, SLOT_APPAIRAGE).map((c) => ({
        id: `${c.extensionId}:${c.id}`,
        label: c.label ?? c.id,
        description: c.hint ?? "",
        apport: c,
      })),
    ],
    [extensions],
  );
}
