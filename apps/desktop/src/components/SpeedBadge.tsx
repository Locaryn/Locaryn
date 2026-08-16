import type { ModelMetric } from "../lib/core";

/**
 * La vitesse d'un modèle, telle qu'elle a été mesurée ici.
 *
 * Affichée seulement quand elle existe : une case vide dit « pas encore
 * essayé », ce qui est une information ; un chiffre inventé n'en serait pas
 * une. Le nombre de mesures accompagne la moyenne, parce qu'une moyenne sur
 * une seule génération et une moyenne sur trente ne se lisent pas pareil.
 */
export function SpeedBadge({ metric }: { metric: ModelMetric | undefined }) {
  if (!metric) return null;

  const label = formatSpeed(metric);
  if (!label) return null;

  const measured = new Date(metric.last_measured_at);
  const when = Number.isNaN(measured.getTime())
    ? ""
    : ` · dernière mesure le ${measured.toLocaleDateString("fr-FR")}`;

  return (
    <span
      className="locaryn-badge locaryn-badge-speed"
      title={`Moyenne sur ${metric.samples} génération${metric.samples > 1 ? "s" : ""} sur cette machine${when}`}
    >
      {label}
    </span>
  );
}

/** Le chiffre, dans l'unité qui parle pour ce type de modèle. */
export function formatSpeed(metric: ModelMetric): string | null {
  if (metric.kind === "chat" && metric.avg_tokens_per_second) {
    return `${metric.avg_tokens_per_second.toFixed(1)} jetons/s`;
  }
  if (metric.avg_duration_ms) {
    const seconds = metric.avg_duration_ms / 1000;
    // Sous la minute, les secondes suffisent ; au-delà, « 2 min 10 » se lit
    // mieux que « 130 s ».
    if (seconds < 60) return `${seconds.toFixed(seconds < 10 ? 1 : 0)} s`;
    const minutes = Math.floor(seconds / 60);
    const rest = Math.round(seconds % 60);
    return rest === 0 ? `${minutes} min` : `${minutes} min ${rest}`;
  }
  return null;
}

/**
 * Retrouve la mesure d'un modèle.
 *
 * Le catalogue désigne des familles (`qwen3-4b`), les mesures portent le nom
 * du fichier réellement lancé (`Qwen3-4B-Instruct-2507-Q4_K_M.gguf`). On
 * accepte donc l'égalité stricte, le nom de fichier seul, puis la famille
 * contenue dans le nom : c'est la même chose sous deux écritures, et exiger
 * l'identité parfaite reviendrait à ne jamais rien afficher.
 */
export function findMetric(
  metrics: ModelMetric[],
  model: string,
  kind?: string,
): ModelMetric | undefined {
  const file = (model.split(/[/\\]/).pop() ?? model).toLowerCase();
  const family = file.replace(/\.(gguf|safetensors|bin)$/i, "");
  const ofKind = metrics.filter((m) => !kind || m.kind === kind);
  return (
    ofKind.find((m) => m.model.toLowerCase() === file) ??
    ofKind.find((m) => m.model.toLowerCase().includes(family)) ??
    // La famille peut être plus courte que le fichier mesuré ; on regarde
    // aussi dans l'autre sens, en gardant la mesure la plus fournie.
    ofKind
      .filter((m) => family.includes(m.model.toLowerCase().replace(/\.\w+$/, "")))
      .sort((a, b) => b.samples - a.samples)[0]
  );
}
