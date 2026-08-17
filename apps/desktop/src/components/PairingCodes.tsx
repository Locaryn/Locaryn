import { useCallback, useEffect, useRef, useState } from "react";
import { type PairingCode, type PairingMode, core } from "../lib/core";

/**
 * Appairer un téléphone.
 *
 * Trois façons de joindre cette machine, donc trois codes, et un sélecteur
 * pour passer de l'un à l'autre. Ils portent la même chose — l'autorité du
 * déploiement — et diffèrent par l'adresse : celle du réseau local, celle d'un
 * relais, ou celle d'un port redirigé. Le code est le seul chemin qui apporte
 * le certificat ; une adresse tapée à la main sur le téléphone ne l'apporte
 * pas, et ne permettra pas de joindre la machine de l'extérieur.
 */
const MODES: { id: PairingMode; label: string; explication: string }[] = [
  {
    id: "local",
    label: "Réseau local",
    explication:
      "Le téléphone est sur le même Wi-Fi. Rien à ouvrir, rien à traverser : c'est le cas le plus courant, et le plus sûr.",
  },
  {
    id: "tunnel",
    label: "Tunnel sortant",
    explication:
      "La machine appelle un relais et le téléphone contacte le relais. Rien n'est ouvert sur la box. Demande le mode voyage actif.",
  },
  {
    id: "public",
    label: "Port ouvert",
    explication:
      "Un port a été redirigé vers cette machine, ou elle a une adresse fixe. C'est le seul cas où quelque chose est joignable depuis Internet : ne l'utilisez que si vous l'avez décidé.",
  },
];

export function PairingCodes() {
  const [mode, setMode] = useState<PairingMode>("local");
  const [adresse, setAdresse] = useState("");
  const [code, setCode] = useState<PairingCode | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** De quel côté le code arrive : celui d'où l'on vient. */
  const [sens, setSens] = useState<"gauche" | "droite">("droite");
  const precedent = useRef<PairingMode>("local");

  const charger = useCallback(async (m: PairingMode, url?: string) => {
    setBusy(true);
    setError(null);
    setCode(null);
    try {
      setCode(await core.pairingCode(m, url));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  // « Port ouvert » attend une adresse : la demander avant de la connaître
  // produirait une erreur à chaque ouverture de l'écran.
  useEffect(() => {
    if (mode === "public") return;
    void charger(mode);
  }, [mode, charger]);

  const choisi = MODES.find((m) => m.id === mode);

  return (
    <div className="locaryn-field" style={{ marginTop: 28 }}>
      <div className="locaryn-field-label">Appairer un téléphone</div>
      <p className="locaryn-field-hint">
        Installez Locaryn sur le téléphone, puis photographiez le code. Il porte l'adresse et le
        certificat de cette machine : c'est ce certificat qui permet ensuite de la joindre de
        l'extérieur en toute confiance.
      </p>

      <div className="locaryn-segmented" role="tablist" aria-label="Par où joindre la machine">
        {MODES.map((m) => (
          <button
            key={m.id}
            type="button"
            role="tab"
            aria-selected={mode === m.id}
            className={`locaryn-segment${mode === m.id ? " locaryn-segment-on" : ""}`}
            onClick={() => {
              // Le code glisse dans le sens du geste : choisir un onglet à
              // droite le fait entrer par la droite. Sans cela, l'image change
              // sans qu'on sache si l'on a avancé ou reculé.
              const de = MODES.findIndex((x) => x.id === precedent.current);
              const vers = MODES.findIndex((x) => x.id === m.id);
              setSens(vers >= de ? "droite" : "gauche");
              precedent.current = m.id;
              setMode(m.id);
            }}
          >
            {m.label}
          </button>
        ))}
      </div>

      {choisi && <p className="locaryn-field-hint">{choisi.explication}</p>}

      {mode === "public" && (
        <div className="locaryn-srv-row">
          <input
            className="locaryn-input"
            placeholder="mondomaine.fr:7474 — ou l'adresse fixe de la box"
            value={adresse}
            onChange={(e) => setAdresse(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void charger("public", adresse)}
          />
          <button
            type="button"
            className="locaryn-btn-ghost"
            disabled={busy || !adresse.trim()}
            onClick={() => void charger("public", adresse)}
          >
            Produire le code
          </button>
        </div>
      )}

      {busy && <p className="locaryn-field-hint">…</p>}

      {code?.qr_svg && (
        <div
          key={`${mode}-${code.url}`}
          className={`locaryn-travel-code locaryn-qr-entre locaryn-qr-${sens}`}
        >
          <div
            className="locaryn-travel-qr"
            // biome-ignore lint/security/noDangerouslySetInnerHtml: le SVG est dessiné par notre propre démon, sur cette machine, à partir d'une chaîne qu'il vient de composer — il ne traverse jamais le réseau et ne contient pas de script.
            dangerouslySetInnerHTML={{ __html: code.qr_svg }}
          />
          <div className="locaryn-travel-say">
            <p className="locaryn-travel-title">Scannez avec le téléphone</p>
            <p className="locaryn-travel-sub">
              Le téléphone enregistrera <strong>{code.url}</strong> et le certificat de cette
              machine. Rien à taper.
            </p>
          </div>
        </div>
      )}

      {error && <div className="locaryn-vp-error">{error}</div>}
    </div>
  );
}
