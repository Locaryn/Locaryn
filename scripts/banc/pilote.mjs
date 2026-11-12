// Pilote l'application lancée par `lancer.sh`, par le débogage à distance de
// WebView2 : la vraie application, son vrai service, pas la démo du navigateur.
//
//   node scripts/banc/pilote.mjs texte                 texte visible de l'écran
//   node scripts/banc/pilote.mjs capture sortie.png    capture de la fenêtre
//   node scripts/banc/pilote.mjs clic "Réglages"       clic sur un bouton par son texte
//   node scripts/banc/pilote.mjs saisir "bonjour"      taper dans le champ actif
//   node scripts/banc/pilote.mjs touche Enter          une touche
//   node scripts/banc/pilote.mjs eval "document.title" évaluer une expression
//   node scripts/banc/pilote.mjs attendre "texte" 30   attendre qu'un texte paraisse (secondes)
//   node scripts/banc/pilote.mjs erreurs               erreurs de console depuis 2 s
//
// `puppeteer-core` n'est pas une dépendance du dépôt : `NODE_PATH` désigne un
// dossier `node_modules` qui le contient.

import { writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";

const PORT = process.env.PORT_BANC ?? "9333";
const require = createRequire(import.meta.url);

function chargerPuppeteer() {
  const dossiers = (process.env.NODE_PATH ?? "").split(path.delimiter).filter(Boolean);
  for (const d of dossiers) {
    try {
      return require(path.join(d, "puppeteer-core"));
    } catch {
      // Dossier suivant : celui-ci ne le contient pas.
    }
  }
  throw new Error(
    "puppeteer-core introuvable : posez NODE_PATH sur un node_modules qui le contient.",
  );
}

/** La page de l'application : pas l'écran de lancement, pas un outil de dev. */
async function pageDeLApplication(navigateur) {
  const pages = await navigateur.pages();
  const candidates = [];
  for (const p of pages) {
    const url = p.url();
    if (url.startsWith("devtools://")) continue;
    const taille = await p.evaluate(() => document.body?.innerText.length ?? 0).catch(() => 0);
    candidates.push({ p, url, taille });
  }
  candidates.sort((a, b) => b.taille - a.taille);
  if (candidates.length === 0) throw new Error("aucune page ouverte dans l'application");
  return candidates[0].p;
}

async function cliquerParTexte(page, texte) {
  const trouve = await page.evaluate((cherche) => {
    const visibles = [
      ...document.querySelectorAll("button, a, [role=button], [role=tab], label"),
    ].filter((e) => e.offsetParent !== null);
    const libelle = (e) =>
      `${e.textContent ?? ""} ${e.getAttribute("aria-label") ?? ""} ${e.getAttribute("title") ?? ""}`.trim();
    const exact = visibles.find((e) => (e.textContent ?? "").trim() === cherche);
    const cible = exact ?? visibles.find((e) => libelle(e).includes(cherche));
    if (!cible) return null;
    cible.scrollIntoView({ block: "center" });
    cible.click();
    return libelle(cible).slice(0, 80);
  }, texte);
  if (!trouve) throw new Error(`aucun élément cliquable ne porte « ${texte} »`);
  return trouve;
}

async function principal() {
  const [commande, ...args] = process.argv.slice(2);
  const puppeteer = chargerPuppeteer();
  const navigateur = await puppeteer.connect({
    browserURL: `http://127.0.0.1:${PORT}`,
    defaultViewport: null,
  });
  try {
    const page = await pageDeLApplication(navigateur);
    switch (commande) {
      case "texte":
        console.log(await page.evaluate(() => document.body.innerText));
        break;
      case "capture": {
        const fichier = args[0] ?? "capture.png";
        // Par le protocole, sans passer par la surface de la fenêtre : posée
        // hors de l'écran, celle-ci n'est pas toujours rafraîchie par Windows,
        // et la capture ordinaire attendait alors une image qui ne venait pas.
        const session = await page.createCDPSession();
        // Deux façons de prendre l'image ; selon l'écran, l'une des deux échoue.
        const prendre = (fromSurface) =>
          Promise.race([
            session.send("Page.captureScreenshot", { format: "png", fromSurface }),
            new Promise((_, refus) =>
              setTimeout(() => refus(new Error("capture sans réponse après 15 s")), 15000),
            ),
          ]);
        const { data } = await prendre(true).catch(() => prendre(false));
        await writeFile(fichier, Buffer.from(data, "base64"));
        await session.detach();
        console.log(fichier);
        break;
      }
      case "clic":
        console.log(`cliqué : ${await cliquerParTexte(page, args[0])}`);
        break;
      case "saisir":
        await page.keyboard.type(args[0] ?? "", { delay: 5 });
        break;
      case "touche":
        await page.keyboard.press(args[0]);
        break;
      case "eval":
        console.log(JSON.stringify(await page.evaluate(`(async () => (${args[0]}))()`), null, 1));
        break;
      case "attendre": {
        const limite = Date.now() + Number(args[1] ?? 30) * 1000;
        while (Date.now() < limite) {
          const present = await page.evaluate((t) => document.body.innerText.includes(t), args[0]);
          if (present) {
            console.log("présent");
            return;
          }
          await new Promise((r) => setTimeout(r, 500));
        }
        throw new Error(`« ${args[0]} » n'est pas apparu`);
      }
      case "erreurs": {
        const vues = [];
        page.on("console", (m) => {
          if (m.type() === "error") vues.push(m.text());
        });
        page.on("pageerror", (e) => vues.push(String(e)));
        await new Promise((r) => setTimeout(r, 2000));
        console.log(vues.length ? vues.join("\n") : "aucune erreur");
        break;
      }
      default:
        throw new Error(`commande inconnue : ${commande ?? "(aucune)"}`);
    }
  } finally {
    // Se détacher sans fermer l'application.
    navigateur.disconnect();
  }
}

principal().catch((e) => {
  console.error(String(e?.message ?? e));
  process.exit(1);
});
