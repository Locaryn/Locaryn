import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type MobileStatus, api } from "../lib/core";
import { ExtensionSettings } from "./ExtensionSettings";
import { Screen } from "./Screen";
import { VersionSection } from "./VersionSection";

type Props = {
  /** Absent tant que personne n'est connecté : l'écran reste utilisable. */
  status: MobileStatus | null;
  onBack: () => void;
  onSignedOut: (s: MobileStatus) => void;
  onMemory: () => void;
};

/**
 * Réglages.
 *
 * Trois choses seulement, parce que le téléphone n'en décide que trois : à
 * quel serveur il parle, ce que ce serveur retient de son utilisateur, et
 * comment se mettre à jour. Tout le reste — modèles, extensions, comptes — est
 * une décision de la machine à l'autre bout, et se prend là-bas.
 *
 * L'écran s'ouvre **avant** toute connexion. Le cas est concret : le serveur
 * passe à une version que le téléphone ne sait pas encore parler, la connexion
 * échoue, et si la mise à jour n'était atteignable qu'une fois connecté, il n'y
 * aurait aucune façon d'en sortir. Ce qui demande un serveur — la mémoire, la
 * déconnexion — s'efface tant qu'il n'y en a pas ; la version reste.
 */
export function Settings({ status, onBack, onSignedOut, onMemory }: Props) {
  return (
    <Screen title="Réglages" onBack={onBack}>
      <VersionSection />

      {/* Ce que les extensions ajoutent. Une extension de dictée doit faire
          choisir son modèle ici comme sur l'ordinateur : c'est le même
          serveur qui exécutera. Rien ne s'affiche s'il n'y en a pas. */}
      <ExtensionSettings />

      <section className="lo-section">
        <h2 className="lo-section-title">Serveur</h2>
        <p className="lo-hint">
          {status?.server_name ?? "Aucun serveur enregistré"}
          {status?.travelling ? " — joint depuis l'extérieur" : ""}
        </p>
        {status?.signed_in && (
          <button
            type="button"
            className="lo-btn-ghost"
            onClick={() => void api.signOut().then(onSignedOut)}
          >
            Se déconnecter
          </button>
        )}
      </section>

      {/*
        Une ligne, pas une section. La mémoire est une liste qui grandit toute
        seule ; l'étaler ici noierait les réglages qui l'entourent. Elle vit sur
        le serveur : sans session, la proposer mènerait à un écran vide.
      */}
      {status?.signed_in && (
        <section className="lo-section">
          <h2 className="lo-section-title">Personnalisation</h2>
          <button type="button" className="lo-row" onClick={onMemory}>
            <span className="lo-row-icon">
              <Icon name="memory" />
            </span>
            <span className="lo-row-text">
              <span className="lo-row-label">Mémoire</span>
              <span className="lo-hint">Ce que le serveur retient de vous</span>
            </span>
            <span className="lo-row-go">
              <Icon name="chevron" size={16} />
            </span>
          </button>
        </section>
      )}
    </Screen>
  );
}
