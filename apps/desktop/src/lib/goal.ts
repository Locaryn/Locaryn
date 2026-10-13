// `/objectif` : un objectif n'est pas une suite de tours, c'est un plan qu'on
// exécute.
//
// Le modèle découpe d'abord l'objectif en parties, puis chaque partie en tâches
// courtes (`core.planTask`) ; les tâches sont ensuite menées l'une après
// l'autre, et chacune est jugée pour elle-même (`core.judgeGoal`). Une tâche qui
// n'avance plus est laissée de côté sans consommer le budget des suivantes, et
// la vérification finale peut renvoyer à la tâche fautive : on reprend
// celle-là et la suite, au lieu de tout recommencer — c'est ce qui distingue un
// objectif d'une boucle de tours.
//
// Ni le nombre de tours, ni le nombre de reprises, ni le temps ne bornent une
// tâche : elle continue tant qu'elle n'est pas finie, et c'est la personne qui
// arrête quand c'est trop long. Une seule chose coupe automatiquement : la
// vraie boucle — le modèle qui redit la même réponse sans agir. Celle-là
// n'abandonne pas la tâche : elle la coupe et la relance avec l'ordre de
// changer de méthode.

import { type StreamEvent, core } from "./core";
import { taskCenter } from "./taskCenter";

/** Tâches au plus après découpage : un plan qui explose n'est pas un plan. */
const TACHES_MAX = 24;

/**
 * Redites identiques et sans action à partir desquelles une tâche tourne
 * réellement en rond.
 *
 * C'est la seule coupure automatique d'une tâche : ni le nombre de tours ni le
 * nombre de reprises ne la bornent, et aucun délai ne l'arrête — elle continue
 * tant qu'elle n'est pas finie, et c'est la personne qui arrête quand c'est
 * trop long.
 */
const REDITES_BOUCLE = 2;

/** Source de l'avis d'un tour fini sans réponse (`SOURCE_TOUR_INACHEVE`). */
const TOUR_INACHEVE = "tour_inacheve";

/** Source de l'avis qui annonce le compte rendu d'un abandon. */
const TOUR_BILAN = "objectif_bilan";

/** Source de l'avis qui annonce qu'une tâche tourne en rond et change de méthode. */
const TOUR_BOUCLE = "objectif_boucle";

/** Ce qu'on dit au modele quand il redit la meme chose : l'acte, pas l'intention. */
const INSISTANCE = [
  "[Objectif] Tu as redit la meme chose au lieu d'agir. N'annonce plus ce que tu vas faire : ",
  "appelle maintenant un outil (fichier, terminal, connecteur MCP...) et reponds avec ce qui ",
  "en resulte — le fichier ecrit, la commande lancee, le resultat obtenu. Si l'outil necessaire ",
  "manque ou ne repond pas, dis lequel et pourquoi : ne decris pas une intention.",
].join("");

/**
 * Ce qu'on dit au modèle quand il tourne réellement en rond : la tâche est
 * coupée, et il repart avec l'ordre de changer de méthode plutôt que de redire
 * la même chose. Sans cet ordre, couper ne ferait que relancer la même réponse.
 */
const CHANGEMENT_METHODE = [
  "[Objectif] Tu tournes en rond : tu répètes la même réponse sans agir, et rien n'avance. ",
  "La tentative vient d'être coupée. Ne reprends pas la même approche : change de méthode. ",
  "Prends un autre chemin — un autre outil, une autre décomposition, un moyen de vérification ",
  "plus simple — ou dis nommément ce qui te bloque (un outil ou un connecteur qui ne répond pas, ",
  "un droit refusé, une information qui manque). N'annonce rien : agis, et réponds par ce qui en ",
  "résulte.",
].join("");

/** Un appel d'outil qui a échoué pendant l'objectif : le compte rendu s'appuie dessus. */
interface EchecOutil {
  tool: string;
  output: string;
}

/** Une tâche du plan : c'est elle qui avance, pas un « tour ». */
export interface Tache {
  /** L'intitulé, à l'impératif, tel que le planificateur l'a écrit. */
  titre: string;
  /** La partie du plan dont elle vient : la carte les montre groupées. */
  partie: string;
  etat: "a_faire" | "en_cours" | "fait" | "echec";
}

export interface GoalHooks {
  onEvent: (ev: StreamEvent) => void;
  /** Le plan est établi : les tâches dans l'ordre, la dernière étant la vérification. */
  onPlan: (taches: Tache[], parties: number) => void;
  /** Une tâche commence. */
  onTache: (index: number) => void;
  /** Une tâche se termine ; `reste` explique un échec, et n'est pas dit sur un succès. */
  onTacheFin: (index: number, etat: "fait" | "echec", reste?: string) => void;
  /** La vérification renvoie à une tâche : elle sera reprise, avec la suite. */
  onRenvoi?: (index: number, raison: string, essai: number) => void;
  /** Vrai quand la personne a demandé l'arrêt. */
  arret: () => boolean;
}

/** La première ligne d'une sortie d'outil, bornée : un compte rendu se lit. */
function premiereLigne(texte: string): string {
  const ligne = texte.trim().split("\n")[0]?.trim() ?? "";
  return ligne.length > 200 ? `${ligne.slice(0, 200)}…` : ligne;
}

/** Ce que le modèle a écrit, sans sa réflexion : le juge lisait la fin d'un
 *  bloc `think` et la prenait pour un bilan. */
export function sansReflexion(texte: string): string {
  return texte
    .replace(/<think[\s\S]*?<\/think>/g, "")
    .replace(/<think[\s\S]*$/, "")
    .trim();
}

/**
 * Deux réponses disent-elles la même chose ?
 *
 * Le juge ne suffit pas à repérer un objectif qui tourne : il varie sa formule
 * (« le jeu n'est pas construit », puis « la réponse doit décrire l'état
 * actuel ») alors que le modèle, lui, redit la même phrase d'un tour à
 * l'autre — parfois mot pour mot. On compare donc ce que le modèle a écrit, en
 * ignorant ponctuation, emoji et casse.
 */
function memeReponse(a: string, b: string): boolean {
  const cle = (s: string) =>
    s
      .toLowerCase()
      .replace(/\p{Extended_Pictographic}|\uFE0F|\u200D/gu, " ")
      .replace(/[^\p{L}\p{N}]+/gu, " ")
      .replace(/\s+/g, " ")
      .trim();
  const ca = cle(a);
  const cb = cle(b);
  if (ca === "" || cb === "") return false;
  // Redite mot pour mot : « D'abord, je vérifie l'état du studio : » deux fois
  // de suite ne dit rien d'autre qu'une boucle. Deux mots (« ok ») ne prouvent
  // rien.
  if (ca === cb) return ca.length >= 12;
  // Reformulée à la marge : il faut de la matière pour comparer sans se tromper.
  if (ca.length < 40 || cb.length < 40) return false;
  const mots = (s: string) => new Set(s.split(" ").filter((m) => m.length > 3));
  const ma = mots(ca);
  const mb = mots(cb);
  if (ma.size === 0 || mb.size === 0) return false;
  let communs = 0;
  for (const m of ma) if (mb.has(m)) communs++;
  return communs / Math.max(ma.size, mb.size) >= 0.8;
}

/** Où en est une tâche, en une puce, dans un plan qu'on relit. */
function marqueEtat(etat: Tache["etat"]): string {
  if (etat === "fait") return "✓";
  if (etat === "echec") return "✗";
  if (etat === "en_cours") return "→";
  return "·";
}

/** Le plan relu par le modèle à chaque message : ce qui est fait, ce qui reste. */
function lirePlan(taches: Tache[]): string {
  return taches.map((t, i) => `  ${i + 1}. ${marqueEtat(t.etat)} ${t.titre}`).join("\n");
}

/** Ce qu'on demande au modèle pour mener une tâche. */
function consigneTache(goal: string, taches: Tache[], index: number): string {
  const t = taches[index];
  return [
    `[Objectif] « ${goal} »`,
    "",
    "Tu avances par tâches, pas par conversation. Plan et état des tâches :",
    lirePlan(taches),
    "",
    `Tâche à mener maintenant — n° ${index + 1} : ${t.titre}`,
    `(partie « ${t.partie} »)`,
    "",
    "Mène-la complètement avec tes outils, puis vérifie-la toi-même.",
    "Ne traite pas les tâches suivantes : tu seras relancé pour elles.",
    "N'annonce pas ce que tu vas faire et n'attends aucune confirmation : fais-le, et réponds par ce qui en résulte (fichiers écrits, commandes lancées, résultat obtenu). Si un outil indispensable manque ou ne répond pas, dis lequel.",
  ].join("\n");
}

/** Le message qui relance la tâche en cours, d'après ce qui manque. */
function relanceTache(reste: string): string {
  return [
    `[Objectif] Cette tâche n'est pas terminée. Il reste : ${reste}`,
    "Continue sans demander de confirmation, et réponds par ce que tu as réellement fait.",
  ].join("\n");
}

/** Le message qui relance la vérification quand elle n'a pas conclu. */
function relanceVerification(reste: string): string {
  return [
    `[Objectif] La vérification n'a pas conclu : ${reste}`,
    "Termine par exactement deux lignes : « OBJECTIF: atteint » ou « OBJECTIF: non atteint », puis « RENVOI: » suivi du numéro de la tâche à reprendre — ou « RENVOI: aucun ».",
  ].join("\n");
}

/**
 * Ce qu'on demande au modèle pour la vérification finale.
 *
 * C'est la tâche qui décide de la suite : elle dit si l'objectif est atteint, et
 * sinon **quelle tâche** doit être reprise. Sans cette ligne, un échec ne
 * pouvait que tout relancer ou tout arrêter.
 */
function consigneVerification(goal: string, taches: Tache[], index: number): string {
  const areprendre = taches.length - 1;
  return [
    `[Objectif] « ${goal} »`,
    "",
    "C'est la vérification finale. Ne te fie pas à ce qui a été annoncé : constate par toi-même.",
    "1. lance réellement les vérifications possibles (tests, commandes, ouverture des fichiers produits) ;",
    "2. vérifie que les morceaux faits séparément vont ensemble ;",
    "3. si quelque chose ne va pas, dis laquelle des tâches doit être reprise — c'est elle qui sera relancée, pas tout l'objectif.",
    "",
    "Plan et état des tâches :",
    lirePlan(taches),
    "",
    `Tâche en cours — n° ${index + 1} : ${taches[index].titre}`,
    "",
    "Termine ta réponse par exactement deux lignes, et rien après :",
    "OBJECTIF: atteint   — ou —   OBJECTIF: non atteint",
    `RENVOI: <numéro de la tâche à reprendre, de 1 à ${areprendre}>   — ou —   RENVOI: aucun`,
  ].join("\n");
}

/**
 * Ce qu'on demande au modèle quand l'objectif est abandonné.
 *
 * Sans ce tour, l'abandon laissait la personne devant une carte « échec » et
 * des dizaines de tours dont elle ne pouvait rien tirer : impossible de savoir
 * si un connecteur MCP manquait, si un droit avait été refusé, ou si le modèle
 * avait simplement mal compris. Le plan et ses échecs lui sont donnés tels
 * qu'ils sont, pour que le compte rendu porte sur des faits.
 */
function consigneBilan(
  goal: string,
  taches: Tache[],
  raison: string,
  echecs: EchecOutil[],
  appels: number,
): string {
  // Trois cas très différents, qu'un même silence confondait : le modèle n'a
  // jamais rien appelé (il n'a fait qu'écrire), ses outils ont tous répondu, ou
  // certains ont échoué.
  const liste = echecs.length
    ? echecs
        .slice(-8)
        .map((e) => `- ${e.tool} : ${premiereLigne(e.output) || "(sans message)"}`)
        .join("\n")
    : appels === 0
      ? "(aucun outil n'a été appelé de tout l'objectif : le modèle n'a fait qu'écrire du texte)"
      : "(les outils appelés ont répondu sans erreur)";
  return [
    `[Objectif] Arrêt de l'objectif « ${goal} » : il n'est pas atteint (${raison}).`,
    "Plan et état réel des tâches :",
    lirePlan(taches),
    "",
    `Appels d'outils pendant l'objectif : ${appels}`,
    "Appels d'outils qui ont échoué :",
    liste,
    "",
    "Fais maintenant le compte rendu pour la personne qui attendait, en quelques lignes :",
    "1. ce qui est fait et ce qui ne l'est pas, tâche par tâche ;",
    "2. ce qui t'a bloqué, nommément — un outil ou un connecteur MCP qui n'a pas répondu, un droit refusé, un fichier ou une information qui manque, une commande qui échoue ;",
    "3. ce qu'il faut pour repartir, ou la décision que tu attends d'elle.",
    "",
    "Ne relance pas de plan et ne promets rien : c'est un compte rendu, pas une nouvelle tentative.",
  ].join("\n");
}

/** Le renvoi demandé à la vérification : le rang de la tâche à reprendre. */
function lireRenvoi(texte: string, taches: number): number | null {
  const m = texte.match(/^\s*RENVOI\s*:\s*(aucun|\d+)/im);
  if (!m) return null;
  if (m[1].toLowerCase() === "aucun") return null;
  const n = Number(m[1]);
  // La dernière tâche est la vérification elle-même : elle ne se renvoie pas.
  return n >= 1 && n <= taches - 1 ? n - 1 : null;
}

/** Le mot de la vérification : comment finir un plan qui doit être testé. */
function estVerification(titre: string): boolean {
  return /v[ée]rifi|\btest|contr[ôo]l|valide|recette|assure[- ]toi|s'assurer/i.test(titre);
}

/** Ce que le planificateur a rendu, réduit à ce dont la boucle a besoin. */
async function decouper(texte: string): Promise<string[]> {
  try {
    const plan = await core.planTask(texte, true);
    return plan.steps.map((s) => s.trim()).filter((s) => s !== "");
  } catch {
    // Un planificateur muet ne condamne pas l'objectif : il fera une tâche.
    return [];
  }
}

/**
 * L'objectif découpé en tâches.
 *
 * Deux découpages, parce qu'ils ne répondent pas à la même question : le
 * premier donne les **parties** (le cadrage, l'histoire, la carte…), le second
 * les **tâches** de chaque partie (« écrire le script du saut »). Sans le
 * second, « construire la carte » reste une tâche qu'un seul tour ne peut pas
 * finir.
 */
async function etablirPlan(
  goal: string,
): Promise<{ parties: number; taches: Tache[]; planifie: boolean }> {
  let parties = await decouper(goal);
  const planifie = parties.length > 0;
  if (!planifie) parties = [goal];

  const taches: Tache[] = [];
  for (const partie of parties) {
    let fines = await decouper(partie);
    if (fines.length <= 1) fines = [partie];
    for (const t of fines) taches.push({ titre: t, partie, etat: "a_faire" });
  }

  // Un objectif se termine par un test : c'est lui qui décide de renvoyer à une
  // tâche. Le planificateur le met en dernier quand le résultat doit être
  // vérifié ; sinon on l'ajoute.
  if (!estVerification(taches[taches.length - 1]?.titre ?? "")) {
    taches.push({
      titre:
        "Vérifie l'objectif de bout en bout : lance les tests ou les contrôles possibles, ouvre ce qui a été produit, et dis ce qui ne va pas.",
      partie: "Vérification",
      etat: "a_faire",
    });
  }

  // Un plan qui explose n'est pas un plan : on borne les tâches ordinaires mais
  // on garde la vérification, qui est la dernière.
  if (taches.length > TACHES_MAX) {
    const verif = taches.splice(TACHES_MAX - 1);
    taches.length = TACHES_MAX - 1;
    taches.push(...verif.slice(-1));
  }
  return { parties: parties.length, taches, planifie };
}

/** L'objectif est-il mené à son terme ? `false` quand il a été arrêté. */
export async function runGoal(sessionId: string, goal: string, hooks: GoalHooks): Promise<boolean> {
  const tacheCentre = taskCenter.addWorkflow(
    `Objectif : ${goal.slice(0, 40)}${goal.length > 40 ? "…" : ""}`,
  );

  const { parties, taches, planifie } = await etablirPlan(goal);
  hooks.onPlan(taches, parties);
  taskCenter.setPlan(
    tacheCentre,
    taches.map((t) => t.titre),
  );
  if (!planifie) {
    hooks.onEvent({
      type: "log",
      level: "warn",
      msg: "Le planificateur n'a pas découpé cet objectif : il est mené d'un bloc, puis vérifié.",
      source: TOUR_BILAN,
    });
  }

  /** Les outils appelés pendant l'objectif, et ceux qui ont échoué. */
  const outilsVus = new Map<string, string>();
  const echecs: EchecOutil[] = [];

  /** Un tour de conversation, dont on garde ce qui a été écrit et agi. */
  const envoyer = async (
    message: string,
  ): Promise<{ ecrit: string; coupe: boolean; outils: number }> => {
    let reponse = "";
    let coupe = false;
    const outilsDuTour = new Set<string>();
    await core.sendMessage(
      sessionId,
      message,
      (ev) => {
        if (ev.type === "token") reponse += ev.text;
        if (ev.type === "log" && ev.source === TOUR_INACHEVE) coupe = true;
        if (ev.type === "tool_call") {
          outilsVus.set(ev.call_id, ev.tool);
          outilsDuTour.add(ev.tool);
        }
        if (ev.type === "tool_result" && !ev.ok) {
          echecs.push({ tool: outilsVus.get(ev.call_id) ?? "outil", output: ev.output });
        }
        hooks.onEvent(ev);
      },
      undefined,
      null,
      null,
    );
    return { ecrit: sansReflexion(reponse), coupe, outils: outilsDuTour.size };
  };

  /**
   * Mène une tâche jusqu'à ce qu'elle soit finie.
   *
   * Aucun plafond de tours : la tâche continue tant qu'elle n'est pas finie, et
   * c'est la personne qui arrête si c'est trop long. Une seule coupure
   * automatique — la vraie boucle : quand le modèle redit la même réponse sans
   * appeler le moindre outil (`REDITES_BOUCLE` fois), la tentative est coupée et
   * la tâche relancée avec l'ordre de changer de méthode, jamais abandonnée.
   *
   * `interrompu` n'est ni un succès ni un échec : la personne a arrêté.
   */
  const menerTache = async (index: number): Promise<"fait" | "interrompu"> => {
    if (hooks.arret()) return "interrompu";
    const t = taches[index];
    t.etat = "en_cours";
    hooks.onTache(index);
    const reponsesVues: string[] = [];
    let redites = 0;
    let message = consigneTache(goal, taches, index);
    for (;;) {
      if (hooks.arret()) return "interrompu";
      const rep = await envoyer(message);
      if (hooks.arret()) return "interrompu";
      // Un tour coupé par la limite d'actions : le travail était en cours, pas
      // en boucle — on reprend là où il s'est arrêté.
      if (rep.coupe) {
        message = relanceTache(
          "la tâche a été interrompue en cours de route : reprends là où tu t'es arrêté",
        );
        continue;
      }
      // Le juge décide de la tâche, pas de l'objectif : c'est ce qui permet de
      // reprendre une seule tâche plus tard.
      let reste = "";
      if (rep.ecrit) {
        const verdict = await core.judgeGoal(t.titre, rep.ecrit);
        if (verdict.atteint) {
          t.etat = "fait";
          hooks.onTacheFin(index, "fait");
          return "fait";
        }
        reste = verdict.reste;
      }
      // La boucle au sens strict : le modèle redit la même réponse sans appeler
      // le moindre outil. On coupe la tentative et on ordonne de changer de
      // méthode — insister sur la même approche ne mènerait nulle part.
      const redite =
        rep.outils === 0 &&
        (rep.ecrit === "" || reponsesVues.some((precedente) => memeReponse(precedente, rep.ecrit)));
      if (rep.ecrit) reponsesVues.push(rep.ecrit);
      redites = redite ? redites + 1 : 0;
      if (redites >= REDITES_BOUCLE) {
        redites = 0;
        hooks.onEvent({
          type: "log",
          level: "warn",
          msg: `La tâche ${index + 1} tourne en rond (même réponse sans agir) : elle est coupée et relancée avec l'ordre de changer de méthode.`,
          source: TOUR_BOUCLE,
        });
        message = `${relanceTache("terminer et vérifier cette tâche")}\n\n${CHANGEMENT_METHODE}`;
        continue;
      }
      const manque = rep.ecrit
        ? reste || "terminer et vérifier cette tâche"
        : "tu n'as rien répondu : agis avec tes outils et réponds par ce qui en résulte";
      message = redites === 0 ? relanceTache(manque) : `${relanceTache(manque)}\n\n${INSISTANCE}`;
    }
  };

  /**
   * La vérification finale : atteint, ou la tâche à reprendre.
   *
   * Tant qu'elle dit « non atteint », elle doit désigner la tâche fautive ; si
   * elle ne le fait pas, on la relance au lieu d'arrêter — c'est la seule façon
   * de continuer sans tout recommencer. Comme pour une tâche, aucune limite de
   * reprises : une vraie boucle est coupée et relancée avec l'ordre de changer
   * de méthode.
   */
  const verifier = async (
    index: number,
  ): Promise<{ atteint: boolean; renvoi: number | null; reste: string }> => {
    const t = taches[index];
    t.etat = "en_cours";
    hooks.onTache(index);
    const vues: string[] = [];
    let redites = 0;
    let message = consigneVerification(goal, taches, index);
    for (;;) {
      if (hooks.arret()) return { atteint: false, renvoi: null, reste: "" };
      const rep = await envoyer(message);
      if (hooks.arret()) return { atteint: false, renvoi: null, reste: "" };
      const renvoi = rep.ecrit ? lireRenvoi(rep.ecrit, taches.length) : null;
      const verdict = rep.ecrit
        ? await core.judgeGoal(goal, rep.ecrit)
        : { atteint: false, reste: "" as string };
      if (verdict.atteint) {
        t.etat = "fait";
        hooks.onTacheFin(index, "fait");
        return { atteint: true, renvoi: null, reste: "" };
      }
      if (renvoi !== null) {
        t.etat = "echec";
        hooks.onTacheFin(
          index,
          "echec",
          `à reprendre : tâche ${renvoi + 1} — ${taches[renvoi].titre}`,
        );
        return { atteint: false, renvoi, reste: verdict.reste };
      }
      // « Non atteint » sans tâche désignée : on redemande, on n'arrête pas.
      const redite =
        rep.outils === 0 &&
        (rep.ecrit === "" || vues.some((precedente) => memeReponse(precedente, rep.ecrit)));
      if (rep.ecrit) vues.push(rep.ecrit);
      redites = redite ? redites + 1 : 0;
      if (redites >= REDITES_BOUCLE) {
        redites = 0;
        hooks.onEvent({
          type: "log",
          level: "warn",
          msg: "La vérification tourne en rond : elle est relancée avec l'ordre de changer de méthode.",
          source: TOUR_BOUCLE,
        });
        message = `${relanceVerification("nomme la tâche à reprendre, ou dis l'objectif atteint")}\n\n${CHANGEMENT_METHODE}`;
        continue;
      }
      message = relanceVerification(
        verdict.reste ||
          "tu as conclu que l'objectif n'est pas atteint, mais tu n'as pas dit quelle tâche doit être reprise",
      );
    }
  };

  /** Le compte rendu d'un abandon : un dernier tour, non jugé, où le modèle s'explique. */
  const bilan = async (raison: string) => {
    hooks.onEvent({
      type: "log",
      level: "warn",
      msg: `Objectif non atteint (${raison}) : le modèle fait le compte rendu de ce qui a bloqué.`,
      source: TOUR_BILAN,
    });
    try {
      await core.sendMessage(
        sessionId,
        consigneBilan(goal, taches, raison, echecs, outilsVus.size),
        hooks.onEvent,
        undefined,
        null,
        null,
      );
    } catch (e) {
      // Un compte rendu impossible ne change rien : l'arrêt est déjà annoncé.
      console.warn("compte rendu de l'objectif impossible :", e);
    }
  };

  const dernier = taches.length - 1;
  let renvois = 0;
  let depart = 0;
  try {
    for (;;) {
      // La vérification est toujours la dernière : les tâches ordinaires vont
      // de `depart` à `taches.length - 2` inclus.
      for (let i = depart; i < taches.length - 1; i++) {
        const etat = await menerTache(i);
        if (etat === "interrompu") {
          taskCenter.fail(tacheCentre, "arrêté");
          return false;
        }
      }
      if (hooks.arret()) {
        taskCenter.fail(tacheCentre, "arrêté");
        return false;
      }
      const v = await verifier(dernier);
      if (v.atteint) {
        taskCenter.done(tacheCentre, {
          detail: renvois > 0 ? `atteint après ${renvois} reprise(s)` : "atteint",
        });
        return true;
      }
      if (v.renvoi !== null) {
        renvois += 1;
        depart = v.renvoi;
        // Les tâches reprises repartent de zéro : la vérification a dit qu'elles
        // ne tenaient pas.
        for (let i = depart; i < taches.length; i++) taches[i].etat = "a_faire";
        hooks.onRenvoi?.(v.renvoi, v.reste || "la vérification finale a échoué", renvois);
        if (!hooks.arret()) {
          hooks.onEvent({
            type: "log",
            level: "info",
            msg: `La vérification renvoie à la tâche ${v.renvoi + 1} : elle est reprise, avec les suivantes.`,
            source: TOUR_BILAN,
          });
        }
        continue;
      }
      // La vérification s'est interrompue (la personne a arrêté) : c'est le
      // seul cas où elle ne désigne pas de tâche.
      taskCenter.fail(tacheCentre, "arrêté");
      return false;
    }
  } catch (e) {
    // Juge indisponible, moteur arrêté : on ne relance pas à l'aveugle. Le
    // modèle explique ce qu'il a fait de son côté.
    const raison = String(e);
    await bilan(raison);
    taskCenter.fail(tacheCentre, raison);
    return false;
  }
}
