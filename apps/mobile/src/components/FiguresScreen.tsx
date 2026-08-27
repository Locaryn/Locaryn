import { LoSwitch } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type FigureDraft, type PhoneFigure, type PhoneFigureSession, api } from "../lib/core";
import { Screen } from "./Screen";

type Props = {
  onBack: () => void;
  /** Une conversation vient d'être ouverte, tenue par la figure. */
  onOpenChat: (sessionId: string) => void;
};

/** Le formulaire vierge, pour une figure neuve. */
const vierge: FigureDraft = {
  name: "",
  description: "",
  instructions: "",
  model: null,
  opening: null,
  usesMemory: false,
  tools: "",
};

/**
 * Les figures, sur leur écran.
 *
 * Une figure est un rôle et un agencement : des consignes versées au prompt
 * de chacune de ses conversations, un modèle, une phrase d'ouverture, et
 * l'accès ou non à la mémoire de l'utilisateur. Elles vivent sur le serveur —
 * ce qu'on configure ici se retrouve sur l'ordinateur, et réciproquement.
 */
export function FiguresScreen({ onBack, onOpenChat }: Props) {
  const [figures, setFigures] = useState<PhoneFigure[] | null>(null);
  const [edit, setEdit] = useState<FigureDraft | null>(null);
  /** Les conversations de chaque figure, par identifiant de figure. */
  const [sessions, setSessions] = useState<Record<string, PhoneFigureSession[]>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const liste = await api.listFigures();
      setFigures(liste);
      setError(null);
      // Les conversations de chaque figure, en parallèle : l'écran les montre
      // pour reprendre d'un tap. Une figure muette ne bloque pas ses voisines.
      const parFigure: Record<string, PhoneFigureSession[]> = {};
      await Promise.all(
        liste.map(async (f) => {
          try {
            parFigure[f.id] = await api.figureSessions(f.id);
          } catch {
            parFigure[f.id] = [];
          }
        }),
      );
      setSessions(parFigure);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function act(key: string, run: () => Promise<unknown>) {
    setBusy(key);
    setError(null);
    try {
      await run();
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function ouvrir(f: PhoneFigure) {
    setBusy(`ouvrir:${f.id}`);
    setError(null);
    try {
      onOpenChat(await api.startFigureChat(f.id));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function enregistrer() {
    if (!edit) return;
    if (!edit.name.trim() || !edit.instructions.trim()) {
      setError("Une figure a besoin d'un nom et de consignes. Le reste est facultatif.");
      return;
    }
    await act("save", () =>
      api.saveFigure({
        ...edit,
        model: edit.model?.trim() || null,
        opening: edit.opening?.trim() || null,
      }),
    );
    setEdit(null);
  }

  async function supprimer(f: PhoneFigure) {
    await act(`del:${f.id}`, () => api.deleteFigure(f.id));
  }

  // Le formulaire, création comme édition : même endroit, mêmes champs.
  if (edit) {
    const c = edit;
    return (
      <Screen
        title={c.name ? `Figure : ${c.name}` : "Nouvelle figure"}
        onBack={() => setEdit(null)}
      >
        <p className="lo-hint">
          Le nom et les consignes suffisent. Le reste est facultatif : une figure de trois lignes
          est une figure valable.
        </p>
        {error && <p className="lo-error">{error}</p>}
        <ul className="lo-cards">
          <li className="lo-card">
            <div className="lo-card-text">
              <span className="lo-card-title">Nom</span>
              <span className="lo-hint">Ce que vous lisez dans la liste.</span>
            </div>
            <input
              className="lo-input"
              value={c.name}
              placeholder="relecteur"
              onChange={(e) => setEdit({ ...c, name: e.target.value })}
            />
          </li>
          <li className="lo-card">
            <div className="lo-card-text">
              <span className="lo-card-title">Description</span>
              <span className="lo-hint">Une phrase pour la retrouver.</span>
            </div>
            <input
              className="lo-input"
              value={c.description}
              onChange={(e) => setEdit({ ...c, description: e.target.value })}
            />
          </li>
          <li className="lo-card">
            <div className="lo-card-text">
              <span className="lo-card-title">Consignes</span>
              <span className="lo-hint">
                Ce que le modèle reçoit avant chaque conversation. C'est le cœur.
              </span>
            </div>
            <textarea
              className="lo-textarea"
              rows={6}
              value={c.instructions}
              onChange={(e) => setEdit({ ...c, instructions: e.target.value })}
            />
          </li>
          <li className="lo-card">
            <div className="lo-card-text">
              <span className="lo-card-title">Modèle</span>
              <span className="lo-hint">Vide : celui de l'application.</span>
            </div>
            <input
              className="lo-input"
              value={c.model ?? ""}
              placeholder="celui de l'application"
              onChange={(e) => setEdit({ ...c, model: e.target.value })}
            />
          </li>
          <li className="lo-card">
            <div className="lo-card-text">
              <span className="lo-card-title">Ouverture</span>
              <span className="lo-hint">Une première phrase, envoyée d'office à l'ouverture.</span>
            </div>
            <input
              className="lo-input"
              value={c.opening ?? ""}
              onChange={(e) => setEdit({ ...c, opening: e.target.value })}
            />
          </li>
          <li className="lo-card">
            <div className="lo-card-text">
              <span className="lo-card-title">Outils</span>
              <span className="lo-hint">
                Ce qu'elle a le droit d'appeler, séparé par des virgules. Vide : tout ce que
                l'application propose.
              </span>
            </div>
            <input
              className="lo-input"
              value={c.tools}
              placeholder="generate_image, generate_speech"
              onChange={(e) => setEdit({ ...c, tools: e.target.value })}
            />
          </li>
          <li className="lo-card">
            <div className="lo-card-text">
              <span className="lo-card-title">Mémoire</span>
              <span className="lo-hint">La figure lit ce que le service retient de vous.</span>
            </div>
            <div className="lo-card-actions">
              <LoSwitch
                checked={c.usesMemory}
                onChange={(usesMemory) => setEdit({ ...c, usesMemory })}
                label="La figure lit la mémoire"
              />
            </div>
          </li>
        </ul>
        <div className="lo-row">
          <button type="button" className="lo-btn-ghost" onClick={() => setEdit(null)}>
            Annuler
          </button>
          <button
            type="button"
            className="lo-btn"
            disabled={busy === "save"}
            onClick={() => void enregistrer()}
          >
            {busy === "save" ? "…" : c.name ? "Enregistrer" : "Créer la figure"}
          </button>
        </div>
      </Screen>
    );
  }

  return (
    <Screen
      title="Figures"
      onBack={onBack}
      action={
        <button type="button" className="lo-bar-action" onClick={() => setEdit(vierge)}>
          Nouvelle
        </button>
      }
    >
      <p className="lo-hint">
        Un rôle, ses consignes, ses conversations. Configurées une fois, elles se retrouvent telles
        quelles à chaque ouverture — sur le téléphone comme sur l'ordinateur.
      </p>
      {figures === null && !error && <p className="lo-sub">Chargement…</p>}
      {error && <p className="lo-error">{error}</p>}
      {figures?.length === 0 && (
        <p className="lo-sub">
          Aucune figure. Écrivez-en une : un nom, et ce que le modèle doit faire.
        </p>
      )}
      <ul className="lo-cards">
        {figures?.map((f) => (
          <li key={f.id} className="lo-card">
            <div className="lo-card-text">
              <span className="lo-card-title">{f.name}</span>
              {f.description && <span className="lo-hint">{f.description}</span>}
              {f.opening && <span className="lo-tag">« {f.opening} »</span>}
            </div>
            {sessions[f.id]?.length > 0 && (
              <div className="lo-figure-sessions">
                <span className="lo-hint">Conversations</span>
                {sessions[f.id].map((s) => (
                  <button
                    key={s.id}
                    type="button"
                    className="lo-figure-session"
                    onClick={() => onOpenChat(s.id)}
                  >
                    {s.title || "Sans titre"}
                  </button>
                ))}
              </div>
            )}
            <div className="lo-card-actions">
              <button
                type="button"
                className="lo-btn-small lo-btn-small-on"
                disabled={busy === `ouvrir:${f.id}`}
                onClick={() => void ouvrir(f)}
              >
                {busy === `ouvrir:${f.id}` ? "…" : "Ouvrir"}
              </button>
              <button
                type="button"
                className="lo-btn-small"
                disabled={busy === `del:${f.id}`}
                onClick={() =>
                  setEdit({
                    name: f.name,
                    description: f.description,
                    instructions: f.instructions,
                    model: f.model,
                    opening: f.opening,
                    usesMemory: f.uses_memory,
                    tools: (f.tools ?? []).join(", "),
                  })
                }
              >
                Modifier
              </button>
              <button
                type="button"
                className="lo-btn-small"
                disabled={busy === `del:${f.id}`}
                onClick={() => void supprimer(f)}
              >
                Retirer
              </button>
            </div>
          </li>
        ))}
      </ul>
    </Screen>
  );
}
