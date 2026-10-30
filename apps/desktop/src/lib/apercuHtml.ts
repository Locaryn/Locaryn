/**
 * Préparer une page écrite par le modèle pour un cadre isolé.
 *
 * Le cadre n'a pas `allow-same-origin` : la page ne doit jamais toucher
 * l'origine de l'application. Mais une origine opaque fait lever
 * `localStorage` à la première lecture, et la plupart des petites applications
 * web générées (listes, jeux, suivis) s'arrêtent là, page blanche. Une mémoire
 * de remplacement, vidée à chaque rechargement, les laisse tourner.
 */

const MEMOIRE_DE_REMPLACEMENT = `<script>(function(){function memoire(){var d={};return{getItem:function(k){return Object.prototype.hasOwnProperty.call(d,k)?d[k]:null},setItem:function(k,v){d[k]=String(v)},removeItem:function(k){delete d[k]},clear:function(){d={}},key:function(i){return Object.keys(d)[i]||null},get length(){return Object.keys(d).length}}}["localStorage","sessionStorage"].forEach(function(n){try{window[n].getItem("")}catch(e){Object.defineProperty(window,n,{value:memoire(),configurable:true})}})})();</script>`;

/** Une page qui charge des modules : ils exigent CORS, que le protocole des fichiers locaux réserve à l'application. */
export function chargeDesModules(html: string): boolean {
  return /<script[^>]*type\s*=\s*["']?module/i.test(html);
}

/**
 * Le document à donner à `srcDoc`. `base`, l'adresse du dossier de la page,
 * permet de trouver ses feuilles de style, scripts et images relatifs.
 */
export function documentApercu(html: string, base?: string): string {
  const baseTag = base ? `<base href="${base.replace(/"/g, "%22")}">` : "";
  const entete = baseTag + MEMOIRE_DE_REMPLACEMENT;
  const head = /<head[^>]*>/i.exec(html);
  if (head) {
    const fin = head.index + head[0].length;
    return html.slice(0, fin) + entete + html.slice(fin);
  }
  return entete + html;
}

/** L'adresse du dossier d'un fichier servi par `toFrameUrl`, barre finale comprise. */
export function dossierDe(url: string): string {
  const sansRequete = url.split("?")[0] ?? url;
  return sansRequete.slice(0, sansRequete.lastIndexOf("/") + 1);
}
