import { Icon } from "@locaryn/ui-core";
import { INSTALL_LABEL_PREFIX } from "../lib/extensionInstallProgress";
import { useI18n } from "../lib/i18n";
import { type AppTask, useTasks } from "../lib/taskCenter";
import "../styles/onboarding.css";

const isInstall = (t: AppTask) => t.label.startsWith(INSTALL_LABEL_PREFIX);

function titleFor(install: boolean, done: boolean, fr: boolean): string {
  if (install) {
    if (done) return fr ? "Extension installée" : "Extension installed";
    return fr ? "Installation de l'extension" : "Installing the extension";
  }
  if (done) return fr ? "Modèle prêt" : "Model ready";
  return fr ? "Téléchargement du modèle" : "Downloading the model";
}

/**
 * La bulle de téléchargement, en bas à droite.
 *
 * Pendant l'accueil et en interface simplifiée, personne ne regarde la barre
 * d'état : le téléchargement du modèle doit se voir sans la chercher. Une
 * installation d'extension s'y montre dans toutes les interfaces : elle part
 * souvent d'une conversation, loin des réglages. La bulle n'existe que le
 * temps du travail, puis annonce sa fin.
 */
export function DownloadBubble({ visible }: { visible: boolean }) {
  const { lang } = useI18n();
  const tasks = useTasks().filter((t) => t.type === "download" && (visible || isInstall(t)));
  const running = tasks.find((t) => t.status === "running");
  const lastDone = tasks.find((t) => t.status === "done" && Date.now() - t.updatedAt < 6000);
  const shown = running ?? lastDone;
  if (!shown) return null;

  const pct = Math.max(0, Math.min(100, Math.round(shown.progress ?? 0)));
  const done = shown.status === "done";
  const install = isInstall(shown);
  const name = install
    ? shown.label.slice(INSTALL_LABEL_PREFIX.length)
    : shown.label.replace(/^Téléchargement\s*:\s*/, "");
  return (
    <output className="lo-bubble" aria-live="polite" key={`${shown.id}-${done ? "done" : "run"}`}>
      <span className="lo-bubble-icon" data-done={done}>
        <Icon name={done ? "check-circle" : install ? "extensions" : "download-simple"} size={18} />
      </span>
      <span className="lo-bubble-body">
        <span className="lo-bubble-title">{titleFor(install, done, lang === "fr")}</span>
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
