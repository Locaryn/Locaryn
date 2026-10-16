import { Icon } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import { type AppTask, TASK_META, taskCenter, useTasks } from "../../lib/taskCenter";

/** Depuis combien de temps, en clair : « 42 s », « 3 min ». */
function duree(depuis: number, jusqua: number): string {
  const s = Math.max(0, Math.round((jusqua - depuis) / 1000));
  if (s < 60) return `${s} s`;
  const m = Math.round(s / 60);
  return m < 60 ? `${m} min` : `${Math.floor(m / 60)} h ${m % 60} min`;
}

function Tache({ t, maintenant }: { t: AppTask; maintenant: number }) {
  const [ouvert, setOuvert] = useState(false);
  const meta = TASK_META[t.type];
  const fin = t.status === "running" ? maintenant : t.updatedAt;
  const etapes = t.steps?.length ?? 0;
  const avecFil = !!t.transcript;
  return (
    <li className={`lt-tache is-${t.status}`}>
      <button
        type="button"
        className="lt-tete"
        aria-expanded={avecFil ? ouvert : undefined}
        onClick={() => avecFil && setOuvert((v) => !v)}
      >
        <span className="lt-icone" style={{ color: meta.color }} aria-hidden>
          <Icon name={meta.icon} size={16} />
        </span>
        <span className="lt-corps">
          <span className="lt-libelle">{t.label}</span>
          <span className="lt-detail">
            {meta.label}
            {t.model ? ` · ${t.model}` : t.detail ? ` · ${t.detail}` : ""}
            {etapes > 0 &&
              ` · étape ${Math.min((t.stepIndex ?? 0) + (t.status === "running" ? 1 : 0), etapes)}/${etapes}`}
            {t.tools && t.tools.length > 0 && ` · ${t.tools.length} outil(s)`}
          </span>
        </span>
        <span className="lt-etat">
          {t.status === "running" ? (
            <span className="lt-pouls" aria-label="En cours" />
          ) : t.status === "done" ? (
            <Icon name="check" size={14} title="Terminé" />
          ) : (
            <Icon name="warning" size={14} title="Échec" />
          )}
          <span className="lt-duree">{duree(t.createdAt, fin)}</span>
        </span>
      </button>
      {typeof t.progress === "number" && t.status === "running" && (
        <div className="lt-barre" aria-hidden>
          <span style={{ width: `${t.progress}%` }} />
        </div>
      )}
      {t.error && <p className="lt-erreur">{t.error}</p>}
      {ouvert && avecFil && <pre className="lt-fil">{t.transcript}</pre>}
    </li>
  );
}

/**
 * L'onglet « Tâches » : tout ce qui tourne en arrière-plan pendant que la
 * conversation continue — sous-agents (avec leur fil), plans, téléchargements,
 * générations. Ce qui tourne d'abord, le reste ensuite.
 */
export function TasksTab() {
  const taches = useTasks();
  const [maintenant, setMaintenant] = useState(Date.now());
  const enCours = taches.filter((t) => t.status === "running");
  const finies = taches.filter((t) => t.status !== "running");

  // Les durées avancent tant que quelque chose tourne.
  useEffect(() => {
    if (enCours.length === 0) return;
    const minuterie = window.setInterval(() => setMaintenant(Date.now()), 1000);
    return () => window.clearInterval(minuterie);
  }, [enCours.length]);

  if (taches.length === 0) {
    return (
      <div className="lt-vide">
        <Icon name="list-bullets" size={24} />
        <p>Aucune tâche pour l'instant.</p>
        <p className="lt-vide-aide">
          Les sous-agents que le modèle lance, les plans et les téléchargements apparaissent ici
          pendant qu'ils tournent.
        </p>
      </div>
    );
  }

  return (
    <div className="lt-panneau">
      {enCours.length > 0 && (
        <section>
          <h3 className="lt-titre">En cours · {enCours.length}</h3>
          <ul className="lt-liste">
            {enCours.map((t) => (
              <Tache key={t.id} t={t} maintenant={maintenant} />
            ))}
          </ul>
        </section>
      )}
      {finies.length > 0 && (
        <section>
          <h3 className="lt-titre">
            Terminées · {finies.length}
            <button
              type="button"
              className="lt-vider"
              onClick={() => {
                for (const t of finies) taskCenter.remove(t.id);
              }}
            >
              Vider
            </button>
          </h3>
          <ul className="lt-liste">
            {finies.map((t) => (
              <Tache key={t.id} t={t} maintenant={maintenant} />
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
