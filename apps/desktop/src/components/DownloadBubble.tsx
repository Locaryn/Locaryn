import { Icon } from "@locaryn/ui-core";
import { useI18n } from "../lib/i18n";
import { useTasks } from "../lib/taskCenter";
import "../styles/onboarding.css";

/**
 * La bulle de téléchargement, en bas à droite.
 *
 * Pendant l'accueil et en interface simplifiée, personne ne regarde la barre
 * d'état : le téléchargement du modèle doit se voir sans la chercher. Elle
 * n'existe que le temps qu'un téléchargement tourne, puis annonce sa fin.
 */
export function DownloadBubble({ visible }: { visible: boolean }) {
  const { lang } = useI18n();
  const tasks = useTasks().filter((t) => t.type === "download");
  const running = tasks.find((t) => t.status === "running");
  const lastDone = tasks.find((t) => t.status === "done" && Date.now() - t.updatedAt < 6000);
  const shown = running ?? lastDone;
  if (!visible || !shown) return null;

  const pct = Math.max(0, Math.min(100, Math.round(shown.progress ?? 0)));
  const done = shown.status === "done";
  const name = shown.label.replace(/^Téléchargement\s*:\s*/, "");
  return (
    <output className="lo-bubble" aria-live="polite" key={done ? "done" : "run"}>
      <span className="lo-bubble-icon" data-done={done}>
        <Icon name={done ? "check-circle" : "download-simple"} size={18} />
      </span>
      <span className="lo-bubble-body">
        <span className="lo-bubble-title">
          {done
            ? lang === "fr"
              ? "Modèle prêt"
              : "Model ready"
            : lang === "fr"
              ? "Téléchargement du modèle"
              : "Downloading the model"}
        </span>
        <span className="lo-bubble-sub" data-no-translate>
          {name}
        </span>
        <span className="lo-bubble-track" aria-hidden="true">
          <span className="lo-bubble-fill" style={{ width: `${done ? 100 : pct}%` }} />
        </span>
      </span>
      <span className="lo-bubble-pct" data-no-translate>
        {done ? "100%" : `${pct}%`}
      </span>
    </output>
  );
}
