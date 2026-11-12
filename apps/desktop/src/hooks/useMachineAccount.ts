import { useEffect, useState } from "react";
import { MACHINE_ACCOUNT_EVENT, type MachineAccount, core } from "../lib/core";

/**
 * Le compte du serveur lié à cette machine, relu à chaque changement annoncé
 * par `MACHINE_ACCOUNT_EVENT`. `null` : profil local.
 */
export function useMachineAccount(): MachineAccount | null {
  const [compte, setCompte] = useState<MachineAccount | null>(null);
  useEffect(() => {
    let annule = false;
    const lire = () => {
      core
        .machineAccount()
        .then((c) => {
          if (!annule) setCompte(c);
        })
        .catch((e) => console.warn("[Compte] compte de la machine illisible :", e));
    };
    lire();
    window.addEventListener(MACHINE_ACCOUNT_EVENT, lire);
    return () => {
      annule = true;
      window.removeEventListener(MACHINE_ACCOUNT_EVENT, lire);
    };
  }, []);
  return compte;
}
