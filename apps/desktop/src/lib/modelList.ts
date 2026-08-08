/** Normalizes Windows backslashes to forward slashes for cross-platform parsing. */
export function normalizeModelPath(path: string): string {
  return path.replace(/\\/g, "/");
}

/** Returns the directory portion of a model path.
 *  For a top-level file, the path itself is returned. */
export function getModelDirectory(path: string): string {
  const normalized = normalizeModelPath(path);
  const lastSlash = normalized.lastIndexOf("/");
  return lastSlash !== -1 ? normalized.slice(0, lastSlash) : normalized;
}

/** Pick a canonical representative file from a list of weight files.
 *  Prefers .pth > .onnx > .bin > .safetensors, then the shortest name. */
function pickRepresentativeFile(files: string[]): string {
  const score = (f: string) => {
    const lower = f.toLowerCase();
    if (lower.endsWith(".pth")) return 1;
    if (lower.endsWith(".onnx")) return 2;
    if (lower.endsWith(".bin")) return 3;
    if (lower.endsWith(".safetensors")) return 4;
    return 5;
  };

  return [...files].sort((a, b) => {
    const diff = score(a) - score(b);
    return diff !== 0 ? diff : a.length - b.length;
  })[0];
}

/** Groups paths by parent directory, picks a canonical representative file per
 *  directory, and returns sorted normalized paths. */
export function dedupeModelsByDirectory(models: string[]): string[] {
  const groups = new Map<string, string[]>();

  for (const m of models) {
    const dir = getModelDirectory(m);
    const existing = groups.get(dir);
    if (existing) existing.push(m);
    else groups.set(dir, [m]);
  }

  const representatives = Array.from(groups.entries()).map(([dir, files]) => ({
    dir,
    file: normalizeModelPath(pickRepresentativeFile(files)),
  }));

  return representatives
    .sort((a, b) => a.dir.localeCompare(b.dir, undefined, { sensitivity: "base" }))
    .map((r) => r.file);
}
