import { Icon } from "@locaryn/ui-core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { type BrowserState, core, isTauri } from "../../lib/core";
import { suivreRecouvrement } from "../../lib/nativeViewCover";

/** La page de chaque conversation. La vue native est unique : quitter une
 *  conversation la cache, y revenir rouvre sa page. */
const pagesParConversation = new Map<string, string>();

function estUnePage(url: string): boolean {
  return url !== "" && url !== "about:blank";
}

type Props = {
  /** L'onglet est celui qu'on regarde, et le panneau est à l'écran. */
  visible: boolean;
  /** La conversation affichée. */
  sessionId: string | null;
};

/**
 * Le navigateur intégré. La page elle-même est une vue web native que le
 * service pose exactement sur la zone réservée ici ; cette zone la suit quand
 * le panneau change de taille, et la cache dès qu'elle ne doit plus se voir.
 */
export function BrowserTab({ visible, sessionId }: Props) {
  const conversation = sessionId ?? "";
  const zone = useRef<HTMLDivElement | null>(null);
  const [etat, setEtat] = useState<BrowserState>({ url: "", title: "", loading: false });
  const [saisie, setSaisie] = useState("");
  const [recouvert, setRecouvert] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);
  const editionRef = useRef(false);
  /** La conversation à qui appartient la page affichée par la vue. */
  const [proprio, setProprio] = useState<string | null>(null);
  const proprioRef = useRef<string | null>(null);
  const conversationRef = useRef(conversation);

  const changerProprio = useCallback((qui: string | null) => {
    proprioRef.current = qui;
    setProprio(qui);
  }, []);

  // Une nouvelle adresse appartient à la conversation affichée ; une page déjà
  // connue (fin de chargement, retour sur une conversation) ne change rien.
  const recevoir = useCallback(
    (e: BrowserState) => {
      if (estUnePage(e.url)) {
        const actuel = proprioRef.current;
        if (actuel === null || pagesParConversation.get(actuel) !== e.url) {
          changerProprio(conversationRef.current);
          pagesParConversation.set(conversationRef.current, e.url);
        }
      }
      setEtat(e);
      if (!editionRef.current && proprioRef.current === conversationRef.current) {
        setSaisie(estUnePage(e.url) ? e.url : "");
      }
    },
    [changerProprio],
  );

  // L'état de la page, poussé par le service à chaque chargement et titre.
  useEffect(() => {
    if (!isTauri) return;
    let fin: (() => void) | null = null;
    let vivant = true;
    void core.browserState().then((e) => {
      if (vivant) recevoir(e);
    });
    void listen<BrowserState>("locaryn://browser", (ev) => recevoir(ev.payload)).then((un) => {
      if (vivant) fin = un;
      else un();
    });
    return () => {
      vivant = false;
      fin?.();
    };
  }, [recevoir]);

  // Changer de conversation : la page de l'autre se cache (elle n'est plus à
  // nous), et celle de cette conversation, s'il y en a une, revient.
  useEffect(() => {
    conversationRef.current = conversation;
    editionRef.current = false;
    setErreur(null);
    const page = pagesParConversation.get(conversation);
    setSaisie(page ?? "");
    if (!page || proprioRef.current === conversation) return;
    changerProprio(conversation);
    void core.browserNavigate(page).catch((e: unknown) => console.warn("navigateur :", e));
  }, [conversation, changerProprio]);

  // Quoi que ce soit par-dessus la place de la page (fenêtre, menu,
  // notification, infobulle) ou une séparation qu'on tire : la page s'efface
  // le temps que cela dure. Une vue native se dessinerait par-dessus.
  useEffect(
    () =>
      suivreRecouvrement(
        () => zone.current,
        (r) => setRecouvert(r),
      ),
    [],
  );

  const cadre = useCallback(() => {
    const r = zone.current?.getBoundingClientRect();
    if (!r || r.width < 2 || r.height < 2) return null;
    return { x: r.left, y: r.top, width: r.width, height: r.height };
  }, []);

  // Avant la première adresse, pas de vue : une page blanche cacherait le mot
  // d'accueil.
  // La page doit aussi être celle de cette conversation, chargée : pas celle
  // qu'on vient de quitter, le temps que la sienne revienne.
  const aUnePage =
    estUnePage(etat.url) &&
    proprio === conversation &&
    pagesParConversation.get(conversation) === etat.url;
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
    // La page tapée ici est celle de cette conversation. La première adresse
    // crée la vue, cachée ; elle se montre dès que la page arrive.
    changerProprio(conversationRef.current);
    pagesParConversation.delete(conversationRef.current);
    void core
      .browserNavigate(saisie)
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
