import { Icon } from "@locaryn/ui-core";
import { open } from "@tauri-apps/plugin-shell";
import { useCallback, useEffect, useState } from "react";
import {
  type PairingCode,
  type PairingState,
  type ServerStatus,
  type ServerUserSummary,
  core,
  coreMode,
} from "../lib/core";
import { UserLoginQr } from "./UserLoginQr";

/**
 * Le téléphone, en mode simple : trois étapes et ce que ça permet, sans un
 * mot technique. Le détail (port, comptes, accès hors de chez soi) reste dans
 * les réglages avancés, section « Serveur & fonctions ».
 */

const PEUT = [
  "Discuter avec vos modèles depuis le téléphone, où que vous soyez dans la maison.",
  "Retrouver vos conversations, commencées ici ou là-bas.",
];

const LIMITES = [
  "Cet ordinateur fait le travail : il doit rester allumé, avec Locaryn ouvert ou réduit.",
  "Le téléphone doit être sur le même Wi‑Fi que l'ordinateur. Hors de chez vous, il ne le trouvera pas.",
  "Les réponses ne vont pas plus vite que sur l'ordinateur.",
  "Installer des modèles et changer les réglages se fait sur l'ordinateur.",
  "L'app existe pour Android. Sur iPhone, elle n'est pas encore disponible.",
];

function Liste({ items, icone }: { items: string[]; icone: "check" | "info" }) {
  return (
    <ul className="locaryn-phone-liste">
      {items.map((t) => (
        <li key={t}>
          <Icon name={icone} size={14} />
          <span>{t}</span>
        </li>
      ))}
    </ul>
  );
}

function Etape({
  n,
  titre,
  fait,
  children,
}: {
  n: number;
  titre: string;
  fait: boolean;
  children: React.ReactNode;
}) {
  return (
    <li className={`locaryn-phone-etape${fait ? " locaryn-phone-etape-faite" : ""}`}>
      <span className="locaryn-phone-etape-n" aria-hidden>
        {fait ? <Icon name="check" size={14} /> : n}
      </span>
      <div className="locaryn-phone-etape-corps">
        <h4>{titre}</h4>
        {children}
      </div>
    </li>
  );
}

function sansErreur(e: unknown): string {
  return String(e).replace(/^Error:\s*/, "");
}

export function PhoneSimpleSettings() {
  const [app, setApp] = useState<{ url: string; qr_svg: string } | null>(null);
  const [server, setServer] = useState<ServerStatus | null>(null);
  const [comptes, setComptes] = useState<ServerUserSummary[]>([]);
  const [code, setCode] = useState<PairingCode | null>(null);
  const [appairage, setAppairage] = useState<PairingState | null>(null);
  const [nom, setNom] = useState("");
  const [mdp, setMdp] = useState("");
  const [occupe, setOccupe] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);
  const [qrConnexion, setQrConnexion] = useState(false);

  const relire = useCallback(async () => {
    try {
      const s = await core.serverStatus();
      setServer(s);
      setComptes(await core.listServerUsers());
    } catch (e) {
      console.warn("[Téléphone] état du partage illisible :", e);
    }
  }, []);

  useEffect(() => {
    core
      .appTelephone()
      .then(setApp)
      .catch((e: unknown) => console.warn("[Téléphone] lien de l'app indisponible :", e));
    void relire();
    const t = window.setInterval(() => void relire(), 4000);
    return () => window.clearInterval(t);
  }, [relire]);

  const ouvert = Boolean(server?.running);

  // Le QR de liaison dès que l'accès est ouvert.
  useEffect(() => {
    if (!ouvert) {
      setCode(null);
      return;
    }
    let annule = false;
    core
      .pairingCode("local")
      .then((c) => {
        if (!annule) setCode(c);
      })
      .catch((e: unknown) => {
        if (!annule) setErreur(sansErreur(e));
      });
    return () => {
      annule = true;
    };
  }, [ouvert]);

  // Qui scanne, pour afficher le code à taper sur le téléphone.
  useEffect(() => {
    if (!code?.qr_svg) return;
    let annule = false;
    const lire = () => {
      core
        .pairingState()
        .then((e) => {
          if (!annule) setAppairage(e);
        })
        .catch((e: unknown) => console.warn("[Téléphone] appairage illisible :", e));
    };
    lire();
    const t = window.setInterval(lire, 2000);
    return () => {
      annule = true;
      window.clearInterval(t);
    };
  }, [code?.qr_svg]);

  async function ouvrir() {
    setErreur(null);
    setOccupe(true);
    try {
      if (server && server.accounts === 0) {
        if (nom.trim() === "") throw new Error("Choisissez un nom.");
        if (mdp.length < 8) throw new Error("Le mot de passe doit faire au moins 8 caractères.");
        await core.createServerUser(nom.trim(), mdp, true);
        setMdp("");
      }
      setServer(await core.setServerMode(true));
      await relire();
    } catch (e) {
      setErreur(sansErreur(e));
    } finally {
      setOccupe(false);
    }
  }

  async function fermer() {
    setErreur(null);
    setOccupe(true);
    try {
      setServer(await core.setServerMode(false));
    } catch (e) {
      setErreur(sansErreur(e));
    } finally {
      setOccupe(false);
    }
  }

  async function refuserAppareil() {
    try {
      await core.rejectPairing();
      setAppairage(null);
      setCode(await core.pairingCode("local"));
    } catch (e) {
      setErreur(sansErreur(e));
    }
  }

  const sansCompte = server !== null && server.accounts === 0;
  const compte = comptes.find((c) => c.role === "admin") ?? comptes[0] ?? null;

  return (
    <section className="locaryn-phone">
      <div className="locaryn-phone-intro">
        <Icon name="device-mobile" size={22} />
        <div>
          <h3>Utiliser Locaryn sur votre téléphone</h3>
          <p>Le téléphone se sert des modèles de cet ordinateur. Rien ne passe par Internet.</p>
        </div>
      </div>

      <div className="locaryn-phone-colonnes">
        <div>
          <h4 className="locaryn-phone-sous-titre">Ce que vous pourrez faire</h4>
          <Liste items={PEUT} icone="check" />
        </div>
        <div>
          <h4 className="locaryn-phone-sous-titre">Bon à savoir</h4>
          <Liste items={LIMITES} icone="info" />
        </div>
      </div>

      <ol className="locaryn-phone-etapes">
        <Etape n={1} titre="Installer l'app sur le téléphone" fait={false}>
          <p>
            Scannez ce code avec l'appareil photo du téléphone Android, puis ouvrez le fichier
            téléchargé. Le téléphone peut demander d'autoriser l'installation : acceptez pour
            Locaryn.
          </p>
          {app && (
            <div className="locaryn-phone-qr-ligne">
              <div
                className="locaryn-phone-qr"
                // biome-ignore lint/security/noDangerouslySetInnerHtml: SVG produit localement à partir d'une adresse fixe.
                dangerouslySetInnerHTML={{ __html: app.qr_svg }}
              />
              {coreMode === "tauri" && (
                <button
                  type="button"
                  className="locaryn-btn-ghost"
                  onClick={() => void open(app.url).catch((e: unknown) => setErreur(sansErreur(e)))}
                >
                  Ouvrir le lien sur l'ordinateur
                </button>
              )}
            </div>
          )}
        </Etape>

        <Etape n={2} titre="Ouvrir l'accès au téléphone" fait={ouvert}>
          {ouvert ? (
            <div className="locaryn-phone-ligne">
              <p>L'accès est ouvert. Vous pouvez le fermer à tout moment.</p>
              <button
                type="button"
                className="locaryn-btn-ghost"
                disabled={occupe}
                onClick={() => void fermer()}
              >
                Fermer l'accès
              </button>
            </div>
          ) : (
            <>
              <p>
                {sansCompte
                  ? "Choisissez un nom et un mot de passe : ils empêchent quelqu'un d'autre sur le Wi‑Fi de se servir de votre ordinateur."
                  : "Votre ordinateur accepte alors les téléphones que vous reliez vous-même."}
              </p>
              {sansCompte && (
                <div className="locaryn-phone-form">
                  <label>
                    Nom
                    <input
                      className="locaryn-input"
                      value={nom}
                      autoComplete="username"
                      disabled={occupe}
                      onChange={(e) => setNom(e.target.value)}
                    />
                  </label>
                  <label>
                    Mot de passe
                    <input
                      className="locaryn-input"
                      type="password"
                      value={mdp}
                      autoComplete="new-password"
                      disabled={occupe}
                      onChange={(e) => setMdp(e.target.value)}
                    />
                  </label>
                </div>
              )}
              {server?.blocker && <p className="locaryn-field-hint">{server.blocker}</p>}
              <button
                type="button"
                className="locaryn-btn-primary"
                disabled={occupe || Boolean(server?.blocker)}
                onClick={() => void ouvrir()}
              >
                {occupe ? "Ouverture…" : "Ouvrir l'accès"}
              </button>
              <p className="locaryn-field-hint">
                Windows peut demander d'autoriser Locaryn sur le réseau : choisissez « réseaux
                privés ».
              </p>
            </>
          )}
        </Etape>

        <Etape n={3} titre="Relier le téléphone" fait={false}>
          {!ouvert ? (
            <p className="locaryn-field-hint">Ouvrez d'abord l'accès (étape 2).</p>
          ) : (
            <>
              <p>
                Visez ce code avec l'appareil photo du téléphone, ou touchez « Scanner un QR code »
                dans l'app Locaryn.
              </p>
              {code?.qr_svg && (
                <div
                  className="locaryn-phone-qr"
                  // biome-ignore lint/security/noDangerouslySetInnerHtml: SVG produit par le service local.
                  dangerouslySetInnerHTML={{ __html: code.qr_svg }}
                />
              )}
              {appairage?.announced && appairage.pairing_code && (
                <div className="locaryn-phone-code">
                  <p>
                    <strong>{appairage.device || "Un téléphone"}</strong> veut se relier. Tapez ce
                    code sur le téléphone :
                  </p>
                  <p className="locaryn-phone-code-valeur">{appairage.pairing_code}</p>
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    onClick={() => void refuserAppareil()}
                  >
                    Ce n'est pas mon téléphone
                  </button>
                </div>
              )}
              {compte && (
                <div className="locaryn-phone-ligne">
                  <p>
                    Le téléphone demande ensuite de se connecter. Montrez-lui ce code pour ne pas
                    taper le mot de passe.
                  </p>
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    onClick={() => setQrConnexion(true)}
                  >
                    Code de connexion
                  </button>
                </div>
              )}
            </>
          )}
        </Etape>
      </ol>

      {erreur && <p className="locaryn-vp-error">{erreur}</p>}
      <p className="locaryn-field-hint">
        Accès depuis l'extérieur, plusieurs comptes, port : Réglages avancés → Serveur & fonctions.
      </p>

      {qrConnexion && compte && (
        <UserLoginQr
          userId={compte.id}
          username={compte.username}
          onClose={() => setQrConnexion(false)}
        />
      )}
    </section>
  );
}
