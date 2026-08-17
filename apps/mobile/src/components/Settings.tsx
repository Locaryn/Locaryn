import { useCallback, useEffect, useState } from "react";
import { type MobileStatus, api } from "../lib/core";
import { Icon } from "@locaryn/ui-core";
import { Screen } from "./Screen";
import { UpdateButton } from "./UpdateButton";

type Props = {
  status: MobileStatus;
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
 */
export function Settings({ status, onBack, onSignedOut, onMemory }: Props) {
  return (
    <Screen title="Réglages" onBack={onBack} action={<UpdateButton />}>
      <section className="lo-section">
        <h2 className="lo-section-title">Serveur</h2>
        <p className="lo-hint">
          {status.server_name ?? "Aucun"}
          {status.travelling ? " — joint depuis l'extérieur" : ""}
        </p>
        <button
          type="button"
          className="lo-btn-ghost"
          onClick={() => void api.signOut().then(onSignedOut)}
        >
          Se déconnecter
        </button>
      </section>

      {/*
        Une ligne, pas une section. La mémoire est une liste qui grandit toute
        seule ; l'étaler ici noierait les réglages qui l'entourent.
      */}
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
    </Screen>
  );
}
