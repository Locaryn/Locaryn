/**
 * Les icônes de Locaryn.
 *
 * Un seul jeu, partagé par l'ordinateur et le téléphone : deux dessins pour la
 * même idée, c'est deux applications qui se ressemblent de loin et se
 * contredisent de près.
 *
 * Direction : tout est au trait, d'une seule épaisseur, sans remplissage ni
 * couleur propre — chaque icône prend `currentColor` et se pose donc au niveau
 * de gris du texte qu'elle accompagne. C'est ce qui les distingue d'une police
 * d'émojis : un émoji est coloré, arrondi, et change de dessin d'un appareil à
 * l'autre. Trois façons de jurer avec une interface sobre.
 *
 * Le nom dit la chose, pas le dessin : `private`, pas `masque` — le jour où le
 * masque devient autre chose, les appels ne mentent pas.
 */

export type IconName =
  // Navigation et chrome
  | "menu"
  | "more"
  | "back"
  | "forward"
  | "chevron"
  | "close"
  | "check"
  | "plus"
  // Espaces
  | "chat"
  | "studio"
  | "extensions"
  | "models"
  | "settings"
  | "memory"
  | "project"
  | "marketplace"
  // États et actions
  | "private"
  | "archive"
  | "trash"
  | "download"
  | "speed"
  | "warning"
  | "image"
  | "sound"
  | "server"
  | "shield"
  // Studio et catalogue
  | "mic"
  | "music"
  | "video"
  | "cube"
  | "edit"
  | "target"
  | "translate"
  | "chart"
  | "question"
  | "cloud"
  | "cpu"
  | "star"
  | "refresh"
  | "lock"
  | "figures";

type Props = {
  name: IconName;
  /** Taille en pixels ; carré. */
  size?: number;
  /** Titre accessible. Absent : l'icône est décorative et reste muette. */
  title?: string;
};

export function Icon({ name, size = 20, title }: Props) {
  const common = {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.6,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    // Une icône sans titre accompagne toujours un mot : la lire à voix haute
    // ne ferait que répéter.
    "aria-hidden": title ? undefined : true,
    role: title ? "img" : undefined,
    focusable: false,
  };
  const label = title ? <title>{title}</title> : null;

  switch (name) {
    case "menu":
      return (
        <svg {...common}>
          {label}
          <path d="M4 7h16M4 12h16M4 17h16" />
        </svg>
      );
    case "more":
      return (
        <svg {...common}>
          {label}
          <circle cx="12" cy="5" r="1.4" fill="currentColor" stroke="none" />
          <circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none" />
          <circle cx="12" cy="19" r="1.4" fill="currentColor" stroke="none" />
        </svg>
      );
    case "back":
      return (
        <svg {...common}>
          {label}
          <path d="M15 5l-7 7 7 7" />
        </svg>
      );
    case "forward":
      return (
        <svg {...common}>
          {label}
          <path d="M5 12h14M13 6l6 6-6 6" />
        </svg>
      );
    case "chevron":
      return (
        <svg {...common}>
          {label}
          <path d="M9 5l7 7-7 7" />
        </svg>
      );
    case "close":
      return (
        <svg {...common}>
          {label}
          <path d="M6 6l12 12M18 6L6 18" />
        </svg>
      );
    case "check":
      return (
        <svg {...common}>
          {label}
          <path d="M4 12.5l5 5L20 7" />
        </svg>
      );
    case "plus":
      return (
        <svg {...common}>
          {label}
          <path d="M12 5v14M5 12h14" />
        </svg>
      );
    case "chat":
      return (
        <svg {...common}>
          {label}
          <path d="M20 15a2 2 0 01-2 2H8l-4 4V6a2 2 0 012-2h12a2 2 0 012 2v9z" />
        </svg>
      );
    case "studio":
    case "image":
      return (
        <svg {...common}>
          {label}
          <rect x="3" y="4" width="18" height="14" rx="2" />
          <circle cx="8.5" cy="9" r="1.5" />
          <path d="M3 15l4.5-4 4 3.5L15 11l6 5" />
        </svg>
      );
    case "extensions":
      return (
        <svg {...common}>
          {label}
          <path d="M9 4h6v2.5a1.8 1.8 0 103.6 0H21v6h-2.5a1.8 1.8 0 100 3.6V20H9v-2.5a1.8 1.8 0 10-3.6 0H3v-6h2.5a1.8 1.8 0 100-3.6H3V4h6" />
        </svg>
      );
    case "models":
      return (
        <svg {...common}>
          {label}
          <path d="M12 3l8 4.5v9L12 21l-8-4.5v-9L12 3z" />
          <path d="M4 7.5l8 4.5 8-4.5M12 12v9" />
        </svg>
      );
    // Trois curseurs : des réglages qu'on déplace, pas un engrenage — rien ne
    // tourne dans une préférence.
    case "settings":
      return (
        <svg {...common}>
          {label}
          <path d="M5 7h14M5 12h14M5 17h14" />
          <circle cx="9" cy="7" r="2" fill="var(--bg, #0e1116)" />
          <circle cx="15" cy="12" r="2" fill="var(--bg, #0e1116)" />
          <circle cx="10" cy="17" r="2" fill="var(--bg, #0e1116)" />
        </svg>
      );
    case "memory":
      return (
        <svg {...common}>
          {label}
          <path d="M12 5a4 4 0 00-4 4v.4A3 3 0 007 15v.5A3.5 3.5 0 0012 18a3.5 3.5 0 005-2.5V15a3 3 0 00-1-5.6V9a4 4 0 00-4-4z" />
          <path d="M12 5v13" />
        </svg>
      );
    case "project":
      return (
        <svg {...common}>
          {label}
          <path d="M3 7a2 2 0 012-2h4l2 2.5h8a2 2 0 012 2V17a2 2 0 01-2 2H5a2 2 0 01-2-2V7z" />
        </svg>
      );
    case "marketplace":
      return (
        <svg {...common}>
          {label}
          <path d="M4 8h16l-1 10.5a2 2 0 01-2 1.5H7a2 2 0 01-2-1.5L4 8z" />
          <path d="M9 8V6a3 3 0 016 0v2" />
        </svg>
      );
    // Un masque : ce qui couvre un visage dit « privé » sans mot. Le cadenas
    // dirait « chiffré », qui n'est pas la même promesse.
    case "private":
      return (
        <svg {...common}>
          {label}
          <path d="M3 9h18M5 9c-.6 2.6-.4 5 .6 6.2 1.2 1.4 3.7 1.2 4.6-.4.4-.7.6-1.4.8-2.3M19 9c.6 2.6.4 5-.6 6.2-1.2 1.4-3.7 1.2-4.6-.4-.4-.7-.6-1.4-.8-2.3" />
          <path d="M7 9V7.4C7 6.6 7.6 6 8.4 6h7.2c.8 0 1.4.6 1.4 1.4V9" />
        </svg>
      );
    case "archive":
      return (
        <svg {...common}>
          {label}
          <rect x="3" y="4" width="18" height="4" rx="1" />
          <path d="M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8M10 12h4" />
        </svg>
      );
    case "trash":
      return (
        <svg {...common}>
          {label}
          <path d="M4 7h16M9 7V5.5A1.5 1.5 0 0110.5 4h3A1.5 1.5 0 0115 5.5V7" />
          <path d="M6 7l1 12a2 2 0 002 2h6a2 2 0 002-2l1-12" />
        </svg>
      );
    case "download":
      return (
        <svg {...common}>
          {label}
          <path d="M12 4v11M7.5 10.5L12 15l4.5-4.5" />
          <path d="M5 19h14" />
        </svg>
      );
    case "speed":
      return (
        <svg {...common}>
          {label}
          <path d="M13 3l-8 11h6l-1 7 8-11h-6l1-7z" />
        </svg>
      );
    case "warning":
      return (
        <svg {...common}>
          {label}
          <path d="M12 4l9 16H3l9-16z" />
          <path d="M12 10v4M12 17.2v.1" />
        </svg>
      );
    case "sound":
      return (
        <svg {...common}>
          {label}
          <path d="M5 9v6h3l4.5 4V5L8 9H5z" />
          <path d="M16 9.5a3.5 3.5 0 010 5M18.5 7a7 7 0 010 10" />
        </svg>
      );
    case "server":
      return (
        <svg {...common}>
          {label}
          <rect x="3" y="4" width="18" height="7" rx="2" />
          <rect x="3" y="13" width="18" height="7" rx="2" />
          <path d="M7 7.5v.1M7 16.5v.1" />
        </svg>
      );
    case "shield":
      return (
        <svg {...common}>
          {label}
          <path d="M12 3l7 3v6c0 4.4-3 7.7-7 9-4-1.3-7-4.6-7-9V6l7-3z" />
        </svg>
      );
    case "mic":
      return (
        <svg {...common}>
          {label}
          <rect x="9" y="3" width="6" height="11" rx="3" />
          <path d="M5 11a7 7 0 0014 0M12 18v3" />
        </svg>
      );
    case "music":
      return (
        <svg {...common}>
          {label}
          <path d="M9 18V6l10-2v12" />
          <circle cx="6.5" cy="18" r="2.5" />
          <circle cx="16.5" cy="16" r="2.5" />
        </svg>
      );
    case "video":
      return (
        <svg {...common}>
          {label}
          <rect x="3" y="6" width="12" height="12" rx="2" />
          <path d="M15 10.5l6-3.5v10l-6-3.5" />
        </svg>
      );
    case "cube":
      return (
        <svg {...common}>
          {label}
          <path d="M12 3l8 4.5v9L12 21l-8-4.5v-9L12 3z" />
        </svg>
      );
    case "edit":
      return (
        <svg {...common}>
          {label}
          <path d="M4 20h4L19 9a2.1 2.1 0 00-3-3L5 17v3z" />
          <path d="M14.5 6.5l3 3" />
        </svg>
      );
    case "target":
      return (
        <svg {...common}>
          {label}
          <circle cx="12" cy="12" r="8" />
          <circle cx="12" cy="12" r="3.2" />
        </svg>
      );
    case "translate":
      return (
        <svg {...common}>
          {label}
          <path d="M4 6h9M8.5 4v2M10.5 6c-.6 4-3 7-6.5 8.5M7 9.5c1 2.4 3 4.2 5.5 5" />
          <path d="M12 20l4-10 4 10M13.4 17h5.2" />
        </svg>
      );
    case "chart":
      return (
        <svg {...common}>
          {label}
          <path d="M4 20V4M4 20h16" />
          <path d="M8 17v-5M12 17V7M16 17v-8" />
        </svg>
      );
    case "question":
      return (
        <svg {...common}>
          {label}
          <circle cx="12" cy="12" r="8.5" />
          <path d="M9.6 9.4A2.5 2.5 0 0114.5 10c0 1.7-2.5 2-2.5 3.6M12 16.6v.1" />
        </svg>
      );
    case "cloud":
      return (
        <svg {...common}>
          {label}
          <path d="M7 18a4 4 0 01-.4-8A5.5 5.5 0 0117.4 10 3.6 3.6 0 0117 18H7z" />
        </svg>
      );
    case "cpu":
      return (
        <svg {...common}>
          {label}
          <rect x="7" y="7" width="10" height="10" rx="1.5" />
          <path d="M10 3v4M14 3v4M10 17v4M14 17v4M3 10h4M3 14h4M17 10h4M17 14h4" />
        </svg>
      );
    case "star":
      return (
        <svg {...common}>
          {label}
          <path d="M12 4l2.4 5 5.6.8-4 3.9 1 5.5-5-2.7-5 2.7 1-5.5-4-3.9 5.6-.8L12 4z" />
        </svg>
      );
    case "refresh":
      return (
        <svg {...common}>
          {label}
          <path d="M20 12a8 8 0 11-2.6-5.9M20 4v4h-4" />
        </svg>
      );
    case "lock":
      return (
        <svg {...common}>
          {label}
          <rect x="5" y="10" width="14" height="10" rx="2" />
          <path d="M8.5 10V7.5a3.5 3.5 0 017 0V10" />
        </svg>
      );
    // Deux masques qui se recouvrent : un rôle qu'on endosse, et plusieurs
    // qu'on agence. Le mot « figure » dit les deux, le dessin aussi.
    case "figures":
      return (
        <svg {...common}>
          {label}
          <path d="M3 7h9v4.5A4.5 4.5 0 013 11.5V7z" />
          <path d="M12 7h9v4.5a4.5 4.5 0 01-9 0V7z" />
          <path d="M6 15.5c1.6 1.4 3.4 1.4 5 0M13 15.5c1.6 1.4 3.4 1.4 5 0" />
        </svg>
      );
  }
}
