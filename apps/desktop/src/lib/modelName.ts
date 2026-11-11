// Lire le nom d'un modèle comme une personne le lirait.
//
// Un dépôt se nomme parfois
// « Qwen3.8-27B-TURBO-Fable-Cold-Fusion-735-882-Heretic-Uncensored-NEO-CODER-MAX-MTP-Q4_K_M » :
// posé tel quel en titre, il déborde de sa carte et on n'y retrouve ni la
// famille ni la taille. Ici le nom est découpé : la famille et la taille en
// titre, la quantification à part, et le reste (variantes, réglages d'un
// auteur) en petites étiquettes.

export interface NomDeModele {
  /** Famille et taille : « Qwen3.8 27B ». */
  titre: string;
  /** Quantification (« Q4_K_M »), si le nom la porte. */
  quantification: string | null;
  /** Format du fichier (« GGUF »). */
  format: string;
  /** Ce que le nom dit de plus : « TURBO », « Heretic », « MTP »… */
  variantes: string[];
}

const QUANTIFICATION =
  /(?:^|[-_.])((?:I?Q[1-8](?:_[A-Z0-9]+)*|F(?:P)?16|F32|BF16|INT8|UD-I?Q[1-8](?:_[A-Z0-9]+)*))(?=$|[-_.])/i;

/** « 27B », « 1.5b », « 600M » : un nombre de paramètres. */
const TAILLE = /^\d+(?:\.\d+)?[bm]$/i;

/** Sans taille dans le nom, combien de mots font le titre. */
const MOTS_DU_TITRE = 3;

/** Les mots qui disent comment le modèle se comporte ou se charge. */
const MARQUEURS =
  /^(mtp|abliterated|uncensored|heretic|instruct|it|chat|vision|thinking|reasoning|coder|code)$/i;

/** Les mots de format ou de publication, jamais utiles à lire. */
const BRUIT = /^(gguf|ggml|safetensors|i1|imatrix|latest)$/i;

/** Un numéro de série au milieu des variantes (« 735 », « 882 »). */
function sansInteret(mot: string): boolean {
  return /^\d+$/.test(mot);
}

/** Le nom d'un modèle local en une ligne courte : « Qwen3 8B · IQ4_NL ». */
export function nomCourt(brut: string): string {
  const nom = decrireModele(brut);
  return [nom.titre, nom.quantification].filter(Boolean).join(" · ");
}

/** Découper un nom de fichier ou de dépôt en ce qu'une carte doit montrer. */
export function decrireModele(brut: string): NomDeModele {
  const normalise = brut.replace(/\\/g, "/");
  // « auteur__dépôt » : l'auteur n'est pas le nom du modèle.
  const fichier = (normalise.split("/").pop() ?? normalise).replace(/^[^_]+__/, "");
  // Une extension connue seulement : « …-0.6b » n'est pas un fichier « .6b ».
  const ext = fichier.match(/\.(gguf|ggml|safetensors|bin|pth|pt|onnx|ckpt)$/i);
  const format = ext?.[1]?.toUpperCase() ?? "MODEL";
  let tige = ext ? fichier.slice(0, -ext[0].length) : fichier;
  tige = tige.replace(/[-_.]\d{5}-of-\d{5}$/i, "").replace(/:latest$/i, "");

  const q = tige.match(QUANTIFICATION);
  const quantification = q?.[1]?.toUpperCase() ?? null;
  if (q && q.index !== undefined) {
    tige = `${tige.slice(0, q.index)}${tige.slice(q.index + q[0].length)}`;
  }

  // Un point entre deux chiffres appartient à un numéro de version (« 3.8 »).
  const mots = tige
    .split(/[-_\s]+|\.(?!\d)/)
    .filter(Boolean)
    .filter((m) => !BRUIT.test(m));
  const iTaille = mots.findIndex((m) => TAILLE.test(m));
  // Sans taille dans le nom, il n'y a pas de frontière entre la famille et ses
  // variantes : les premiers mots font le titre (« phi4 mini »).
  const fin = iTaille >= 0 ? iTaille : Math.min(mots.length, MOTS_DU_TITRE) - 1;
  const titre =
    mots
      .slice(0, fin + 1)
      .map((m, i) => (i === iTaille ? m.toUpperCase() : m))
      .join(" ") || fichier;

  const vus = new Set<string>();
  const variantes = mots
    .slice(fin + 1)
    .filter((m) => !sansInteret(m))
    .filter((m) => {
      const cle = m.toLowerCase();
      if (vus.has(cle)) return false;
      vus.add(cle);
      return true;
    })
    // Ce qui change l'usage du modèle d'abord : au-delà de quelques
    // étiquettes le reste est résumé, et « MTP » ne doit pas s'y perdre.
    .sort((a, b) => Number(MARQUEURS.test(b)) - Number(MARQUEURS.test(a)));
  return { titre, quantification, format, variantes };
}
