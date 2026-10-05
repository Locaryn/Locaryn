import { Icon } from "@locaryn/ui-core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { type BrowserState, core, isTauri } from "../../lib/core";

/** Ce qui, posé par-dessus l'interface, doit passer devant la page : la vue
 *  web native se dessine au-dessus de tout le reste, elle se cache donc. */
const PAR_DESSUS = 'dialog[open], [role="dialog"], [role="alertdialog"], [aria-modal="true"]';

type Props = {
  /** L'onglet est celui qu'on regarde, et le panneau est à l'écran. */
  visible: boolean;
};

/**
 * Le navigateur intégré. La page elle-même est une vue web native que le
 * service pose exactement sur la zone réservée ici ; cette zone la suit quand
 * le panneau change de taille, et la cache dès qu'elle ne doit plus se voir.
 */
export function BrowserTab({ visible }: Props) {
  const zone = useRef<HTMLDivElement | null>(null);
  const [etat, setEtat] = useState<BrowserState>({ url: "", title: "", loading: false });
  const [saisie, setSaisie] = useState("");
  const [recouvert, setRecouvert] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);
  const editionRef = useRef(false);

  // L'état de la page, poussé par le service à chaque chargement et titre.
  useEffect(() => {
    if (!isTauri) return;
    let fin: (() => void) | null = null;
    let vivant = true;
    void core.browserState().then((e) => {
      if (!vivant) return;
      setEtat(e);
      if (!editionRef.current) setSaisie(e.url === "about:blank" ? "" : e.url);
    });
    void listen<BrowserState>("locaryn://browser", (ev) => {
      setEtat(ev.payload);
      if (!editionRef.current) setSaisie(ev.payload.url === "about:blank" ? "" : ev.payload.url);
    }).then((un) => {
      if (vivant) fin = un;
      else un();
    });
    return () => {
      vivant = false;
      fin?.();
    };
  }, []);

  // Une fenêtre par-dessus l'interface, ou une séparation qu'on tire : la
  // page s'efface le temps que cela dure. Une vue native capterait la souris
  // au passage et couvrirait la fenêtre.
  useEffect(() => {
    const verifier = () =>
      setRecouvert(
        document.querySelector(PAR_DESSUS) !== null ||
          document.body.classList.contains("locaryn-resizing"),
      );
    verifier();
    const obs = new MutationObserver(verifier);
    obs.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["open", "role", "aria-modal"],
    });
    const corps = new MutationObserver(verifier);
    corps.observe(document.body, { attributes: true, attributeFilter: ["class"] });
    return () => {
      obs.disconnect();
      corps.disconnect();
    };
  }, []);

  const cadre = useCallback(() => {
    const r = zone.current?.getBoundingClientRect();
    if (!r || r.width < 2 || r.height < 2) return null;
    return { x: r.left, y: r.top, width: r.width, height: r.height };
  }, []);

  // Avant la première adresse, pas de vue : une page blanche cacherait le mot
  // d'accueil.
  const aUnePage = etat.url !== "" && etat.url !== "about:blank";
  const montrer = visible && !recouvert && aUnePage;

  // Montrer, cacher, suivre la place.
  useEffect(() => {
    if (!isTauri) return;
    if (!montrer) {
      void core.browserHide().catch((e: unknown) => console.warn("navigateur :", e));
      return;
    }
    const c = cadre();
    if (c) {
      void core
        .browserShow(c)
        .then(setEtat)
        .catch((e: unknown) => setErreur(String(e)));
    }
    const suivre = () => {
      const n = cadre();
      if (n) void core.browserBounds(n).catch((e: unknown) => console.warn("navigateur :", e));
    };
    const obs = new ResizeObserver(suivre);
    if (zone.current) obs.observe(zone.current);
    window.addEventListener("resize", suivre);
    return () => {
      obs.disconnect();
      window.removeEventListener("resize", suivre);
    };
  }, [montrer, cadre]);

  // Le panneau se démonte (fermeture de l'onglet) : la page ne reste pas à l'écran.
  useEffect(
    () => () => {
      if (isTauri) void core.browserHide().catch((e: unknown) => console.warn("navigateur :", e));
    },
    [],
  );

  function aller(e: React.FormEvent) {
    e.preventDefault();
    editionRef.current = false;
    setErreur(null);
    const c = cadre();
    if (!c) return;
    // La première adresse crée la vue, cachée ; elle se montre à sa place.
    void core
      .browserNavigate(saisie)
      .then(() => core.browserShow(c))
      .then(setEtat)
      .catch((err: unknown) => setErreur(String(err).replace(/^Error:\s*/, "")));
  }

  function historique(action: "back" | "forward" | "reload") {
    void core.browserHistory(action).catch((e: unknown) => setErreur(String(e)));
  }

  if (!isTauri) {
    return (
      <div className="lw-empty">
        <Icon name="globe" size={22} />
        <p>Le navigateur intégré s'ouvre dans l'application de bureau.</p>
      </div>
    );
  }

  return (
    <div className="lw-browser">
      <form className="lw-browser-bar" onSubmit={aller}>
        <button
          type="button"
          className="lw-icon-btn"
          aria-label="Page précédente"
          title="Page précédente"
          onClick={() => historique("back")}
        >
          <Icon name="arrow-left" size={15} />
        </button>
        <button
          type="button"
          className="lw-icon-btn"
          aria-label="Page suivante"
          title="Page suivante"
          onClick={() => historique("forward")}
        >
          <Icon name="arrow-right" size={15} />
        </button>
        <button
          type="button"
          className="lw-icon-btn"
          aria-label="Recharger"
          title="Recharger"
          onClick={() => historique("reload")}
        >
          <span className={etat.loading ? "locaryn-spin lw-spin" : "lw-spin"}>
            <Icon name="arrow-clockwise" size={15} />
          </span>
        </button>
        <input
          className="lw-browser-url"
          value={saisie}
          placeholder="Adresse ou recherche"
          aria-label="Adresse ou recherche"
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          onFocus={(e) => {
            editionRef.current = true;
            e.currentTarget.select();
          }}
          onBlur={() => {
            editionRef.current = false;
          }}
          onChange={(e) => setSaisie(e.target.value)}
        />
      </form>
      {erreur && (
        <p className="lw-browser-error" role="alert">
          {erreur}
        </p>
      )}
      <div className="lw-browser-view" ref={zone}>
        {!aUnePage ? (
          <div className="lw-empty">
            <Icon name="globe" size={22} />
            <p>
              Tapez une adresse ou une recherche. Le modèle peut aussi ouvrir, lire et remplir des
              pages ici.
            </p>
          </div>
        ) : null}
      </div>
    </div>
  );
}
