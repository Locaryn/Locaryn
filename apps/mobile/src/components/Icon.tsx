type Name =
  | "menu"
  | "more"
  | "back"
  | "private"
  | "studio"
  | "extensions"
  | "models"
  | "settings"
  | "memory"
  | "chevron";

type Props = {
  name: Name;
  /** Taille en pixels ; carré. */
  size?: number;
};

/**
 * Les icônes de l'application.
 *
 * Dessinées, pas empruntées à la police d'émojis du système : un émoji est
 * coloré, arrondi, et change de dessin d'un téléphone à l'autre — trois façons
 * de jurer avec une interface sobre. Ici tout est au trait, d'une seule
 * épaisseur, et prend la couleur du texte : les icônes se fondent dans
 * l'interface au lieu de s'y poser.
 */
export function Icon({ name, size = 20 }: Props) {
  const common = {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.6,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
    focusable: false,
  };

  switch (name) {
    case "menu":
      return (
        <svg {...common}>
          <title>Menu</title>
          <path d="M4 7h16M4 12h16M4 17h16" />
        </svg>
      );
    case "more":
      return (
        <svg {...common}>
          <title>Plus</title>
          <circle cx="12" cy="5" r="1.4" fill="currentColor" stroke="none" />
          <circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none" />
          <circle cx="12" cy="19" r="1.4" fill="currentColor" stroke="none" />
        </svg>
      );
    case "back":
      return (
        <svg {...common}>
          <title>Retour</title>
          <path d="M15 5l-7 7 7 7" />
        </svg>
      );
    // Un masque : ce qui couvre un visage dit « privé » sans mot ni cadenas,
    // et le cadenas voudrait dire « chiffré », ce qui n'est pas la même chose.
    case "private":
      return (
        <svg {...common}>
          <title>Éphémère</title>
          <path d="M3 9h18M5 9c-.6 2.6-.4 5 .6 6.2 1.2 1.4 3.7 1.2 4.6-.4.4-.7.6-1.4.8-2.3M19 9c.6 2.6.4 5-.6 6.2-1.2 1.4-3.7 1.2-4.6-.4-.4-.7-.6-1.4-.8-2.3" />
          <path d="M7 9V7.4C7 6.6 7.6 6 8.4 6h7.2c.8 0 1.4.6 1.4 1.4V9" />
        </svg>
      );
    case "studio":
      return (
        <svg {...common}>
          <title>Studio</title>
          <rect x="3" y="4" width="18" height="14" rx="2" />
          <circle cx="8.5" cy="9" r="1.5" />
          <path d="M3 15l4.5-4 4 3.5L15 11l6 5" />
        </svg>
      );
    case "extensions":
      return (
        <svg {...common}>
          <title>Extensions</title>
          <path d="M9 4h6v2.5a1.8 1.8 0 103.6 0H21v6h-2.5a1.8 1.8 0 100 3.6V20H9v-2.5a1.8 1.8 0 10-3.6 0H3v-6h2.5a1.8 1.8 0 100-3.6H3V4h6" />
        </svg>
      );
    case "models":
      return (
        <svg {...common}>
          <title>Modèles</title>
          <path d="M12 3l8 4.5v9L12 21l-8-4.5v-9L12 3z" />
          <path d="M4 7.5l8 4.5 8-4.5M12 12v9" />
        </svg>
      );
    case "settings":
      return (
        <svg {...common}>
          <title>Réglages</title>
          <path d="M5 7h14M5 12h14M5 17h14" />
          <circle cx="9" cy="7" r="2" fill="var(--bg)" />
          <circle cx="15" cy="12" r="2" fill="var(--bg)" />
          <circle cx="10" cy="17" r="2" fill="var(--bg)" />
        </svg>
      );
    case "memory":
      return (
        <svg {...common}>
          <title>Mémoire</title>
          <path d="M12 5a4 4 0 00-4 4v.4A3 3 0 007 15v.5A3.5 3.5 0 0012 18a3.5 3.5 0 005-2.5V15a3 3 0 00-1-5.6V9a4 4 0 00-4-4z" />
          <path d="M12 5v13" />
        </svg>
      );
    case "chevron":
      return (
        <svg {...common}>
          <title>Ouvrir</title>
          <path d="M9 5l7 7-7 7" />
        </svg>
      );
  }
}
