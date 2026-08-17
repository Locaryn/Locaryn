import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type Figure, type Session, core } from "../lib/core";

type Props = {
  /** Ouvrir une conversation de la figure. */
  onOpenSession: (s: Session) => void;
  /** Commencer une conversation tenue par cette figure. */
  onNewWithFigure: (f: Figure) => void;
};

const VIDE = {
  name: "",
  description: "",
  instructions: "",
  model: "",
  opening: "",
  usesMemory: false,
};

/**
 * Les figures.
 *
 * Une figure est un rôle **et** un agencement : elle donne un caractère au
 * modèle et écrit d'avance ce qu'il doit faire. Ses consignes sont versées au
 * prompt système de chacune de ses conversations, devant la mémoire de
 * l'utilisateur — le rôle qu'on lui a donné prime.
 *
 * L'écran n'existe que si une extension apporte la capacité `figures` ; la
 * retirer le fait disparaître, et les figures écrites restent en attendant.
 */
export function FiguresView({ onOpenSession, onNewWithFigure }: Props) {
  const [figures, setFigures] = useState<Figure[] | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [draft, setDraft] = useState({ ...VIDE });
  const [sessions, setSessions] = useState<Session[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setFigures(await core.listFigures());
      setError(null);
    } catch (e) {
      setError(String(e));
      setFigures([]);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const choisie = figures?.find((f) => f.id === selected) ?? null;

  async function ouvrir(f: Figure) {
    setSelected(f.id);
    setDraft({
      name: f.name,
      description: f.description,
      instructions: f.instructions,
      model: f.model ?? "",
      opening: f.opening ?? "",
      usesMemory: f.uses_memory,
    });
    try {
      setSessions(await core.figureSessions(f.id));
    } catch {
      setSessions([]);
    }
  }

  function nouvelle() {
    setSelected(null);
    setDraft({ ...VIDE });
    setSessions([]);
  }

  async function enregistrer() {
    if (!draft.name.trim() || !draft.instructions.trim()) {
      setError("Une figure a besoin d'un nom et de consignes. Le reste est facultatif.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const f = await core.saveFigure({
        name: draft.name,
        description: draft.description,
        instructions: draft.instructions,
        model: draft.model.trim() || null,
        opening: draft.opening.trim() || null,
        usesMemory: draft.usesMemory,
      });
      await reload();
      setSelected(f.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function supprimer(f: Figure) {
    setBusy(true);
    try {
      await core.deleteFigure(f.id);
      await reload();
      if (selected === f.id) nouvelle();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="locaryn-figures">
      <aside className="locaryn-figures-list">
        <button type="button" className="locaryn-newchat-full" onClick={nouvelle}>
          <Icon name="plus" size={15} /> Nouvelle figure
        </button>

        {figures === null && <p className="locaryn-field-hint">Chargement…</p>}
        {figures?.length === 0 && (
          <p className="locaryn-field-hint">
            Aucune figure. Écrivez-en une : un nom, et ce que le modèle doit faire.
          </p>
        )}

        <ul className="locaryn-tree">
          {figures?.map((f) => (
            <li key={f.id}>
              <button
                type="button"
                className={`locaryn-tree-item${f.id === selected ? " locaryn-active" : ""}`}
                onClick={() => void ouvrir(f)}
              >
                <Icon name="figures" size={14} /> {f.name}
              </button>
            </li>
          ))}
        </ul>
      </aside>

      <section className="locaryn-figures-edit">
        <div className="locaryn-field">
          <div className="locaryn-field-label">Nom</div>
          <input
            className="locaryn-input"
            value={draft.name}
            placeholder="Relecteur"
            onChange={(e) => setDraft({ ...draft, name: e.target.value })}
          />
        </div>

        <div className="locaryn-field">
          <div className="locaryn-field-label">En une ligne</div>
          <input
            className="locaryn-input"
            value={draft.description}
            placeholder="Relit un diff et signale ce qui casse."
            onChange={(e) => setDraft({ ...draft, description: e.target.value })}
          />
        </div>

        <div className="locaryn-field">
          <div className="locaryn-field-label">Consignes</div>
          <p className="locaryn-field-hint">
            C'est le cœur : ce que le modèle reçoit avant chaque conversation de cette figure.
            Écrivez ce qu'il doit faire, et surtout ce qu'il ne doit pas faire — une consigne
            négative tient mieux qu'un ton demandé.
          </p>
          <textarea
            className="locaryn-input locaryn-figures-instructions"
            rows={12}
            value={draft.instructions}
            placeholder={
              "Tu relis du code. Tu ne signales que ce qui peut casser…\n\n" +
              "Tu ne parles ni de style, ni de nommage."
            }
            onChange={(e) => setDraft({ ...draft, instructions: e.target.value })}
          />
        </div>

        <div className="locaryn-field">
          <div className="locaryn-field-label">Première phrase</div>
          <p className="locaryn-field-hint">
            Proposée à l'ouverture d'une conversation. Laissez vide pour commencer sur une page
            blanche.
          </p>
          <input
            className="locaryn-input"
            value={draft.opening}
            placeholder="Colle le diff à relire."
            onChange={(e) => setDraft({ ...draft, opening: e.target.value })}
          />
        </div>

        <div className="locaryn-field">
          <label className="locaryn-srv-toggle">
            <input
              type="checkbox"
              checked={draft.usesMemory}
              onChange={(e) => setDraft({ ...draft, usesMemory: e.target.checked })}
            />
            <span>Cette figure lit ce que le service retient de vous</span>
          </label>
          <p className="locaryn-field-hint">
            Décochée, elle travaille à part : elle ne sait rien de vos préférences ni de vos
            projets, et ne répondra qu'à partir de ses consignes.
          </p>
        </div>

        <div className="locaryn-srv-row">
          <button type="button" className="locaryn-btn" disabled={busy} onClick={enregistrer}>
            {busy ? "…" : choisie ? "Enregistrer" : "Créer la figure"}
          </button>
          {choisie && (
            <>
              <button
                type="button"
                className="locaryn-btn-ghost"
                onClick={() => onNewWithFigure(choisie)}
              >
                <Icon name="chat" size={15} /> Ouvrir une conversation
              </button>
              <button
                type="button"
                className="locaryn-btn-ghost"
                disabled={busy}
                onClick={() => void supprimer(choisie)}
              >
                <Icon name="trash" size={15} /> Supprimer
              </button>
            </>
          )}
        </div>

        {error && <div className="locaryn-vp-error">{error}</div>}

        {choisie && (
          <div className="locaryn-field" style={{ marginTop: 24 }}>
            <div className="locaryn-field-label">Ses conversations</div>
            {sessions.length === 0 ? (
              <p className="locaryn-field-hint">Aucune pour l'instant.</p>
            ) : (
              <ul className="locaryn-tree">
                {sessions.map((s) => (
                  <li key={s.id}>
                    <button
                      type="button"
                      className="locaryn-tree-item"
                      onClick={() => onOpenSession(s)}
                    >
                      <Icon name="chat" size={14} /> {s.title ?? "Conversation"}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </section>
    </div>
  );
}
