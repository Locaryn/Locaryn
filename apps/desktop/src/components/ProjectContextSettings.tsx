import { Icon, type IconName } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type ContextAvailability, type ContextEntry, type ContextScope, core } from "../lib/core";

/**
 * Ce qu'il faut savoir pour travailler sur ce projet, et qui a le droit de le
 * savoir.
 *
 * **Sans domaine.** Une fiche peut dire « le rendu final est en A2 sur papier
 * grain torchon », « les mesures se font à 20 °C, sinon la dilatation fausse
 * tout », « le client refuse le violet » ou « les tests passent par
 * `cargo test` ». Un projet d'art, un dossier de physique et du code posent la
 * même question à qui arrive dessus : qu'est-ce qui a déjà été décidé, et
 * pourquoi.
 *
 * **La portée se choisit à l'écriture, par la personne.** Se tromper coûte dans
 * les deux sens : une exigence personnelle rangée en partagé impose à tout le
 * monde ce qu'une seule personne voulait ; une décision de projet rangée en
 * personnel laisse les autres l'ignorer et refaire le débat. Chaque choix porte
 * donc sa conséquence écrite à côté, plutôt qu'un nom seul à deviner.
 */

const PORTEES: { id: ContextScope; label: string; consequence: string; icon: IconName }[] = [
  {
    id: "machine",
    label: "Cet ordinateur",
    consequence:
      "Ne part jamais d'ici. Pour ce qui n'a de sens que sur ce poste : outils installés, chemins, versions.",
    icon: "laptop",
  },
  {
    id: "compte",
    label: "Mon compte",
    consequence:
      "Me suit sur mes autres appareils, et reste invisible aux autres personnes du projet. Pour mes exigences à moi.",
    icon: "private",
  },
  {
    id: "partage",
    label: "Tout le projet",
    consequence:
      "Visible de tous ceux qui travaillent sur ce projet. Pour ce que le projet a décidé, pas pour une préférence personnelle.",
    icon: "devices",
  },
];

function quandDit(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const jours = Math.floor((Date.now() - d.getTime()) / 86_400_000);
  if (jours <= 0) return "aujourd'hui";
  if (jours === 1) return "hier";
  if (jours < 7) return `il y a ${jours} jours`;
  return d.toLocaleDateString("fr-FR", { day: "numeric", month: "short" });
}

export function ProjectContextSettings({ projectId }: { projectId: string }) {
  const [fiches, setFiches] = useState<ContextEntry[]>([]);
  const [dispo, setDispo] = useState<ContextAvailability | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [ouverte, setOuverte] = useState<string | null>(null);

  const [titre, setTitre] = useState("");
  const [detail, setDetail] = useState("");
  const [portee, setPortee] = useState<ContextScope>("machine");
  const [occupe, setOccupe] = useState(false);

  const charger = useCallback(async () => {
    try {
      const [f, d] = await Promise.all([core.listContext(projectId), core.contextAvailability()]);
      setFiches(f);
      setDispo(d);
      setErreur(null);
    } catch (e) {
      setErreur(String(e).replace(/^Error:\s*/, ""));
    }
  }, [projectId]);

  useEffect(() => {
    void charger();
  }, [charger]);

  async function ajouter() {
    const t = titre.trim();
    const d = detail.trim();
    if (!t || !d || occupe) return;
    setOccupe(true);
    setErreur(null);
    try {
      await core.rememberContext({ projectId, scope: portee, title: t, detail: d });
      setTitre("");
      setDetail("");
      await charger();
    } catch (e) {
      setErreur(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setOccupe(false);
    }
  }

  async function changerPortee(id: string, scope: ContextScope) {
    setErreur(null);
    try {
      await core.setContextScope(id, scope);
      await charger();
    } catch (e) {
      setErreur(String(e).replace(/^Error:\s*/, ""));
    }
  }

  async function oublier(id: string) {
    setErreur(null);
    try {
      await core.forgetContext(id);
      setOuverte(null);
      await charger();
    } catch (e) {
      setErreur(String(e).replace(/^Error:\s*/, ""));
    }
  }

  const utilisables = dispo?.scopes ?? ["machine"];
  const fiche = fiches.find((f) => f.id === ouverte) ?? null;

  return (
    <div className="locaryn-ctx-settings">
      <div className="locaryn-memory-intro">
        <div>
          <span className="locaryn-account-eyebrow">CONTEXTE DU PROJET</span>
          <h3>Ce qu'il faut savoir pour travailler ici</h3>
          <p>
            Une fiche par chose décidée — un format de rendu, une contrainte de mesure, une exigence
            de relecture, une commande à lancer. Locaryn les relit avant de répondre, et la portée
            dit qui les voit.
          </p>
        </div>
        <span className="locaryn-memory-count">
          {fiches.length} fiche{fiches.length === 1 ? "" : "s"}
        </span>
      </div>

      {erreur && <div className="locaryn-vp-error">{erreur}</div>}

      {dispo?.blocker && (
        <div className="locaryn-ctx-blocage" role="note">
          <Icon name="info" size={15} />
          <p>{dispo.blocker}</p>
        </div>
      )}

      {fiche ? (
        <div className="locaryn-memory-detail">
          <div className="locaryn-memory-detail-head">
            <button
              type="button"
              className="locaryn-memory-detail-back"
              onClick={() => setOuverte(null)}
            >
              <Icon name="back" size={15} /> Contexte
            </button>
            <button
              type="button"
              className="locaryn-btn-ghost locaryn-btn-danger"
              onClick={() => void oublier(fiche.id)}
            >
              Supprimer
            </button>
          </div>

          <h3 className="locaryn-memory-detail-title">{fiche.title}</h3>
          {fiche.author && <p className="locaryn-field-hint">Posée par {fiche.author}.</p>}

          <div className="locaryn-memory-detail-section">
            <p className="locaryn-memory-detail-label">Qui la voit</p>
            <div className="locaryn-ctx-portees">
              {PORTEES.map((p) => {
                const permise = utilisables.includes(p.id);
                return (
                  <button
                    key={p.id}
                    type="button"
                    className={`locaryn-ctx-portee${fiche.scope === p.id ? " locaryn-active" : ""}`}
                    disabled={!permise || fiche.scope === p.id}
                    title={permise ? p.consequence : (dispo?.blocker ?? "")}
                    onClick={() => void changerPortee(fiche.id, p.id)}
                  >
                    <Icon name={p.icon} size={14} /> {p.label}
                  </button>
                );
              })}
            </div>
            <p className="locaryn-field-hint">
              {PORTEES.find((p) => p.id === fiche.scope)?.consequence}
            </p>
          </div>

          {fiche.details.length > 0 && (
            <div className="locaryn-memory-detail-section">
              <p className="locaryn-memory-detail-label">Détails</p>
              <div className="locaryn-memory-detail-list">
                {fiche.details.map((d) => (
                  <div key={d} className="locaryn-memory-detail-item">
                    <span>{d}</span>
                    <button
                      type="button"
                      className="locaryn-memory-detail-item-remove"
                      title="Retirer ce détail"
                      onClick={() => {
                        void core
                          .removeContextDetail(fiche.id, d)
                          .then(charger)
                          .catch((e) => setErreur(String(e).replace(/^Error:\s*/, "")));
                      }}
                    >
                      <Icon name="close" size={13} />
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      ) : (
        <>
          {fiches.length === 0 ? (
            <div className="locaryn-memory-empty">
              <strong>Aucune fiche pour ce projet.</strong>
              <span>Ajoutez ce qu'il faut savoir pour y travailler.</span>
            </div>
          ) : (
            <div className="locaryn-memory-group-rows">
              {fiches.map((f) => (
                <button
                  key={f.id}
                  type="button"
                  className="locaryn-memory-row"
                  onClick={() => setOuverte(f.id)}
                >
                  <span className="locaryn-memory-row-main">
                    <span className="locaryn-memory-row-title">
                      {f.title}
                      <span className={`locaryn-ctx-etiquette locaryn-ctx-${f.scope}`}>
                        {PORTEES.find((p) => p.id === f.scope)?.label}
                      </span>
                    </span>
                    <span className="locaryn-memory-row-summary">{f.summary}</span>
                  </span>
                  <span className="locaryn-memory-row-date">{quandDit(f.updated_at)}</span>
                </button>
              ))}
            </div>
          )}

          <form
            className="locaryn-ctx-ajout"
            onSubmit={(e) => {
              e.preventDefault();
              void ajouter();
            }}
          >
            <div className="locaryn-field">
              <label htmlFor="ctx-titre" className="locaryn-field-label">
                Sujet
              </label>
              <input
                id="ctx-titre"
                type="text"
                value={titre}
                placeholder="Format de rendu, Contraintes de mesure, Relecture…"
                onChange={(e) => setTitre(e.target.value)}
                disabled={occupe}
              />
            </div>

            <div className="locaryn-field">
              <label htmlFor="ctx-detail" className="locaryn-field-label">
                Ce qu'il faut savoir
              </label>
              <textarea
                id="ctx-detail"
                rows={2}
                value={detail}
                placeholder="Le rendu final est en A2 sur papier grain torchon."
                onChange={(e) => setDetail(e.target.value)}
                disabled={occupe}
              />
            </div>

            <div className="locaryn-field">
              <div className="locaryn-field-label">Qui doit le voir</div>
              <div className="locaryn-ctx-portees">
                {PORTEES.map((p) => {
                  const permise = utilisables.includes(p.id);
                  return (
                    <button
                      key={p.id}
                      type="button"
                      className={`locaryn-ctx-portee${portee === p.id ? " locaryn-active" : ""}`}
                      disabled={!permise}
                      title={permise ? p.consequence : (dispo?.blocker ?? "")}
                      onClick={() => setPortee(p.id)}
                    >
                      <Icon name={p.icon} size={14} /> {p.label}
                    </button>
                  );
                })}
              </div>
              {/* La conséquence du choix, écrite : un nom seul se devine mal, et
                  se tromper de portée coûte dans les deux sens. */}
              <p className="locaryn-field-hint">
                {PORTEES.find((p) => p.id === portee)?.consequence}
              </p>
            </div>

            <button
              type="submit"
              className="locaryn-btn-primary"
              disabled={occupe || !titre.trim() || !detail.trim()}
            >
              <Icon name="check" size={14} /> Ajouter au contexte
            </button>
          </form>
        </>
      )}
    </div>
  );
}
