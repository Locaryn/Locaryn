import { useEffect, useState } from "react";
import { getHfToken, setHfToken } from "../lib/core";

/**
 * HuggingFace access token, used when downloading model repositories.
 * Gated repos (kyutai/pocket-tts, Qwen3-TTS, …) answer HTTP 401 without it.
 * The token is sent only to huggingface.co, as an Authorization: Bearer
 * header, during model downloads — never anywhere else.
 */
export function HuggingFaceSettings() {
  const [token, setToken] = useState("");
  const [show, setShow] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    setToken(getHfToken());
  }, []);

  function save() {
    setHfToken(token);
    setSaved(true);
    window.setTimeout(() => setSaved(false), 2000);
  }

  function clear() {
    setToken("");
    setHfToken("");
    setSaved(true);
    window.setTimeout(() => setSaved(false), 2000);
  }

  return (
    <div className="locaryn-field">
      <label className="locaryn-field-label" htmlFor="hf-token">
        Token d'accès HuggingFace
        {saved && (
          <span style={{ marginLeft: 8, color: "var(--accent)", fontSize: "var(--text-xs)" }}>
            enregistré
          </span>
        )}
      </label>
      <p className="locaryn-field-hint">
        Requis pour télécharger les dépôts restreints (<em>gated</em>) comme{" "}
        <code>kyutai/pocket-tts</code> ou <code>Qwen/Qwen3-TTS-12Hz-…</code>, qui refusent les
        téléchargements anonymes. Le token est envoyé uniquement à huggingface.co, dans l'en-tête{" "}
        <code>Authorization</code>, pendant les téléchargements de modèles. Créez-le sur{" "}
        <strong>huggingface.co → Settings → Access Tokens</strong> (type <code>read</code>).
      </p>
      <div style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 12 }}>
        <input
          id="hf-token"
          type={show ? "text" : "password"}
          className="locaryn-input"
          value={token}
          onChange={(e) => setToken(e.target.value)}
          placeholder="hf_xxxxxxxxxxxxxxxxxxxxxxxx"
          autoComplete="off"
          spellCheck={false}
          style={{ flex: 1, maxWidth: 420 }}
        />
        <button
          type="button"
          className="locaryn-btn-ghost"
          onClick={() => setShow((s) => !s)}
          title={show ? "Masquer le token" : "Afficher le token"}
          style={{ whiteSpace: "nowrap" }}
        >
          {show ? "Masquer" : "Afficher"}
        </button>
      </div>
      <div style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 12 }}>
        <button type="button" className="locaryn-btn-primary" onClick={save}>
          Enregistrer
        </button>
        {token && (
          <button type="button" className="locaryn-btn-ghost" onClick={clear}>
            Effacer
          </button>
        )}
      </div>
      <p className="locaryn-field-hint" style={{ marginTop: 12 }}>
        Stocké uniquement sur cette machine (localStorage de l'application) : le token persiste
        entre les ouvertures, n'est écrit dans aucun fichier du projet et ne peut donc jamais être
        poussé sur GitHub. Une réinstallation depuis le dépôt ne le fera pas réapparaître.
      </p>
    </div>
  );
}
