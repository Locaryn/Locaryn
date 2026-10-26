import { convertFileSrc } from "@tauri-apps/api/core";

/**
 * Turn a path or URL into something an `<img>`, `<audio>` or `<video>` element
 * can actually load.
 *
 * A webview cannot read a raw disk path: `<audio src="D:\clip.wav">` resolves
 * against the page origin and silently yields a zero-length media element
 * (the `0:00 / 0:00` symptom). Everything that reaches a media element must
 * pass through here.
 *
 * Data and blob URLs are already loadable and are returned untouched.
 */
export function toMediaUrl(pathOrUrl: string): string {
  if (!pathOrUrl) return "";
  if (
    pathOrUrl.startsWith("data:") ||
    pathOrUrl.startsWith("blob:") ||
    pathOrUrl.startsWith("http:") ||
    pathOrUrl.startsWith("https:") ||
    pathOrUrl.startsWith("asset:")
  ) {
    return pathOrUrl;
  }
  // Normalise Windows separators and percent-encode spaces and accents, which
  // are common in user media folders and break the asset protocol otherwise.
  return convertFileSrc(encodeURI(pathOrUrl.replace(/\\/g, "/")));
}

/** MIME type for a media path, by extension. The asset protocol does not
 *  always set one, and `<audio>` needs it to compute a duration. */
function mediaMimeType(path: string): string {
  const ext = path.toLowerCase().split(".").pop() ?? "";
  switch (ext) {
    case "mp3":
      return "audio/mpeg";
    case "flac":
      return "audio/flac";
    case "ogg":
    case "oga":
      return "audio/ogg";
    case "m4a":
    case "aac":
      return "audio/mp4";
    case "opus":
      return "audio/opus";
    case "webm":
      return "audio/webm";
    default:
      return "audio/wav";
  }
}

/**
 * Load a media file into a blob URL, with the correct MIME type attached.
 *
 * Preferred over a bare {@link toMediaUrl} for `<audio>`: a blob carries an
 * explicit type, so the element can seek and report a duration instead of
 * showing `0:00 / 0:00`. Rejects rather than falling back silently — a
 * fallback URL the webview cannot load looks identical to the bug it hides.
 */
export async function loadMediaObjectUrl(path: string): Promise<string> {
  if (path.startsWith("data:") || path.startsWith("blob:")) return path;
  const src = toMediaUrl(path);
  const res = await fetch(src);
  if (!res.ok) {
    throw new Error(`Lecture impossible (${res.status}) : ${path}`);
  }
  const buf = await res.arrayBuffer();
  if (buf.byteLength === 0) {
    throw new Error(`Fichier vide ou illisible : ${path}`);
  }
  return URL.createObjectURL(new Blob([buf], { type: mediaMimeType(path) }));
}

/**
 * L'adresse d'un fichier du disque pour un cadre qui charge ses voisins : une
 * page HTML et son `style.css`, son `script.js`, ses images.
 *
 * `convertFileSrc` encode tout le chemin Windows en un seul segment
 * (`C%3A%5CUsers%5C…`) : les chemins relatifs de la page se résolvaient
 * alors à la racine du protocole, et l'aperçu s'affichait sans style ni
 * script. Ici chaque dossier reste un segment, et `style.css` se trouve à
 * côté de la page.
 */
export function toFrameUrl(cheminAbsolu: string, version?: string): string {
  const base = convertFileSrc("").replace(/\/+$/, "");
  const segments = cheminAbsolu
    .replace(/\\/g, "/")
    .split("/")
    .filter((s) => s.length > 0)
    .map(encodeURIComponent);
  const url = `${base}/${segments.join("/")}`;
  return version ? `${url}?v=${encodeURIComponent(version)}` : url;
}
