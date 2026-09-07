import { Icon, type IconName } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type NotificationGroup, type NotificationPrefs, core } from "../lib/core";
import { oublierPreferences } from "../lib/osNotify";

/**
 * Ce pour quoi l'application accepte de déranger.
 *
 * Les réglages vont par **groupe**, pas par événement. Une case par
 * notification aurait donné une page de trente interrupteurs que personne ne
 * lit ; six groupes se parcourent d'un coup d'œil, et chacun répond à la seule
 * question qu'on se pose : « est-ce que je veux être dérangé pour ça ? »
 *
 * Deux réglages traversent tous les groupes — ne prévenir que si la fenêtre
 * n'est pas au premier plan, et un seuil de durée en dessous duquel une tâche
 * se termine sans bruit. Ils sont rangés à part, parce qu'ils ne parlent pas de
 * *quoi* mais de *quand*.
 */

const GROUPES: {
  id: NotificationGroup;
  label: string;
  desc: string;
  icon: IconName;
  /** Ce groupe mérite un avertissement quand on l'éteint. */
  critique?: boolean;
}[] = [
  {
    id: "approvals",
    label: "Approbations d'outil",
    desc: "Un outil attend votre accord. C'est le seul cas où ne pas être prévenu arrête le travail : l'agent reste en attente tant que personne ne répond.",
    icon: "shield",
    critique: true,
  },
  {
    id: "downloads",
    label: "Téléchargements",
    desc: "Fin ou échec du téléchargement d'un modèle.",
    icon: "download",
  },
  {
    id: "long_tasks",
    label: "Tâches longues",
    desc: "Une réponse, un workflow ou une génération s'achève alors que vous êtes ailleurs.",
    icon: "hourglass",
  },
  {
    id: "model_lifecycle",
    label: "Modèles en mémoire",
    desc: "Le modèle a été déchargé de la mémoire après une longue inactivité.",
    icon: "cpu",
  },
  {
    id: "security",
    label: "Sécurité",
    desc: "Un appareil tente de s'appairer, ou vient de se connecter au mode serveur.",
    icon: "lock",
  },
  {
    id: "maintenance",
    label: "Entretien",
    desc: "Mise à jour disponible, espace disque bas dans le dossier des modèles.",
    icon: "gear",
  },
];

/** Les seuils proposés, en secondes. Au-delà d'une minute, on parle en minutes. */
const SEUILS = [0, 5, 20, 60, 300];

function libelleSeuil(s: number): string {
  if (s === 0) return "toujours prévenir";
  if (s < 60) return `au-delà de ${s} s`;
  return `au-delà de ${Math.round(s / 60)} min`;
}

export function NotificationSettings() {
  const [prefs, setPrefs] = useState<NotificationPrefs | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [enregistre, setEnregistre] = useState(false);

  useEffect(() => {
    let vivant = true;
    core
      .notificationPrefs()
      .then((p) => {
        if (vivant) setPrefs(p);
      })
      .catch((e) => {
        if (vivant) setErreur(String(e).replace(/^Error:\s*/, ""));
      });
    return () => {
      vivant = false;
    };
  }, []);

  const appliquer = useCallback(async (suivant: NotificationPrefs) => {
    // L'écran suit tout de suite : attendre l'écriture disque pour bouger une
    // case donne l'impression que le clic n'a pas porté.
    setPrefs(suivant);
    setErreur(null);
    try {
      await core.setNotificationPrefs(suivant);
      // Le cache de session doit lâcher, sinon le changement ne s'appliquerait
      // qu'au prochain lancement.
      oublierPreferences();
      setEnregistre(true);
      setTimeout(() => setEnregistre(false), 1600);
    } catch (e) {
      setErreur(String(e).replace(/^Error:\s*/, ""));
    }
  }, []);

  if (erreur && !prefs) {
    return <div className="locaryn-vp-error">{erreur}</div>;
  }
  if (!prefs) {
    return <p className="locaryn-field-hint">Chargement des préférences…</p>;
  }

  const tout = prefs.enabled;

  return (
    <div className="locaryn-notif-settings">
      <div className="locaryn-memory-intro">
        <div>
          <span className="locaryn-account-eyebrow">NOTIFICATIONS</span>
          <h3>Quand Locaryn vous prévient</h3>
          <p>
            Une bannière du système arrive par-dessus ce que vous faites. Elle est utile pour ce qui
            se termine hors de l'écran, et pesante pour le reste — d'où ces réglages.
          </p>
        </div>
        {enregistre && <span className="locaryn-memory-count">Enregistré</span>}
      </div>

      {erreur && <div className="locaryn-vp-error">{erreur}</div>}

      <label className="locaryn-notif-master">
        <input
          type="checkbox"
          checked={tout}
          onChange={(e) => void appliquer({ ...prefs, enabled: e.target.checked })}
        />
        <span>
          <strong>Autoriser les notifications du système</strong>
          <span className="locaryn-field-hint">
            Décoché, aucune bannière ne part. Le centre de notifications de l'application continue
            de tout enregistrer.
          </span>
        </span>
      </label>

      <div className="locaryn-notif-groupes" aria-disabled={!tout}>
        {GROUPES.map((g) => {
          const actif = prefs[g.id];
          return (
            <label
              key={g.id}
              className={`locaryn-notif-groupe${tout ? "" : " locaryn-notif-eteint"}`}
            >
              <input
                type="checkbox"
                checked={actif}
                disabled={!tout}
                onChange={(e) => void appliquer({ ...prefs, [g.id]: e.target.checked })}
              />
              <span className="locaryn-notif-groupe-icone" aria-hidden="true">
                <Icon name={g.icon} size={15} />
              </span>
              <span className="locaryn-notif-groupe-texte">
                <strong>{g.label}</strong>
                <span className="locaryn-field-hint">{g.desc}</span>
                {g.critique && tout && !actif && (
                  <span className="locaryn-notif-avertit">
                    <Icon name="warning" size={13} /> Sans cette bannière, un outil peut attendre
                    votre accord sans que rien ne le signale.
                  </span>
                )}
              </span>
            </label>
          );
        })}
      </div>

      <div className="locaryn-notif-quand">
        <div className="locaryn-field-label">Quand prévenir</div>

        <label className="locaryn-notif-groupe">
          <input
            type="checkbox"
            checked={prefs.only_when_hidden}
            disabled={!tout}
            onChange={(e) => void appliquer({ ...prefs, only_when_hidden: e.target.checked })}
          />
          <span className="locaryn-notif-groupe-texte">
            <strong>Seulement quand la fenêtre n'est pas au premier plan</strong>
            <span className="locaryn-field-hint">
              Vous regardez déjà : la fenêtre montre la même chose. Décochez si vous travaillez sur
              plusieurs écrans.
            </span>
          </span>
        </label>

        <div className="locaryn-field">
          <label htmlFor="notif-seuil" className="locaryn-field-label">
            Durée minimale d'une tâche
          </label>
          <select
            id="notif-seuil"
            className="locaryn-select"
            value={String(prefs.min_duration_seconds)}
            disabled={!tout}
            onChange={(e) =>
              void appliquer({ ...prefs, min_duration_seconds: Number(e.target.value) })
            }
          >
            {SEUILS.map((s) => (
              <option key={s} value={String(s)}>
                {libelleSeuil(s)}
              </option>
            ))}
          </select>
          <p className="locaryn-field-hint">
            Une tâche finie en trois secondes ne mérite pas de bannière : seule celle qu'on a eu le
            temps d'oublier la mérite. Un échec passe outre ce seuil — c'est ce qu'on veut savoir
            tout de suite.
          </p>
        </div>

        <label className="locaryn-notif-groupe">
          <input
            type="checkbox"
            checked={prefs.taskbar_progress}
            onChange={(e) => void appliquer({ ...prefs, taskbar_progress: e.target.checked })}
          />
          <span className="locaryn-notif-groupe-texte">
            <strong>Avancement dans la barre des tâches</strong>
            <span className="locaryn-field-hint">
              L'icône de l'application porte une barre de progression : c'est le seul moyen de
              suivre un téléchargement sans rouvrir la fenêtre. Indépendant des bannières.
            </span>
          </span>
        </label>
      </div>
    </div>
  );
}
