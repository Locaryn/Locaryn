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

/** Le document à donner à `srcDoc`. */
export function documentApercu(html: string): string {
  const entete = MEMOIRE_DE_REMPLACEMENT;
  const head = /<head[^>]*>/i.exec(html);
  if (head) {
    const fin = head.index + head[0].length;
    return html.slice(0, fin) + entete + html.slice(fin);
  }
  return entete + html;
}
