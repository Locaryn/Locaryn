import { Icon } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import { core } from "../lib/core";
import { ImageModelSetting } from "./ImageModelSetting";
import { MicroModelSetting } from "./MicroModelSetting";
import { TtsModelSetting } from "./TtsModelSetting";

/** Central preferences for every default model used outside ordinary chat. */
export function ModelPreferencesSettings({
  activeCapabilities = [],
}: {
  activeCapabilities?: string[];
}) {
  const [caps, setCaps] = useState<string[]>(activeCapabilities);

  useEffect(() => {
    if (activeCapabilities.length > 0) {
      setCaps(activeCapabilities);
      return;
    }
    let cancelled = false;
    core
      .listExtensions()
      .then((exts) => {
        if (cancelled) return;
        const discovered = exts.filter((e) => e.enabled).flatMap((e) => e.capabilities ?? []);
        if (discovered.length > 0) {
          setCaps(discovered);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [activeCapabilities]);

  const hasTts =
    caps.includes("voice-tts") || caps.includes("voice-cloning") || activeCapabilities.length === 0;
  const hasImageGen =
    caps.includes("image-gen") || caps.includes("image-editor") || activeCapabilities.length === 0;
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

        {hasTts && (
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
        )}
        {hasImageGen && (
          <section className="locaryn-model-preference-card">
            <div className="locaryn-model-preference-heading">
              <Icon name="image" size={17} />
              <div>
                <h4>Génération d'images</h4>
                <p>Modèle de diffusion par défaut pour le Studio et les illustrations du chat.</p>
              </div>
            </div>
            <ImageModelSetting />
          </section>
        )}
      </div>
    </div>
  );
}
