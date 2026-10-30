import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type WorkspaceChanges, type WorkspaceCommit, core } from "../../lib/core";

type Props = { sessionId: string | null; visible: boolean };

/** Le code de `git status`, en mots. */
function etatLisible(code: string): string {
  if (code === "??") return "nouveau";
  if (code.includes("A")) return "ajouté";
  if (code.includes("D")) return "supprimé";
  if (code.includes("R")) return "renommé";
  if (code.includes("U")) return "en conflit";
  return "modifié";
}

/** La classe de couleur d'un état, en ASCII. */
function classeEtat(code: string): string {
  if (code === "??") return "nouveau";
  if (code.includes("A")) return "ajoute";
  if (code.includes("D")) return "supprime";
  if (code.includes("U")) return "conflit";
  return "modifie";
}

/** Une ligne de diff et sa couleur. */
function classeLigne(ligne: string): string {
  if (ligne.startsWith("+++") || ligne.startsWith("---")) return "lw-diff-meta";
  if (ligne.startsWith("@@")) return "lw-diff-hunk";
  if (ligne.startsWith("+")) return "lw-diff-add";
  if (ligne.startsWith("-")) return "lw-diff-del";
  return "";
}

/** « il y a 4 min », assez précis pour un historique de conversation. */
function ilYA(iso: string): string {
  const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (!Number.isFinite(s)) return "";
  if (s < 60) return "à l'instant";
  if (s < 3600) return `il y a ${Math.round(s / 60)} min`;
  if (s < 86400) return `il y a ${Math.round(s / 3600)} h`;
  return `il y a ${Math.round(s / 86400)} j`;
}

/** Le compte +/− d'un fichier ou d'un commit, et sa barre en cinq cases. */
function Delta({ ajouts, retraits }: { ajouts: number | null; retraits: number | null }) {
  if (ajouts === null && retraits === null) {
    return <span className="lw-delta lw-delta-bin">binaire</span>;
  }
  const a = ajouts ?? 0;
  const r = retraits ?? 0;
  const total = a + r;
  const vertes = total === 0 ? 0 : Math.round((a / total) * 5);
  return (
    <span className="lw-delta" data-no-translate>
      <span className="lw-diff-add">+{a}</span>
      <span className="lw-diff-del">−{r}</span>
      <span className="lw-delta-bar" aria-hidden="true">
        {[0, 1, 2, 3, 4].map((i) => (
          <i key={i} className={total === 0 ? "" : i < vertes ? "lw-delta-add" : "lw-delta-del"} />
        ))}
      </span>
    </span>
  );
}

function DiffView({ diff }: { diff: string | null }) {
  return (
    <pre className="lw-diff">
      {diff === null
        ? "Lecture…"
        : diff.trim() === ""
          ? "(aucune différence de texte)"
          : diff.split("\n").map((l, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: lignes d'un diff figé, deux lignes identiques sont courantes.
              <span key={i} className={classeLigne(l)}>
                {l}
                {"\n"}
              </span>
            ))}
    </pre>
  );
}

function Pending({
  changes,
  ouvert,
  diff,
  onOpen,
}: {
  changes: WorkspaceChanges;
  ouvert: string | null;
  diff: string | null;
  onOpen: (chemin: string) => void;
}) {
  const a = changes.fichiers.reduce((n, f) => n + (f.ajouts ?? 0), 0);
  const r = changes.fichiers.reduce((n, f) => n + (f.retraits ?? 0), 0);
  return (
    <section className="lw-section">
      <h4 className="lw-section-head">
        <span>Non commité</span>
        <Delta ajouts={a} retraits={r} />
      </h4>
      <ul className="lw-entries-inline">
        {changes.fichiers.map((f) => (
          <li key={f.chemin}>
            <button
              type="button"
              className={`lw-entry${ouvert === f.chemin ? " lw-entry-open" : ""}`}
              aria-expanded={ouvert === f.chemin}
              onClick={() => onOpen(f.chemin)}
            >
              <span className={`lw-change-tag lw-change-${classeEtat(f.etat)}`}>
                {etatLisible(f.etat)}
              </span>
              <span className="lw-entry-name" title={f.chemin}>
                {f.chemin}
              </span>
              {f.ajouts != null && <Delta ajouts={f.ajouts} retraits={f.retraits} />}
            </button>
            {ouvert === f.chemin && <DiffView diff={diff} />}
          </li>
        ))}
      </ul>
    </section>
  );
}

function CommitRow({
  c,
  open,
  onToggle,
}: { c: WorkspaceCommit; open: boolean; onToggle: () => void }) {
  return (
    <li className={`lw-commit${open ? " lw-commit-open" : ""}`}>
      <button type="button" className="lw-commit-head" aria-expanded={open} onClick={onToggle}>
        <span className="lw-commit-dot" aria-hidden="true" />
        <span className="lw-commit-main">
          <span className="lw-commit-subject" title={c.sujet}>
            {c.sujet}
          </span>
          <span className="lw-commit-meta" data-no-translate>
            <code>{c.court}</code> · {c.auteur} · {ilYA(c.date)} · {c.fichiers.length} fichier
            {c.fichiers.length > 1 ? "s" : ""}
          </span>
        </span>
        <Delta ajouts={c.ajouts} retraits={c.retraits} />
      </button>
      {open && (
        <ul className="lw-commit-files">
          {c.fichiers.map((f) => (
            <li key={f.chemin}>
              <span className="lw-entry-name" title={f.chemin} data-no-translate>
                {f.chemin}
              </span>
              <Delta ajouts={f.ajouts} retraits={f.retraits} />
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

function History({ commits }: { commits: WorkspaceCommit[] }) {
  const [open, setOpen] = useState<string | null>(commits[0]?.hash ?? null);
  return (
    <section className="lw-section">
      <h4 className="lw-section-head">
        <span>Historique</span>
        <span className="lw-changes-count">
          {commits.length} commit{commits.length > 1 ? "s" : ""}
        </span>
      </h4>
      <ol className="lw-commits">
        {commits.map((c) => (
          <CommitRow
            key={c.hash}
            c={c}
            open={open === c.hash}
            onToggle={() => setOpen((v) => (v === c.hash ? null : c.hash))}
          />
        ))}
      </ol>
    </section>
  );
}

/**
 * Ce qui a changé dans le dépôt de la conversation : ce qui attend un commit,
 * puis l'historique, chaque commit avec ses fichiers et ses lignes. Relu
 * chaque fois que l'onglet revient à l'écran — c'est souvent le modèle qui
 * vient d'écrire.
 */
export function ChangesTab({ sessionId, visible }: Props) {
  const [changes, setChanges] = useState<WorkspaceChanges | null>(null);
  const [commits, setCommits] = useState<WorkspaceCommit[]>([]);
  const [ouvert, setOuvert] = useState<string | null>(null);
  const [diff, setDiff] = useState<string | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [chargement, setChargement] = useState(false);

  const relire = useCallback(async () => {
    if (!sessionId) return;
    setChargement(true);
    setErreur(null);
    try {
      const [c, h] = await Promise.all([
        core.workspaceChanges(sessionId),
        core.workspaceCommits(sessionId),
      ]);
      setChanges(c);
      setCommits(h);
    } catch (e) {
      setErreur(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setChargement(false);
    }
  }, [sessionId]);

  useEffect(() => {
    if (visible) void relire();
  }, [visible, relire]);

  async function montrer(chemin: string) {
    if (!sessionId) return;
    if (ouvert === chemin) {
      setOuvert(null);
      setDiff(null);
      return;
    }
    setOuvert(chemin);
    setDiff(null);
    try {
      setDiff(await core.workspaceDiff(sessionId, chemin));
    } catch (e) {
      setErreur(String(e).replace(/^Error:\s*/, ""));
    }
  }

  if (!sessionId) {
    return (
      <div className="lw-empty">
        <Icon name="git-branch" size={22} />
        <p>Ouvrez une conversation pour voir ses modifications.</p>
      </div>
    );
  }

  const rien = changes?.depot && changes.fichiers.length === 0 && commits.length === 0;
  return (
    <div className="lw-changes">
      <div className="lw-changes-head">
        <span className="lw-changes-branch">
          <Icon name="git-branch" size={14} />
          {changes?.depot ? (changes.branche ?? "branche détachée") : "—"}
        </span>
        <span className="lw-changes-count">
          {changes?.depot ? `${changes.fichiers.length} fichier(s) en attente` : ""}
        </span>
        <button
          type="button"
          className="lw-icon-btn"
          aria-label="Relire"
          title="Relire"
          onClick={() => void relire()}
        >
          <span className={chargement ? "locaryn-spin lw-spin" : "lw-spin"}>
            <Icon name="arrow-clockwise" size={14} />
          </span>
        </button>
      </div>

      {erreur && (
        <p className="lw-browser-error" role="alert">
          {erreur}
        </p>
      )}

      {changes && !changes.depot && (
        <div className="lw-empty">
          <Icon name="git-branch" size={22} />
          <p>Ce dossier n'est pas un dépôt git : il n'y a pas d'historique à comparer.</p>
          <code className="lw-empty-path">{changes.racine}</code>
        </div>
      )}

      {rien && (
        <div className="lw-empty">
          <Icon name="check-circle" size={22} />
          <p>Aucune modification depuis le dernier commit.</p>
        </div>
      )}

      {changes?.depot && !rien && (
        <div className="lw-entries">
          {changes.fichiers.length > 0 && (
            <Pending
              changes={changes}
              ouvert={ouvert}
              diff={diff}
              onOpen={(c) => void montrer(c)}
            />
          )}
          {commits.length > 0 && <History key={commits[0]?.hash} commits={commits} />}
        </div>
      )}
    </div>
  );
}
