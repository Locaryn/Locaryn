import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useMemo, useState } from "react";
import { type MemoryEntry, type MemoryGroup, core } from "../lib/core";
import { taskCenter } from "../lib/taskCenter";

/**
 * La mémoire est une liste de fiches, groupées, qui s'ouvrent sur leur
 * détail.
 *
 * Au repos, une fiche ne montre qu'un titre et un résumé d'une ligne : la
 * liste reste lisible même avec des dizaines de sujets. Le clic ouvre la
 * fiche entière — résumé, détails accumulés au fil des conversations, et un
 * bouton pour l'oublier — à la place de la liste. Pour corriger ou nettoyer
 * plusieurs fiches à la fois, la boîte de commande en bas traduit une
 * instruction en actions, plutôt que de cliquer une par une.
 */

const FORGET_MS = 320;

const GROUPS: { id: MemoryGroup; label: string }[] = [
  { id: "vous", label: "Vous" },
  { id: "sujets", label: "Sujets" },
  { id: "zones", label: "Zones" },
  { id: "personnes", label: "Personnes" },
];

function updatedOn(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const jours = Math.floor((Date.now() - d.getTime()) / 86_400_000);
  if (jours <= 0) return "aujourd'hui";
  if (jours === 1) return "hier";
  if (jours < 7) return `il y a ${jours} jours`;
  return `mis à jour le ${d.toLocaleDateString("fr-FR", { day: "numeric", month: "short" })}`;
}

export function MemorySettings() {
  const [entries, setEntries] = useState<MemoryEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [forgetting, setForgetting] = useState<string | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);

  const [instruction, setInstruction] = useState("");
  const [commandBusy, setCommandBusy] = useState(false);
  const [commandFeedback, setCommandFeedback] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setEntries(await core.listMemory());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const grouped = useMemo(() => {
    const parGroupe = new Map<MemoryGroup, MemoryEntry[]>();
    for (const g of GROUPS) parGroupe.set(g.id, []);
    for (const entry of entries) {
      (parGroupe.get(entry.group) ?? parGroupe.get("sujets"))?.push(entry);
    }
    return parGroupe;
  }, [entries]);

  const ouverte = entries.find((e) => e.id === openId) ?? null;

  const forget = useCallback(async (entry: MemoryEntry) => {
    setForgetting(entry.id);
    await new Promise((resolve) => setTimeout(resolve, FORGET_MS));
    try {
      await core.forgetMemory(entry.id);
      setEntries((prev) => prev.filter((e) => e.id !== entry.id));
      setOpenId(null);
      const id = taskCenter.add({ type: "edit", label: "Fiche oubliée" });
      taskCenter.done(id, { detail: entry.title });
    } catch (e) {
      setError(String(e));
    } finally {
      setForgetting(null);
    }
  }, []);

  async function forgetAll() {
    setBusy(true);
    try {
      const count = await core.forgetAllMemory();
      setEntries([]);
      setOpenId(null);
      const id = taskCenter.add({ type: "edit", label: "Mémoire vidée" });
      taskCenter.done(id, { detail: `${count} fiche${count === 1 ? "" : "s"}` });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function removeDetail(entry: MemoryEntry, detail: string) {
    try {
      const updated = await core.removeMemoryDetail(entry.id, detail);
      setEntries((prev) => prev.map((e) => (e.id === updated.id ? updated : e)));
    } catch (e) {
      setError(String(e));
    }
  }

  async function runCommand() {
    const texte = instruction.trim();
    if (!texte || commandBusy) return;
    setCommandBusy(true);
    setCommandFeedback(null);
    try {
      const result = await core.runMemoryCommand(texte);
      setEntries(result.entries);
      setCommandFeedback(result.summary);
      setInstruction("");
      if (result.applied > 0) {
        const id = taskCenter.add({ type: "edit", label: "Mémoire modifiée par instruction" });
        taskCenter.done(id, { detail: result.summary });
      }
    } catch (e) {
      setCommandFeedback(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setCommandBusy(false);
    }
  }

  const total = entries.length;

  return (
    <div className="locaryn-memory-settings">
      <div className="locaryn-memory-intro">
        <div>
          <span className="locaryn-account-eyebrow">MÉMOIRE DU COMPTE</span>
          <h3>Ce que Locaryn retient de vous</h3>
          <p>
            Une fiche par sujet — vous, vos centres d'intérêt, vos projets, les personnes que vous
            mentionnez. Cliquez une fiche pour voir ce qu'elle contient en entier.
          </p>
        </div>
        <span className="locaryn-memory-count">
          {total} fiche{total === 1 ? "" : "s"}
        </span>
      </div>

      {error && <div className="locaryn-vp-error">{error}</div>}

      {total === 0 ? (
        <div className="locaryn-memory-empty">
          <strong>La mémoire est vide.</strong>
          <span>Locaryn ne conserve encore aucune fiche sur vous.</span>
        </div>
      ) : ouverte ? (
        <MemoryDetail
          entry={ouverte}
          onBack={() => setOpenId(null)}
          onForget={() => void forget(ouverte)}
          onRemoveDetail={(d) => void removeDetail(ouverte, d)}
          busy={forgetting === ouverte.id}
        />
      ) : (
        <>
          <div className="locaryn-memory-groups">
            {GROUPS.map((g) => {
              const rows = grouped.get(g.id) ?? [];
              if (rows.length === 0) return null;
              return (
                <div key={g.id}>
                  <h4 className="locaryn-memory-group-title">{g.label}</h4>
                  <div className="locaryn-memory-group-rows">
                    {rows.map((entry) => (
                      <button
                        key={entry.id}
                        type="button"
                        className={`locaryn-memory-row${forgetting === entry.id ? " is-forgetting" : ""}`}
                        onClick={() => setOpenId(entry.id)}
                      >
                        <span className="locaryn-memory-row-main">
                          <span className="locaryn-memory-row-title">{entry.title}</span>
                          <span className="locaryn-memory-row-summary">{entry.summary}</span>
                        </span>
                        <span className="locaryn-memory-row-date">
                          {updatedOn(entry.updated_at)}
                        </span>
                      </button>
                    ))}
                  </div>
                </div>
              );
            })}
          </div>

          <div className="locaryn-memory-actions">
            <span>
              Décrivez ce qu'il faut changer dans le champ ci-dessous, ou ouvrez une fiche pour la
              corriger directement.
            </span>
            <button
              type="button"
              className="locaryn-btn-ghost"
              disabled={busy}
              onClick={() => void forgetAll()}
            >
              Tout oublier
            </button>
          </div>
        </>
      )}

      {total > 0 && !ouverte && (
        <>
          <form
            className="locaryn-memory-command"
            onSubmit={(e) => {
              e.preventDefault();
              void runCommand();
            }}
          >
            <input
              type="text"
              value={instruction}
              placeholder="Indiquez ce qu'il faut modifier ou supprimer…"
              onChange={(e) => setInstruction(e.target.value)}
              disabled={commandBusy}
            />
            <button
              type="submit"
              className="locaryn-memory-command-submit"
              disabled={commandBusy || !instruction.trim()}
              aria-label="Envoyer"
            >
              <Icon name={commandBusy ? "refresh" : "chevron"} size={15} />
            </button>
          </form>
          {commandFeedback && <p className="locaryn-memory-command-feedback">{commandFeedback}</p>}
        </>
      )}
    </div>
  );
}

function MemoryDetail({
  entry,
  onBack,
  onForget,
  onRemoveDetail,
  busy,
}: {
  entry: MemoryEntry;
  onBack: () => void;
  onForget: () => void;
  onRemoveDetail: (detail: string) => void;
  busy: boolean;
}) {
  return (
    <div className="locaryn-memory-detail">
      <div className="locaryn-memory-detail-head">
        <button type="button" className="locaryn-memory-detail-back" onClick={onBack}>
          <Icon name="back" size={15} /> Mémoire
        </button>
        <button type="button" className="locaryn-btn-ghost" disabled={busy} onClick={onForget}>
          Supprimer
        </button>
      </div>

      <h3 className="locaryn-memory-detail-title">{entry.title}</h3>

      <div className="locaryn-memory-detail-section">
        <p className="locaryn-memory-detail-label">Résumé</p>
        <p className="locaryn-memory-detail-summary">{entry.summary}</p>
      </div>

      {entry.details.length > 0 && (
        <div className="locaryn-memory-detail-section">
          <p className="locaryn-memory-detail-label">Détails</p>
          <div className="locaryn-memory-detail-list">
            {entry.details.map((detail) => (
              <div key={detail} className="locaryn-memory-detail-item">
                <span>{detail}</span>
                <button
                  type="button"
                  className="locaryn-memory-detail-item-remove"
                  title="Retirer ce détail"
                  onClick={() => onRemoveDetail(detail)}
                >
                  <Icon name="close" size={13} />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
