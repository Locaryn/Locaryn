import { Icon } from "@locaryn/ui-core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { type BrowserState, core, isTauri } from "../../lib/core";
import { suivreRecouvrement } from "../../lib/nativeViewCover";

function estUnePage(url: string): boolean {
  return url !== "" && url !== "about:blank";
}

type Props = {
  /** L'onglet est celui qu'on regarde, et le panneau est à l'écran. */
  visible: boolean;
  /** La conversation à qui appartient ce navigateur. */
  sessionId: string | null;
};

/**
 * Le navigateur intégré d'une conversation. La page elle-même est une vue web
 * native, une par conversation, que le service pose exactement sur la zone
 * réservée ici ; cette zone la suit quand le panneau change de taille, et la
 * cache dès qu'elle ne doit plus se voir. Cachée, la vue reste chargée : le
 * modèle de la conversation continue de s'en servir en arrière-plan.
 */
export function BrowserTab({ visible, sessionId }: Props) {
  const conversation = sessionId ?? "";
  const zone = useRef<HTMLDivElement | null>(null);
  const [etat, setEtat] = useState<BrowserState>({ url: "", title: "", loading: false });
  const [saisie, setSaisie] = useState("");
  const [recouvert, setRecouvert] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);
  const editionRef = useRef(false);

  // L'état de la page de cette conversation, poussé par le service à chaque
  // chargement et titre ; celui des autres conversations ne nous regarde pas.
  useEffect(() => {
    if (!isTauri) return;
    let fin: (() => void) | null = null;
    let vivant = true;
    const recevoir = (e: BrowserState) => {
      setEtat(e);
      if (!editionRef.current) setSaisie(estUnePage(e.url) ? e.url : "");
    };
    void core.browserState(sessionId).then((e) => {
      if (vivant) recevoir(e);
    });
    void listen<BrowserState>("locaryn://browser", (ev) => {
      if ((ev.payload.session ?? "") === conversation) recevoir(ev.payload);
    }).then((un) => {
      if (vivant) fin = un;
      else un();
    });
    return () => {
      vivant = false;
      fin?.();
    };
  }, [sessionId, conversation]);

  // Quoi que ce soit par-dessus la place de la page (fenêtre, menu,
  // notification, infobulle) ou une séparation qu'on tire : la page s'efface
  // le temps que cela dure. Une vue native se dessinerait par-dessus.
  useEffect(() => {
    if (!visible) return;
    return suivreRecouvrement(
      () => zone.current,
      (r) => setRecouvert(r),
    );
  }, [visible]);

  const cadre = useCallback(() => {
    const r = zone.current?.getBoundingClientRect();
    if (!r || r.width < 2 || r.height < 2) return null;
    return { x: r.left, y: r.top, width: r.width, height: r.height };
  }, []);

  // Avant la première adresse, pas de vue : une page blanche cacherait le mot
  // d'accueil.
  const aUnePage = estUnePage(etat.url);
  const montrer = visible && !recouvert && aUnePage;

  // Montrer, cacher (sans fermer), suivre la place.
  useEffect(() => {
    if (!isTauri) return;
    if (!montrer) {
      void core.browserHide(sessionId).catch((e: unknown) => console.warn("navigateur :", e));
      return;
    }
    const c = cadre();
    if (c) {
      void core
        .browserShow(sessionId, c)
        .then(setEtat)
        .catch((e: unknown) => setErreur(String(e)));
    }
    const suivre = () => {
      const n = cadre();
      if (n) {
        void core
          .browserBounds(sessionId, n)
          .catch((e: unknown) => console.warn("navigateur :", e));
      }
    };
    const obs = new ResizeObserver(suivre);
    if (zone.current) obs.observe(zone.current);
    window.addEventListener("resize", suivre);
    return () => {
      obs.disconnect();
      window.removeEventListener("resize", suivre);
    };
  }, [montrer, cadre, sessionId]);

  // L'onglet se ferme : la page quitte l'écran, mais reste chargée.
  useEffect(
    () => () => {
      if (isTauri) {
        void core.browserHide(sessionId).catch((e: unknown) => console.warn("navigateur :", e));
      }
    },
    [sessionId],
  );

  function aller(e: React.FormEvent) {
    e.preventDefault();
    editionRef.current = false;
    setErreur(null);
    // La première adresse crée la vue, cachée ; elle se montre dès que la
    // page arrive.
    void core
      .browserNavigate(sessionId, saisie)
      .catch((err: unknown) => setErreur(String(err).replace(/^Error:\s*/, "")));
  }

  function historique(action: "back" | "forward" | "reload") {
    void core.browserHistory(sessionId, action).catch((e: unknown) => setErreur(String(e)));
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
