// @locaryn/ui-core — primitives partagées par le bureau, le téléphone et le web.
//
// Les valeurs de design ne vivent PAS ici : la seule source de vérité est
// `packages-ui/tokens/tokens.css`. Ce paquet ne porte que ce que le CSS ne
// peut pas faire seul — les icônes, le vocabulaire des capacités, et les
// quatre formes de chargement dont deux demandent du calcul.

// Un seul jeu d'icônes pour l'ordinateur et le téléphone : deux dessins pour
// la même idée, ce sont deux applications qui se contredisent de près.
export { Icon, isIconName, type IconName } from "./icons";

// Un seul vocabulaire de capacités pour le daemon et les deux clients : la
// liste canonique de `packages/shared-types/capabilities.json`, lue ici et
// embarquée côté Rust. La documentation n'en tient plus de copie.
export {
  CAPABILITIES,
  CAPABILITY_IDS,
  isCapability,
  capabilityLabel,
  type Capability,
} from "./capabilities";

// Le thème : mode sombre / clair et couleur d'accentuation. Partagé pour que
// la dérivation de l'accent en mode clair ne diverge pas d'un client à l'autre.
export {
  ACCENT_PRESETS,
  accentForLight,
  accentForMode,
  applyTheme,
  hexToChannels,
  hexToRgb,
  resolveMode,
  systemMode,
  watchSystemMode,
  type AccentPreset,
  type ResolvedMode,
  type ThemeMode,
} from "./theme";

// L'interrupteur : il se clique et il se tire. Un seul dessin, un seul geste,
// sur les trois clients.
export { LoSwitch, type LoSwitchProps } from "./switch";

// Les quatre formes de chargement du système visuel. Il n'y en a que quatre,
// et on n'en invente pas d'autres.
export {
  LoProgress,
  LoMorph,
  LoSkeleton,
  LoSpinner,
  type LoProgressProps,
  type LoMorphProps,
  type LoSkeletonProps,
  type LoSpinnerProps,
} from "./loading";
