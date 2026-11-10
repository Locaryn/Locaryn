// Les réglages de chargement du modèle, dans la barre latérale droite.
//
// Ce que le moteur ne peut changer qu'en rechargeant le modèle : la part
// posée sur la carte graphique, les lots, le cache KV, le décodage
// spéculatif. Ils vivaient dans un écran de réglages à part ; ils sont ici, à
// côté des réglages de génération, avec un seul bouton pour les appliquer.

import { useEffect, useState } from "react";
import { type InferenceConfig, type KvCacheType, core } from "../lib/core";

/** Les modes de décodage spéculatif proposés, tels que le moteur les nomme. */
const SPECULATIFS: { id: string; libelle: string }[] = [
  { id: "none", libelle: "Aucun" },
  { id: "draft-mtp", libelle: "MTP (tête intégrée au modèle)" },
  { id: "draft-simple", libelle: "Modèle brouillon" },
  { id: "ngram-simple", libelle: "N-grammes (sans second modèle)" },
];

const CACHES: { id: KvCacheType; libelle: string }[] = [
  { id: "f16", libelle: "F16 — précision complète" },
  { id: "q8_0", libelle: "Q8 — moitié moins de mémoire" },
  { id: "q4_0", libelle: "Q4 — le plus léger" },
];

type Props = {
  /** Recharger le modèle chargé : le moteur relit la configuration. */
  onReload: () => Promise<void>;
  /** Un modèle est en mémoire : le bouton recharge, sinon il enregistre. */
  modelLoaded: boolean;
};

function Nombre({
  id,
  libelle,
  aide,
  valeur,
  min,
  max,
  pas = 1,
  onChange,
}: {
  id: string;
  libelle: string;
  aide?: string;
  valeur: number;
  min: number;
  max: number;
  pas?: number;
  onChange: (v: number) => void;
}) {
  return (
    <div className="lmc-ligne" title={aide}>
      <label htmlFor={id} className="lmc-label">
        {libelle}
      </label>
      <input
        id={id}
        type="number"
        className="lmc-nombre"
        value={Number.isFinite(valeur) ? valeur : 0}
        min={min}
        max={max}
        step={pas}
        onChange={(e) => {
          const v = Number(e.target.value);
          if (Number.isFinite(v)) onChange(Math.min(max, Math.max(min, v)));
        }}
      />
    </div>
  );
}

function Bascule({
  id,
  libelle,
  aide,
  actif,
  onChange,
}: {
  id: string;
  libelle: string;
  aide?: string;
  actif: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label htmlFor={id} className="lmc-ligne lmc-ligne-bascule" title={aide}>
      <span className="lmc-label">{libelle}</span>
      <input
        id={id}
        type="checkbox"
        className="lmc-bascule"
        checked={actif}
        onChange={(e) => onChange(e.target.checked)}
      />
    </label>
  );
}

export function LoadSettings({ onReload, modelLoaded }: Props) {
  const [cfg, setCfg] = useState<InferenceConfig | null>(null);
  const [modifie, setModifie] = useState(false);
  const [occupe, setOccupe] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);

  useEffect(() => {
    core
      .getInferenceConfig()
      .then(setCfg)
      .catch((e) => setErreur(String(e).replace(/^Error:\s*/, "")));
  }, []);

  if (!cfg) {
    return erreur ? <div className="lmc-error">{erreur}</div> : null;
  }

  const poser = (morceau: Partial<InferenceConfig>) => {
    setCfg({ ...cfg, ...morceau });
    setModifie(true);
  };

  async function appliquer() {
    if (!cfg) return;
    setOccupe(true);
    setErreur(null);
    try {
      await core.setInferenceConfig(cfg);
      if (modelLoaded) await onReload();
      setModifie(false);
    } catch (e) {
      setErreur(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setOccupe(false);
    }
  }

  const speculatif = cfg.spec_type ?? "none";

  return (
    <div className="lmc-chargement">
      <div className="lmc-section-titre">Chargement du modèle</div>

      <Nombre
        id="lmc-gpu"
        libelle="Couches sur la carte graphique"
        aide="-1 : autant que la mémoire vidéo le permet. 0 : processeur seul."
        valeur={cfg.gpu_layers}
        min={-1}
        max={999}
        onChange={(v) => poser({ gpu_layers: v })}
      />
      <Nombre
        id="lmc-threads"
        libelle="Fils du processeur"
        aide="0 : choisi automatiquement."
        valeur={cfg.cpu_threads}
        min={0}
        max={256}
        onChange={(v) => poser({ cpu_threads: v })}
      />
      <Nombre
        id="lmc-batch"
        libelle="Lot d'évaluation"
        aide="Jetons lus d'un coup. Plus grand : lecture plus rapide, plus de mémoire."
        valeur={cfg.batch_size}
        min={32}
        max={8192}
        pas={32}
        onChange={(v) => poser({ batch_size: v })}
      />
      <Nombre
        id="lmc-ubatch"
        libelle="Lot physique"
        aide="0 : choisi pour votre carte."
        valeur={cfg.ubatch_size ?? 0}
        min={0}
        max={8192}
        pas={32}
        onChange={(v) => poser({ ubatch_size: v })}
      />
      <Nombre
        id="lmc-slots"
        libelle="Réponses simultanées"
        aide="Chaque emplacement se partage la fenêtre de contexte."
        valeur={cfg.parallel_slots}
        min={1}
        max={16}
        onChange={(v) => poser({ parallel_slots: v })}
      />
      <Nombre
        id="lmc-checkpoints"
        libelle="Points de reprise du contexte"
        aide="0 : valeur du moteur."
        valeur={cfg.ctx_checkpoints ?? 0}
        min={0}
        max={256}
        onChange={(v) => poser({ ctx_checkpoints: v })}
      />

      <div className="lmc-ligne">
        <label htmlFor="lmc-kv" className="lmc-label">
          Cache KV
        </label>
        <select
          id="lmc-kv"
          className="lmc-choix"
          value={cfg.kv_cache_type}
          onChange={(e) => poser({ kv_cache_type: e.target.value as KvCacheType })}
        >
          {CACHES.map((c) => (
            <option key={c.id} value={c.id}>
              {c.libelle}
            </option>
          ))}
        </select>
      </div>

      <Bascule
        id="lmc-fa"
        libelle="Flash attention"
        aide="Moins de mémoire vidéo ; nécessaire pour un cache KV compressé."
        actif={cfg.flash_attention}
        onChange={(v) => poser({ flash_attention: v })}
      />
      <Bascule
        id="lmc-kvu"
        libelle="Cache KV unifié"
        aide="Un seul cache partagé entre les réponses simultanées."
        actif={cfg.kv_unified ?? false}
        onChange={(v) => poser({ kv_unified: v })}
      />
      <Bascule
        id="lmc-kvo"
        libelle="Cache KV sur la carte graphique"
        actif={cfg.kv_offload ?? true}
        onChange={(v) => poser({ kv_offload: v })}
      />
      <Bascule
        id="lmc-mlock"
        libelle="Garder le modèle en mémoire"
        aide="Le système ne le déplace pas vers le disque."
        actif={cfg.keep_in_memory ?? false}
        onChange={(v) => poser({ keep_in_memory: v })}
      />
      <Bascule
        id="lmc-mmap"
        libelle="Chargement par mmap"
        actif={cfg.use_mmap}
        onChange={(v) => poser({ use_mmap: v })}
      />

      <div className="lmc-ligne">
        <label htmlFor="lmc-spec" className="lmc-label">
          Décodage spéculatif
        </label>
        <select
          id="lmc-spec"
          className="lmc-choix"
          value={speculatif}
          onChange={(e) => poser({ spec_type: e.target.value })}
        >
          {SPECULATIFS.map((m) => (
            <option key={m.id} value={m.id}>
              {m.libelle}
            </option>
          ))}
        </select>
      </div>
      {speculatif !== "none" && (
        <div className="lmc-sous-bloc">
          <Nombre
            id="lmc-draft-max"
            libelle="Jetons proposés, au plus"
            aide="0 : valeur du moteur."
            valeur={cfg.spec_draft_max ?? 0}
            min={0}
            max={64}
            onChange={(v) => poser({ spec_draft_max: v })}
          />
          <Nombre
            id="lmc-draft-min"
            libelle="Jetons proposés, au moins"
            valeur={cfg.spec_draft_min ?? 0}
            min={0}
            max={64}
            onChange={(v) => poser({ spec_draft_min: v })}
          />
          <Nombre
            id="lmc-draft-p"
            libelle="Probabilité minimale"
            aide="0 : valeur du moteur."
            valeur={cfg.spec_draft_p_min ?? 0}
            min={0}
            max={1}
            pas={0.05}
            onChange={(v) => poser({ spec_draft_p_min: v })}
          />
        </div>
      )}

      {erreur && <div className="lmc-error">{erreur}</div>}
      <button
        type="button"
        className="lmc-save-btn"
        disabled={occupe || !modifie}
        onClick={() => void appliquer()}
      >
        {occupe
          ? "Rechargement…"
          : modelLoaded
            ? "Appliquer et recharger le modèle"
            : "Enregistrer pour le prochain chargement"}
      </button>
    </div>
  );
}
