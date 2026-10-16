import { Icon, type IconName, LoSpinner } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";

type Props = {
  tool: string;
  args: unknown;
  status: "running" | "ok" | "error";
  output: string;
  /** La sortie reçue en direct pendant l'exécution. */
  live?: string;
};

const TOOL_ICONS: Record<string, IconName> = {
  read_file: "models",
  write_file: "edit",
  run_command: "cpu",
  search: "search",
  list_dir: "project",
};

/** Compact one-line summary of the tool arguments. */
function argsSummary(args: unknown): string {
  if (args && typeof args === "object") {
    const entries = Object.entries(args as Record<string, unknown>);
    if (entries.length > 0) {
      return entries
        .map(([k, v]) => `${k}: ${typeof v === "string" ? v : JSON.stringify(v)}`)
        .join(" · ");
    }
  }
  return JSON.stringify(args);
}

/** Les codes de couleur d'un terminal (`\x1b[32m`…) : illisibles hors terminal. */
function sansAnsi(t: string): string {
  // biome-ignore lint/suspicious/noControlCharactersInRegex: on retire justement les séquences d'échappement.
  return t.replace(/\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07]*\x07|\r(?!\n)/g, "");
}

/** Le JSON d'une réponse d'outil, nu ou emballé par MCP (`content[].text`). */
function lireJson(brut: string): unknown {
  try {
    const v = JSON.parse(brut) as unknown;
    if (Array.isArray(v) && typeof v[0]?.text === "string") return lireJson(v[0].text) ?? v;
    const texte = (v as { content?: { text?: string }[] } | null)?.content?.[0]?.text;
    if (typeof texte === "string") return lireJson(texte) ?? texte;
    return v;
  } catch {
    return null;
  }
}

/** Une commande lancée par l'agent, sous la forme où on l'écrirait. */
type Commande = {
  commande: string;
  /** Où elle tourne : la distribution WSL, le serveur SSH, ou ce poste. */
  lieu: string;
  sorte: "wsl" | "ssh" | "local";
  dossier?: string;
  delai?: number;
  root: boolean;
};

function lireCommande(tool: string, args: unknown): Commande | null {
  const a = (args && typeof args === "object" ? args : {}) as Record<string, unknown>;
  if (typeof a.command !== "string") return null;
  const court = tool.replace(/^.*__/, "");
  if (!/(^|_)(run|exec|run_command|start)$/.test(court) && court !== "run_command") return null;
  const sorte = /^wsl_/.test(court) ? "wsl" : /^ssh_/.test(court) ? "ssh" : "local";
  const lieu =
    sorte === "wsl"
      ? typeof a.distro === "string" && a.distro
        ? a.distro
        : "WSL"
      : sorte === "ssh"
        ? String(a.server_id ?? a.server ?? "SSH")
        : "ce poste";
  const dossier =
    typeof a.cwd === "string"
      ? a.cwd
      : typeof a.working_dir === "string"
        ? a.working_dir
        : undefined;
  return {
    commande: a.command,
    lieu,
    sorte,
    dossier,
    delai: typeof a.timeout_s === "number" ? a.timeout_s : undefined,
    root: a.as_root === true,
  };
}

/** Le résultat d'une commande, lu dans la réponse de l'outil. */
type Resultat = { code: number | null; stdout: string; stderr: string; notes: string[] };

function lireResultat(output: string): Resultat | null {
  // `run_command` intégré : « [exit 0]\nsortie\n--- stderr ---\nerreurs ».
  const natif = /^\[exit (-?\d+)\]\n([\s\S]*)$/.exec(output.replace(/^ERROR: /, ""));
  if (natif) {
    const [stdout, stderr = ""] = natif[2].split("\n--- stderr ---\n");
    return { code: Number(natif[1]), stdout, stderr, notes: [] };
  }
  const v = lireJson(output) as Record<string, unknown> | null;
  if (!v || typeof v !== "object") return null;
  const texte = (k: string) => (typeof v[k] === "string" ? (v[k] as string) : "");
  if (!("stdout" in v) && !("stderr" in v) && !("exit_code" in v)) return null;
  const code = typeof v.exit_code === "number" ? v.exit_code : null;
  const notes = ["note", "wsl_notice", "reason", "next"].map(texte).filter(Boolean);
  return { code, stdout: texte("stdout"), stderr: texte("stderr"), notes };
}

/** Une réponse JSON rendue lisible : vraies lignes, indentation. */
function lisible(output: string): string {
  const v = lireJson(output);
  if (typeof v === "string") return v;
  if (v && typeof v === "object") return JSON.stringify(v, null, 2);
  return output;
}

/** Le terminal d'une commande : en direct pendant qu'elle tourne. */
function Terminal({
  texte,
  erreur,
  direct,
}: {
  texte: string;
  erreur?: string;
  direct: boolean;
}) {
  const ref = useRef<HTMLPreElement>(null);
  const colle = useRef(true);
  // biome-ignore lint/correctness/useExhaustiveDependencies: suivre la fin à chaque nouveau morceau.
  useEffect(() => {
    const el = ref.current;
    if (el && colle.current) el.scrollTop = el.scrollHeight;
  }, [texte, erreur]);
  if (!texte && !erreur) {
    return (
      <pre className="locaryn-cmd-sortie is-vide">
        {direct ? "En attente de la sortie…" : "(aucune sortie)"}
      </pre>
    );
  }
  return (
    <pre
      ref={ref}
      className="locaryn-cmd-sortie"
      onScroll={(e) => {
        const el = e.currentTarget;
        colle.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
      }}
    >
      {sansAnsi(texte)}
      {erreur && <span className="locaryn-cmd-stderr">{sansAnsi(erreur)}</span>}
    </pre>
  );
}

function CarteCommande({
  c,
  status,
  output,
  live,
}: {
  c: Commande;
  status: Props["status"];
  output: string;
  live?: string;
}) {
  const r = status === "running" ? null : lireResultat(output);
  return (
    <div className="locaryn-cmd">
      <div className="locaryn-cmd-tete">
        <span className={`locaryn-cmd-lieu is-${c.sorte}`}>{c.lieu}</span>
        {c.root && <span className="locaryn-cmd-root">root</span>}
        {c.dossier && <span className="locaryn-cmd-dossier">{c.dossier}</span>}
        <span className="locaryn-cmd-espace" />
        {r && r.code !== null && (
          <span className={`locaryn-cmd-code ${r.code === 0 ? "is-ok" : "is-ko"}`}>
            code {r.code}
          </span>
        )}
        {c.delai !== undefined && <span className="locaryn-cmd-delai">délai {c.delai} s</span>}
      </div>
      <pre className="locaryn-cmd-ligne">
        <span className="locaryn-cmd-invite" aria-hidden="true">
          {c.root ? "#" : "$"}
        </span>
        {c.commande}
      </pre>
      {status === "running" ? (
        <Terminal texte={live ?? ""} direct />
      ) : r ? (
        <>
          <Terminal texte={r.stdout} erreur={r.stderr} direct={false} />
          {r.notes.map((n) => (
            <p key={n} className="locaryn-cmd-note">
              {n}
            </p>
          ))}
        </>
      ) : (
        <Terminal texte={lisible(output)} direct={false} />
      )}
    </div>
  );
}

export function ToolCard({ tool, args, status, output, live }: Props) {
  const [open, setOpen] = useState(false);
  const icon = TOOL_ICONS[tool] ?? "extensions";
  const commande = lireCommande(tool, args);

  return (
    <div className={`locaryn-tool-card locaryn-tool-${status}`}>
      <button
        type="button"
        className="locaryn-tool-head"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
      >
        <span className="locaryn-tool-icon" aria-hidden="true">
          <Icon name={commande ? "cpu" : icon} size={14} />
        </span>
        <span className="locaryn-tool-name">{tool.replace(/^.*__/, "")}</span>
        <span className="locaryn-tool-summary">
          {commande ? commande.commande : argsSummary(args)}
        </span>
        <span className={`locaryn-tool-status locaryn-tool-status-${status}`}>
          {status === "running" ? (
            <LoSpinner size="sm" label="Outil en cours" />
          ) : status === "ok" ? (
            <Icon name="check" size={13} />
          ) : (
            <Icon name="close" size={13} />
          )}
        </span>
        <span className="locaryn-tool-chevron" aria-hidden="true">
          <Icon name={open ? "caret-down" : "caret-right"} size={12} />
        </span>
      </button>
      {open &&
        (commande ? (
          <div className="locaryn-tool-body">
            <CarteCommande c={commande} status={status} output={output} live={live} />
          </div>
        ) : (
          <div className="locaryn-tool-body">
            <div className="locaryn-tool-section">args</div>
            <pre className="locaryn-tool-pre">{JSON.stringify(args, null, 2)}</pre>
            {(output || live) && (
              <>
                <div className="locaryn-tool-section">output</div>
                <pre className="locaryn-tool-pre">
                  {status === "running" && live
                    ? sansAnsi(live)
                    : (() => {
                        const t = lisible(output);
                        return t.length > 8000 ? `${t.slice(0, 8000)}…` : t;
                      })()}
                </pre>
              </>
            )}
          </div>
        ))}
    </div>
  );
}
