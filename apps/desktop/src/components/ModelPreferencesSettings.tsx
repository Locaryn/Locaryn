import { Icon } from "@locaryn/ui-core";
import { ImageSettings } from "./ImageSettings";
import { MicroModelSetting } from "./MicroModelSetting";
import { TtsModelSetting } from "./TtsModelSetting";

/** Central preferences for every default model used outside ordinary chat. */
export function ModelPreferencesSettings() {
  return (
    <div className="locaryn-model-preferences">
      <div className="locaryn-model-preferences-intro">
        <div>
          <span className="locaryn-account-eyebrow">PRÉFÉRENCES DU COMPTE</span>
          <h3>Préférences des modèles</h3>
          <p>
            Choisissez ici les modèles utilisés par les tâches secondaires de Locaryn. Le modèle de
            conversation reste configurable depuis le chat.
          </p>
        </div>
        <Icon name="models" size={22} />
      </div>

      <div className="locaryn-model-preferences-stack">
        <section className="locaryn-model-preference-card">
          <div className="locaryn-model-preference-heading">
            <Icon name="memory" size={17} />
            <div>
              <h4>Petites tâches</h4>
              <p>Nommage, rangement et résumés courts sans interrompre le modèle principal.</p>
            </div>
          </div>
          <MicroModelSetting />
        </section>

        <section className="locaryn-model-preference-card">
          <div className="locaryn-model-preference-heading">
            <Icon name="mic" size={17} />
            <div>
              <h4>Synthèse vocale</h4>
              <p>Modèle TTS par défaut pour les notes vocales et le Studio audio.</p>
            </div>
          </div>
          <TtsModelSetting />
        </section>

        <section className="locaryn-model-preference-card">
          <div className="locaryn-model-preference-heading">
            <Icon name="studio" size={17} />
            <div>
              <h4>Génération d'images</h4>
              <p>Qualité, résolution et mémoire utilisés par défaut pour les images.</p>
            </div>
          </div>
          <ImageSettings />
        </section>
      </div>
    </div>
  );
}
