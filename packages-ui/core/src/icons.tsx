/**
 * Les icônes de Locaryn — Reicon (https://reicon.dev, licence MIT), contour.
 *
 * Un seul jeu pour l'ordinateur, le téléphone et le web : même trait, même
 * grille de 24, même couleur que le texte qu'elles accompagnent. Les dessins
 * sont ceux du paquet `reicon-react`, embarqués à la construction — rien ne se
 * charge d'un CDN, l'application fonctionne hors ligne.
 *
 * Les appels nomment l'idée (`chat`, `models`, `settings`…), pas le dessin :
 * changer de jeu n'a touché qu'à cette table, aucun des appels.
 *
 * Tailles du guide : 16 en ligne, 20 en navigation, 24 pour un vide ou un
 * en-tête.
 */

import {
  Archive,
  ArrowDown,
  ArrowLeft,
  ArrowRight,
  Bell,
  BellOff,
  BellRing,
  Box,
  Broom,
  Bug,
  Calendar,
  ChartLine,
  ChatRound,
  Check,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  ClockCircle,
  CloseCircle,
  Cloud,
  Code,
  Compass,
  Copy,
  Cpu,
  Cursor,
  Database,
  Devices,
  Download,
  Drop,
  Earth,
  Export,
  Eye,
  EyeClosed,
  FileRemove,
  FileText,
  Flash,
  Folder,
  Folder2,
  FolderAdd,
  FolderOpen,
  Gallery,
  Globe,
  HardDrive,
  HelpCircle,
  Hierarchy,
  Hourglass,
  type IconComponent,
  InfoCircle,
  Key,
  Laptop,
  Layers,
  Library,
  Lightbulb,
  Link,
  List,
  Lock,
  LockOpen,
  MagicStar,
  Magnifier,
  Maximize,
  Microchip,
  Microphone,
  Minus,
  Mobile,
  Monitor,
  Moon,
  MoreH,
  MusicNote,
  Notebook,
  Palette,
  PauseCircle,
  Pen,
  Pin,
  Plug,
  PlugCircle,
  Plus,
  Qr,
  Radar,
  Record,
  RecordCircle,
  RefreshCircle,
  RotateLeft,
  RotateRight,
  Scroll,
  Server,
  Setting2,
  Setting3,
  Settings,
  Shapes,
  ShieldCheck,
  Shop,
  SidebarLeft,
  Sitemap,
  Soundwave,
  Speedometer,
  SquareArrowRightUp,
  Star,
  Stop,
  Sun,
  Target,
  TerminalSquare,
  TestTube,
  Text,
  TextInput,
  ThreeDCube,
  Translate,
  Trash,
  TriangleWarning,
  Tuning,
  UserCircle,
  Users,
  Video,
  VolumeHigh,
  Widget,
  Wifi,
  Wind,
  X,
} from "reicon-react";

/** Les dessins, par nom de l'application. */
const DESSINS = {
  archive: Archive,
  "arrow-clockwise": RotateRight,
  "arrow-counter-clockwise": RotateLeft,
  "arrow-down": ArrowDown,
  "arrow-left": ArrowLeft,
  "arrow-right": ArrowRight,
  "arrow-square-out": SquareArrowRightUp,
  "arrows-out-simple": Maximize,
  bell: Bell,
  "bell-ringing": BellRing,
  "bell-slash": BellOff,
  books: Library,
  brain: Lightbulb,
  bridge: Link,
  broadcast: Radar,
  broom: Broom,
  bug: Bug,
  "calendar-blank": Calendar,
  "caret-down": ChevronDown,
  "caret-right": ChevronRight,
  "chart-line": ChartLine,
  "chat-circle": ChatRound,
  check: Check,
  "check-circle": CheckCircle,
  "circle-notch": RefreshCircle,
  clock: ClockCircle,
  cloud: Cloud,
  code: Code,
  compass: Compass,
  copy: Copy,
  cpu: Cpu,
  crosshair: Target,
  cube: ThreeDCube,
  "cursor-click": Cursor,
  database: Database,
  "device-mobile": Mobile,
  devices: Devices,
  dot: Record,
  "dot-outline": RecordCircle,
  "dots-three": MoreH,
  "download-simple": Download,
  "drop-half": Drop,
  export: Export,
  eye: Eye,
  "eye-slash": EyeClosed,
  "file-text": FileText,
  "file-x": FileRemove,
  flask: TestTube,
  "folder-dashed": FolderAdd,
  "folder-open": FolderOpen,
  "folder-simple": Folder,
  folders: Folder2,
  gauge: Speedometer,
  gear: Settings,
  "gear-six": Setting2,
  "git-branch": Hierarchy,
  globe: Globe,
  "globe-simple": Earth,
  graph: Sitemap,
  "grid-four": Widget,
  "hard-drive": HardDrive,
  "hard-drives": Server,
  hourglass: Hourglass,
  image: Gallery,
  info: InfoCircle,
  key: Key,
  laptop: Laptop,
  lightning: Flash,
  "list-bullets": List,
  lock: Lock,
  "lock-open": LockOpen,
  "magnifying-glass": Magnifier,
  microphone: Microphone,
  minus: Minus,
  monitor: Monitor,
  moon: Moon,
  "music-notes": MusicNote,
  notebook: Notebook,
  palette: Palette,
  "pause-circle": PauseCircle,
  "pencil-simple": Pen,
  plugs: Plug,
  "plugs-connected": PlugCircle,
  plus: Plus,
  "push-pin": Pin,
  "qr-code": Qr,
  question: HelpCircle,
  robot: Microchip,
  scroll: Scroll,
  shapes: Shapes,
  "shield-check": ShieldCheck,
  "sidebar-simple": SidebarLeft,
  sliders: Tuning,
  sparkle: MagicStar,
  "speaker-high": VolumeHigh,
  square: Stop,
  stack: Layers,
  star: Star,
  storefront: Shop,
  sun: Sun,
  "terminal-window": TerminalSquare,
  "text-aa": Text,
  textbox: TextInput,
  toolbox: Box,
  translate: Translate,
  trash: Trash,
  "user-circle": UserCircle,
  "users-three": Users,
  "video-camera": Video,
  warning: TriangleWarning,
  "wave-sine": Soundwave,
  "wifi-high": Wifi,
  wind: Wind,
  wrench: Setting3,
  x: X,
  "x-circle": CloseCircle,
} satisfies Record<string, IconComponent>;

/** Un logo de marque, absent de Reicon : le seul tracé gardé à part (grille 256). */
const LOGOS = {
  "github-logo":
    "M208.31,75.68A59.78,59.78,0,0,0,202.93,28,8,8,0,0,0,196,24a59.75,59.75,0,0,0-48,24H124A59.75,59.75,0,0,0,76,24a8,8,0,0,0-6.93,4,59.78,59.78,0,0,0-5.38,47.68A58.14,58.14,0,0,0,56,104v8a56.06,56.06,0,0,0,48.44,55.47A39.8,39.8,0,0,0,96,192v8H72a24,24,0,0,1-24-24A40,40,0,0,0,8,136a8,8,0,0,0,0,16,24,24,0,0,1,24,24,40,40,0,0,0,40,40H96v16a8,8,0,0,0,16,0V192a24,24,0,0,1,48,0v40a8,8,0,0,0,16,0V192a39.8,39.8,0,0,0-8.44-24.53A56.06,56.06,0,0,0,216,112v-8A58.14,58.14,0,0,0,208.31,75.68ZM200,112a40,40,0,0,1-40,40H112a40,40,0,0,1-40-40v-8a41.74,41.74,0,0,1,6.9-22.48A8,8,0,0,0,80,73.83a43.81,43.81,0,0,1,.79-33.58,43.88,43.88,0,0,1,32.32,20.06A8,8,0,0,0,119.82,64h32.35a8,8,0,0,0,6.74-3.69,43.87,43.87,0,0,1,32.32-20.06A43.81,43.81,0,0,1,192,73.83a8.09,8.09,0,0,0,1,7.65A41.72,41.72,0,0,1,200,104Z",
} as const;

/**
 * Les noms d'usage de l'application, vers leur dessin.
 *
 * Garder ces noms évite de renommer des dizaines d'appels pour un changement
 * de jeu, et garde le vocabulaire métier : `private` dit l'intention,
 * `eye-slash` dit le dessin.
 */
const ALIASES = {
  back: "arrow-left",
  calendar: "calendar-blank",
  chart: "chart-line",
  chat: "chat-circle",
  chevron: "caret-right",
  close: "x",
  download: "download-simple",
  edit: "pencil-simple",
  extensions: "plugs",
  figures: "users-three",
  forward: "arrow-right",
  marketplace: "storefront",
  memory: "brain",
  menu: "sidebar-simple",
  mic: "microphone",
  models: "cube",
  more: "dots-three",
  music: "music-notes",
  private: "eye-slash",
  project: "folder-simple",
  refresh: "arrow-clockwise",
  search: "magnifying-glass",
  server: "hard-drives",
  settings: "gear",
  shield: "shield-check",
  sound: "speaker-high",
  speed: "gauge",
  studio: "sparkle",
  target: "crosshair",
  video: "video-camera",
};

/** L'union réelle des noms acceptés : un nom fautif ne compile pas. */
export type IconName = keyof typeof DESSINS | keyof typeof LOGOS | keyof typeof ALIASES;

/**
 * Vrai si `name` désigne une icône du jeu.
 *
 * Une extension nomme une icône dans son manifeste ; l'application la dessine.
 * Un nom inconnu n'est pas une icône, et vaut alors une icône de secours côté
 * appelant plutôt qu'un rendu vide.
 */
export function isIconName(name: string | null | undefined): name is IconName {
  if (!name) return false;
  return name in DESSINS || name in LOGOS || name in ALIASES;
}

type Props = {
  name: IconName;
  /** Taille en pixels ; carré. 16 en ligne, 20 en navigation, 24 pour un en-tête. */
  size?: number;
  /** Titre accessible. Absent : l'icône est décorative et reste muette. */
  title?: string;
};

export function Icon({ name, size = 20, title }: Props) {
  const dessin = name in ALIASES ? ALIASES[name as keyof typeof ALIASES] : name;
  // Une icône sans titre accompagne toujours un mot : la lire à voix haute ne
  // ferait que répéter.
  const a11y = title
    ? { role: "img" as const, "aria-label": title }
    : { "aria-hidden": true as const };
  const Composant = DESSINS[dessin as keyof typeof DESSINS];
  if (Composant) {
    return <Composant size={size} focusable={false} {...a11y} />;
  }
  const d = LOGOS[dessin as keyof typeof LOGOS];
  // Un nom inconnu ne dessine rien plutôt qu'un carré de secours : la place
  // reste tenue, et l'appelant garde la main sur ce qu'il montre à la place.
  if (!d) return null;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 256 256"
      fill="currentColor"
      focusable={false}
      {...a11y}
    >
      {title ? <title>{title}</title> : null}
      <path d={d} />
    </svg>
  );
}
