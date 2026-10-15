// Minimal, dependency-free markdown renderer for chat messages.
//
// Safety model: the source text is HTML-escaped FIRST, then our own tags are
// injected around already-escaped content, so no user/model-provided HTML can
// reach the DOM. Links are restricted to http(s) and open externally.
//
// Supported: fenced code blocks (with syntax colouring), inline code, bold,
// italic, strikethrough, links, headings, horizontal rules, blockquotes,
// nested ordered/unordered/task lists, tables, paragraphs.
//
// Local models often bend the syntax: a fence glued to the end of a sentence
// ("… (ex. : ```nginx"), a closing fence glued to the last line of code
// ("}```"), fences indented under a list item, numbered steps separated by
// paragraphs. Each of these is repaired rather than shown as raw text.

function escapeHtml(s: string): string {
  return s
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

// ── Inline ──────────────────────────────────────────────────────────────

/** Inline transforms on a raw line (escapes it itself). */
function renderInline(raw: string): string {
  // Code spans are cut out first so nothing inside them becomes bold/links.
  const spans: string[] = [];
  const sansCode = raw.replace(/(`+)([^`]|[^`][\s\S]*?[^`])\1(?!`)/g, (_, _ticks, body: string) => {
    spans.push(
      `<code>${escapeHtml(body.trim() === "" ? body : body.replace(/^ (.*) $/, "$1"))}</code>`,
    );
    // Repère d'usage privé (U+E000) : ni échappé par escapeHtml, ni touché par les motifs.
    return `${spans.length - 1}`;
  });
  const html = escapeHtml(sansCode)
    // [label](https://url)
    .replace(
      /\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g,
      '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>',
    )
    // bare https://url (not already inside a link)
    .replace(
      /(^|[\s(])(https?:\/\/[^\s<)]+[^\s<).,;:!?'"])/g,
      '$1<a href="$2" target="_blank" rel="noopener noreferrer">$2</a>',
    )
    // **bold** / __bold__
    .replace(/\*\*(?=\S)([\s\S]*?\S)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^\w])__(?=\S)([\s\S]*?\S)__(?!\w)/g, "$1<strong>$2</strong>")
    // *italic* / _italic_ (single markers, not part of ** or snake_case)
    .replace(/(^|[^*\w])\*(?=\S)([^*\n]*?\S)\*(?!\*)/g, "$1<em>$2</em>")
    .replace(/(^|[^\w])_(?=\S)([^_\n]*?\S)_(?!\w)/g, "$1<em>$2</em>")
    // ~~strike~~
    .replace(/~~(?=\S)([\s\S]*?\S)~~/g, "<del>$1</del>");
  return html.replace(/(\d+)/g, (_, n: string) => spans[Number(n)] ?? "");
}

// ── Syntax colouring ────────────────────────────────────────────────────

type Famille = {
  ligne: string[];
  bloc?: [string, string];
  mots: Set<string>;
  /** Clés en début de ligne (`cle:` YAML, `cle =` TOML/INI, directives nginx). */
  cles?: RegExp;
  variables?: RegExp;
  balises?: boolean;
};

const mots = (s: string) => new Set(s.split(/\s+/).filter(Boolean));

const C_LIKE = mots(`
  if else for while do switch case default break continue return goto try catch finally throw throws
  new delete class struct enum union interface extends implements public private protected static final
  const let var void int long short char float double bool boolean byte unsigned signed auto
  true false null nullptr undefined this super self import export from package namespace using
  typeof instanceof in of as is async await yield function fn func def lambda virtual override abstract
  sizeof template typename inline extern volatile register mut pub impl trait where match loop move ref
  type crate mod use dyn unsafe static_assert go defer chan select map range struct interface string
  readonly declare keyof never unknown any number bigint symbol satisfies get set constructor
`);

const PYTHON = mots(`
  and as assert async await break class continue def del elif else except False finally for from global
  if import in is lambda None nonlocal not or pass raise return True try while with yield match case self
  print len range int str float list dict set tuple bool
`);

const SHELL = mots(`
  if then else elif fi for while until do done case esac in function return exit export local readonly
  echo cd ls cp mv rm mkdir sudo apt brew npm npx pnpm yarn pip git docker curl wget cat grep sed awk chmod
  source set unset shift true false
  param foreach Write-Host Get-ChildItem Set-Location New-Item Remove-Item $true $false $null
`);

const SQL = mots(`
  select from where insert into values update set delete create table drop alter add column primary key
  foreign references join inner left right outer full on group by order having limit offset as and or not
  null is in like between distinct union all index view default unique check exists case when then else end
  int integer varchar text boolean date timestamp serial bigint float real
  SELECT FROM WHERE INSERT INTO VALUES UPDATE SET DELETE CREATE TABLE DROP ALTER ADD COLUMN PRIMARY KEY
  FOREIGN REFERENCES JOIN INNER LEFT RIGHT OUTER FULL ON GROUP BY ORDER HAVING LIMIT OFFSET AS AND OR NOT
  NULL IS IN LIKE BETWEEN DISTINCT UNION ALL INDEX VIEW DEFAULT UNIQUE CHECK EXISTS CASE WHEN THEN ELSE END
  INT INTEGER VARCHAR TEXT BOOLEAN DATE TIMESTAMP SERIAL BIGINT FLOAT REAL
`);

const CONF = mots(`
  true false yes no on off null
  server location listen upstream proxy_pass root index include return rewrite http events worker_processes
  FROM RUN CMD ENTRYPOINT COPY ADD ENV ARG WORKDIR EXPOSE VOLUME USER LABEL HEALTHCHECK AS
`);

const FAMILLES: Record<string, Famille> = {
  c: { ligne: ["//"], bloc: ["/*", "*/"], mots: C_LIKE },
  py: { ligne: ["#"], mots: PYTHON },
  sh: { ligne: ["#"], mots: SHELL, variables: /^\$\{?[\w:]+\}?/ },
  sql: { ligne: ["--"], bloc: ["/*", "*/"], mots: SQL },
  css: {
    ligne: [],
    bloc: ["/*", "*/"],
    mots: mots("important inherit initial none auto"),
    cles: /^(\s*)([\w-]+)(?=\s*:)/,
    variables: /^--[\w-]+/,
  },
  conf: {
    ligne: ["#"],
    mots: CONF,
    cles: /^(\s*-?\s*)([\w.$][\w.$-]*)(?=\s*[:=]|\s+\S)/,
    variables: /^\$\{?[\w]+\}?/,
  },
  json: { ligne: ["//"], mots: mots("true false null"), cles: /^(\s*)("(?:[^"\\]|\\.)*")(?=\s*:)/ },
  html: { ligne: [], bloc: ["<!--", "-->"], mots: new Set(), balises: true },
  generique: { ligne: ["//"], bloc: ["/*", "*/"], mots: C_LIKE },
};

const LANGAGES: Record<string, keyof typeof FAMILLES> = {
  js: "c",
  jsx: "c",
  javascript: "c",
  ts: "c",
  tsx: "c",
  typescript: "c",
  mjs: "c",
  cjs: "c",
  c: "c",
  h: "c",
  cpp: "c",
  "c++": "c",
  cc: "c",
  hpp: "c",
  cs: "c",
  csharp: "c",
  java: "c",
  kotlin: "c",
  kt: "c",
  swift: "c",
  rust: "c",
  rs: "c",
  go: "c",
  golang: "c",
  dart: "c",
  php: "c",
  scala: "c",
  glsl: "c",
  hlsl: "c",
  luau: "sql",
  lua: "sql",
  groovy: "c",
  gdscript: "py",
  py: "py",
  python: "py",
  python3: "py",
  rb: "py",
  ruby: "py",
  sh: "sh",
  bash: "sh",
  shell: "sh",
  zsh: "sh",
  console: "sh",
  powershell: "sh",
  ps1: "sh",
  ps: "sh",
  pwsh: "sh",
  bat: "sh",
  cmd: "sh",
  batch: "sh",
  fish: "sh",
  sql: "sql",
  mysql: "sql",
  postgres: "sql",
  postgresql: "sql",
  sqlite: "sql",
  css: "css",
  scss: "css",
  sass: "css",
  less: "css",
  yaml: "conf",
  yml: "conf",
  toml: "conf",
  ini: "conf",
  conf: "conf",
  cfg: "conf",
  env: "conf",
  nginx: "conf",
  dockerfile: "conf",
  docker: "conf",
  properties: "conf",
  makefile: "conf",
  make: "conf",
  "docker-compose": "conf",
  compose: "conf",
  hcl: "conf",
  terraform: "conf",
  tf: "conf",
  json: "json",
  jsonc: "json",
  json5: "json",
  html: "html",
  htm: "html",
  xml: "html",
  svg: "html",
  vue: "html",
  svelte: "html",
  xaml: "html",
  markup: "html",
};

/** Normalise une étiquette de langage (`TypeScript`, `language-ts`, `.rs`…). */
export function codeLanguage(lang: string): string {
  return lang
    .trim()
    .toLowerCase()
    .replace(/^language-/, "")
    .replace(/^\./, "")
    .split(/[\s{]/)[0];
}

/** Langage d'un fichier d'après son nom (pour la vue Source des fichiers). */
export function languageFromPath(path: string): string {
  const nom = path.split(/[\\/]/).pop()?.toLowerCase() ?? "";
  if (nom === "dockerfile" || nom.startsWith("dockerfile.")) return "dockerfile";
  if (nom === "makefile") return "makefile";
  if (nom.startsWith(".env")) return "env";
  const point = nom.lastIndexOf(".");
  return point >= 0 ? nom.slice(point + 1) : "";
}

const span = (classe: string, texte: string) =>
  `<span class="tk-${classe}">${escapeHtml(texte)}</span>`;

function colorierBalises(code: string): string {
  let out = "";
  let i = 0;
  while (i < code.length) {
    if (code.startsWith("<!--", i)) {
      const fin = code.indexOf("-->", i + 4);
      const j = fin < 0 ? code.length : fin + 3;
      out += span("com", code.slice(i, j));
      i = j;
      continue;
    }
    if (code[i] === "<" && /[A-Za-z/!?]/.test(code[i + 1] ?? "")) {
      const fin = code.indexOf(">", i);
      const j = fin < 0 ? code.length : fin + 1;
      const balise = code.slice(i, j);
      out += balise.replace(
        /^(<\/?[!?]?)([\w:.-]*)([\s\S]*?)(\/?>?)$/,
        (_, ouvre: string, nom: string, attrs: string, ferme: string) =>
          `${span("punct", ouvre)}${span("tag", nom)}${attrs.replace(
            /([\w:@.-]+)(\s*=\s*)?("[^"]*"|'[^']*'|[^\s"'>]+)?|([^\w:@.-]+)/g,
            (m: string, nomA?: string, egal?: string, valeur?: string, autre?: string) => {
              if (autre !== undefined) return escapeHtml(m);
              return `${span("attr", nomA ?? "")}${escapeHtml(egal ?? "")}${valeur ? span("str", valeur) : ""}`;
            },
          )}${span("punct", ferme)}`,
      );
      i = j;
      continue;
    }
    const prochain = code.indexOf("<", i + 1);
    const j = prochain < 0 ? code.length : prochain;
    out += escapeHtml(code.slice(i, j));
    i = j;
  }
  return out;
}

/**
 * Colore un bloc de code : chaque morceau est échappé avant d'être enveloppé
 * dans un `<span class="tk-…">`, le résultat reste sûr à injecter.
 */
export function highlightCode(code: string, lang: string): string {
  const cle = LANGAGES[codeLanguage(lang)];
  if (!cle && lang.trim() === "") return escapeHtml(code);
  const f = FAMILLES[cle ?? "generique"];
  if (f.balises) return colorierBalises(code);

  let out = "";
  let i = 0;
  let debutDeLigne = true;
  const n = code.length;
  while (i < n) {
    const reste = code.slice(i, i + 200);
    const c = code[i];

    if (c === "\n") {
      out += "\n";
      i += 1;
      debutDeLigne = true;
      continue;
    }

    // Clés en début de ligne (YAML, TOML, CSS, JSON).
    if (debutDeLigne && f.cles) {
      const ligne = code.slice(i, code.indexOf("\n", i) < 0 ? n : code.indexOf("\n", i));
      const m = ligne.match(f.cles);
      if (m && !f.ligne.some((p) => ligne.trimStart().startsWith(p))) {
        out += escapeHtml(m[1]) + span("key", m[2]);
        i += m[0].length;
        debutDeLigne = false;
        continue;
      }
    }
    if (c !== " " && c !== "\t") debutDeLigne = false;

    // Commentaires.
    if (f.bloc && code.startsWith(f.bloc[0], i)) {
      const fin = code.indexOf(f.bloc[1], i + f.bloc[0].length);
      const j = fin < 0 ? n : fin + f.bloc[1].length;
      out += span("com", code.slice(i, j));
      i = j;
      continue;
    }
    const prec = i === 0 ? "\n" : code[i - 1];
    if (f.ligne.some((p) => code.startsWith(p, i)) && (c !== "#" || /\s/.test(prec))) {
      const fin = code.indexOf("\n", i);
      const j = fin < 0 ? n : fin;
      out += span("com", code.slice(i, j));
      i = j;
      continue;
    }

    // Chaînes.
    if (c === '"' || c === "'" || c === "`") {
      // Apostrophe dans un mot (« l'app » dans un commentaire mal fermé, don't) : pas une chaîne.
      if (c === "'" && /\w/.test(prec) && cle !== "py") {
        out += escapeHtml(c);
        i += 1;
        continue;
      }
      let j = i + 1;
      while (j < n && code[j] !== c) {
        if (code[j] === "\\") j += 1;
        else if (code[j] === "\n" && c !== "`") break;
        j += 1;
      }
      j = Math.min(n, j + 1);
      out += span("str", code.slice(i, j));
      i = j;
      continue;
    }

    // Variables ($var, --css-var).
    if (f.variables) {
      const m = reste.match(f.variables);
      if (m && !/[\w$-]/.test(prec)) {
        out += span("var", m[0]);
        i += m[0].length;
        continue;
      }
    }

    // Nombres.
    const nombre = reste.match(/^(?:0x[\da-fA-F]+|\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?)[a-zA-Z%]*/);
    if (nombre && !/[\w.]/.test(prec)) {
      out += span("num", nombre[0]);
      i += nombre[0].length;
      continue;
    }

    // Mots : mots-clés, appels de fonction, types.
    const mot = reste.match(/^[A-Za-z_$@][\w$-]*/);
    if (mot) {
      let m = mot[0];
      // Le tiret n'appartient au mot que pour les familles qui en ont (CSS, shell, conf).
      if (cle !== "css" && cle !== "sh" && cle !== "conf") m = m.replace(/-.*$/, "");
      const suivant = code[i + m.length];
      if (f.mots.has(m)) out += span("kw", m);
      else if (m.startsWith("@")) out += span("kw", m);
      else if (suivant === "(") out += span("fn", m);
      else if (/^[A-Z][a-z]\w*$/.test(m) && cle !== "conf" && cle !== "sql") out += span("type", m);
      else out += escapeHtml(m);
      i += m.length;
      continue;
    }

    out += escapeHtml(c);
    i += 1;
  }
  return out;
}

// ── Blocks ──────────────────────────────────────────────────────────────

const OUVERTURE = /^(\s*)(`{3,}|~{3,})\s*([\w+#.-]*)[^`]*$/;

/**
 * Remet chaque clôture de bloc de code seule sur sa ligne : les modèles
 * locaux écrivent volontiers « (ex. : ```nginx » ou « }``` ».
 */
function separerLesClotures(lignes: string[]): string[] {
  const out: string[] = [];
  let dansUnBloc = false;
  for (const ligne of lignes) {
    if (!dansUnBloc) {
      const ouverture = ligne.match(OUVERTURE);
      if (ouverture) {
        dansUnBloc = true;
        out.push(ligne);
        continue;
      }
      // Texte suivi d'une ouverture : « … (ex. : ```nginx ».
      const collee = ligne.match(/^(.*?\S)\s*(`{3,}|~{3,})([\w+#.-]*)\s*$/);
      if (collee && !collee[1].includes("```") && (collee[1].split("`").length - 1) % 2 === 0) {
        out.push(collee[1], collee[2] + collee[3]);
        dansUnBloc = true;
        continue;
      }
      out.push(ligne);
      continue;
    }
    if (/^\s*(`{3,}|~{3,})\s*$/.test(ligne)) {
      dansUnBloc = false;
      out.push(ligne);
      continue;
    }
    // Code suivi de la clôture : « }``` ».
    const fin = ligne.match(/^(.*\S)\s*(`{3,}|~{3,})\s*$/);
    if (fin && !fin[1].includes("```")) {
      out.push(fin[1], fin[2]);
      dansUnBloc = false;
      continue;
    }
    out.push(ligne);
  }
  return out;
}

function indentation(ligne: string): number {
  let n = 0;
  for (const c of ligne) {
    if (c === " ") n += 1;
    else if (c === "\t") n += 4;
    else break;
  }
  return n;
}

const PUCE = /^(\s*)([-*+]|\d{1,9}[.)])\s+(.*)$/;
const LIGNE_HR = /^\s{0,3}([-*_])(\s*\1){2,}\s*$/;

function cellules(ligne: string): string[] {
  let l = ligne.trim();
  if (l.startsWith("|")) l = l.slice(1);
  if (l.endsWith("|") && !l.endsWith("\\|")) l = l.slice(0, -1);
  return l.split(/(?<!\\)\|/).map((c) => c.trim().replaceAll("\\|", "|"));
}

function estSeparateurDeTableau(ligne: string): boolean {
  return /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/.test(ligne) && ligne.includes("-");
}

function rendreBlocDeCode(lang: string, code: string[]): string {
  const lg = codeLanguage(lang);
  const langAttr = lg ? ` data-lang="${escapeHtml(lg)}"` : "";
  return `<pre class="md-code"${langAttr}><code>${highlightCode(code.join("\n"), lg)}</code></pre>`;
}

/** Rend des lignes de blocs (récursif pour les listes et citations). */
function rendreBlocs(lines: string[]): string {
  const out: string[] = [];
  let i = 0;
  // Étapes « 1. … texte … 1. … » : une liste qui repart à 1 après une autre
  // (séparée par du texte ou du code, pas par un titre) reprend la numérotation.
  let suiteNumerotee = 0;

  let para: string[] = [];
  const flushPara = () => {
    if (para.length > 0) {
      out.push(`<p>${para.map((l) => renderInline(l.trim())).join("<br/>")}</p>`);
      para = [];
    }
  };

  while (i < lines.length) {
    const line = lines[i];

    // Bloc de code (clôture éventuellement indentée sous une liste).
    const fence = line.match(OUVERTURE);
    if (fence) {
      flushPara();
      const retrait = fence[1].length;
      const marque = fence[2];
      const code: string[] = [];
      i += 1;
      while (i < lines.length) {
        const fermeture = lines[i].match(/^\s*(`{3,}|~{3,})\s*$/);
        if (fermeture && fermeture[1][0] === marque[0] && fermeture[1].length >= marque.length)
          break;
        const l = lines[i];
        code.push(
          indentation(l) >= retrait
            ? l.slice(Math.min(retrait, l.length - l.trimStart().length))
            : l.trimStart(),
        );
        i += 1;
      }
      i += 1; // clôture (ou fin du texte pendant le streaming)
      out.push(rendreBlocDeCode(fence[3], code));
      continue;
    }

    // Titres.
    const heading = line.match(/^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$/);
    if (heading) {
      flushPara();
      const niveau = Math.min(heading[1].length, 4);
      out.push(`<p class="md-heading md-h${niveau}">${renderInline(heading[2])}</p>`);
      suiteNumerotee = 0;
      i += 1;
      continue;
    }

    // Ligne horizontale.
    if (LIGNE_HR.test(line)) {
      flushPara();
      out.push("<hr/>");
      suiteNumerotee = 0;
      i += 1;
      continue;
    }

    // Tableau : en-tête + séparateur.
    if (line.includes("|") && i + 1 < lines.length && estSeparateurDeTableau(lines[i + 1])) {
      flushPara();
      const entetes = cellules(line);
      const aligns = cellules(lines[i + 1]).map((c) =>
        c.startsWith(":") && c.endsWith(":") ? "center" : c.endsWith(":") ? "right" : "",
      );
      const style = (k: number) => (aligns[k] ? ` style="text-align:${aligns[k]}"` : "");
      i += 2;
      const corps: string[] = [];
      while (i < lines.length && lines[i].includes("|") && lines[i].trim() !== "") {
        const cs = cellules(lines[i]);
        corps.push(
          `<tr>${entetes.map((_, k) => `<td${style(k)}>${renderInline(cs[k] ?? "")}</td>`).join("")}</tr>`,
        );
        i += 1;
      }
      out.push(
        `<div class="md-table"><table><thead><tr>${entetes
          .map((h, k) => `<th${style(k)}>${renderInline(h)}</th>`)
          .join("")}</tr></thead><tbody>${corps.join("")}</tbody></table></div>`,
      );
      continue;
    }

    // Citation (lignes `>` consécutives, rendues récursivement).
    if (/^\s{0,3}>/.test(line)) {
      flushPara();
      const quote: string[] = [];
      while (i < lines.length && /^\s{0,3}>/.test(lines[i])) {
        quote.push(lines[i].replace(/^\s{0,3}>\s?/, ""));
        i += 1;
      }
      out.push(`<blockquote>${rendreBlocs(quote)}</blockquote>`);
      continue;
    }

    // Listes (imbriquées par l'indentation).
    const puce = line.match(PUCE);
    if (puce && !(para.length > 0 && indentation(line) > 3)) {
      flushPara();
      const base = indentation(line);
      const ordonnee = /\d/.test(puce[2]);
      const tag = ordonnee ? "ol" : "ul";
      let debut = ordonnee ? Number.parseInt(puce[2], 10) : 1;
      if (ordonnee && debut === 1 && suiteNumerotee > 1) debut = suiteNumerotee;
      const items: string[] = [];
      let tache = false;
      while (i < lines.length) {
        const m = lines[i].match(PUCE);
        if (!m || indentation(lines[i]) !== base || /\d/.test(m[2]) !== ordonnee) break;
        // L'élément : sa première ligne puis tout ce qui est indenté dessous.
        const contenu: string[] = [m[3]];
        i += 1;
        while (i < lines.length) {
          const l = lines[i];
          if (l.trim() === "") {
            // Une ligne vide ne termine l'élément que si la suite n'est pas indentée dessous.
            let k = i + 1;
            while (k < lines.length && lines[k].trim() === "") k += 1;
            if (k < lines.length && indentation(lines[k]) > base && !LIGNE_HR.test(lines[k])) {
              contenu.push("");
              i += 1;
              continue;
            }
            break;
          }
          if (indentation(l) > base) {
            contenu.push(l.slice(Math.min(indentation(l), base + 2 + (ordonnee ? 1 : 0))));
            i += 1;
            continue;
          }
          // Ligne paresseuse : du texte collé sous l'élément, sans retrait.
          if (!PUCE.test(l) && !OUVERTURE.test(l) && !/^\s*(#|>|\|)/.test(l) && !LIGNE_HR.test(l)) {
            contenu.push(l);
            i += 1;
            continue;
          }
          break;
        }
        const coche = contenu[0].match(/^\[([ xX])\]\s+(.*)$/);
        let classe = "";
        if (coche && !ordonnee) {
          tache = true;
          contenu[0] = coche[2];
          classe = coche[1] === " " ? ' class="md-task"' : ' class="md-task md-task-done"';
        }
        const corps = rendreBlocs(contenu);
        // Un seul paragraphe : on retire le <p> pour garder une liste serrée.
        const simple = corps.match(/^<p>([\s\S]*?)<\/p>([\s\S]*)$/);
        const html = simple && !simple[1].includes("<p>") ? simple[1] + simple[2] : corps;
        const caseACocher = classe ? '<span class="md-check" aria-hidden="true"></span>' : "";
        items.push(`<li${classe}>${caseACocher}${html}</li>`);
        // Éléments séparés par une ligne vide : la liste continue.
        let k = i;
        while (k < lines.length && lines[k].trim() === "") k += 1;
        const suivant = k < lines.length ? lines[k].match(PUCE) : null;
        if (
          k > i &&
          suivant &&
          indentation(lines[k]) === base &&
          /\d/.test(suivant[2]) === ordonnee
        ) {
          i = k;
        }
      }
      if (ordonnee) suiteNumerotee = debut + items.length;
      const startAttr = ordonnee && debut !== 1 ? ` start="${debut}"` : "";
      const tacheAttr = tache ? ' class="md-tasks"' : "";
      out.push(`<${tag}${startAttr}${tacheAttr}>${items.join("")}</${tag}>`);
      continue;
    }

    // Ligne vide : fin de paragraphe.
    if (line.trim() === "") {
      flushPara();
      i += 1;
      continue;
    }

    para.push(line);
    i += 1;
  }
  flushPara();
  return out.join("");
}

/** Render markdown to sanitized HTML (see safety model above). */
export function renderMarkdown(src: string): string {
  return rendreBlocs(separerLesClotures(src.replaceAll("\r\n", "\n").split("\n")));
}
