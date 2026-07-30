import { FormEvent, useEffect, useState } from "react";
import { api } from "../api";
import { copySecret } from "../clipboard";
import { formatError } from "../errors";
import appMark from "../assets/app-mark.png";

type Mode = "unlock" | "setup" | "recovery" | "show-recovery";

interface Props {
  initialized: boolean;
  pendingRecoveryKey: string | null;
  onSetupComplete: (recoveryKey: string) => void;
  onRecoveryConfirmed: () => void;
  onUnlocked: () => void;
}

export function UnlockScreen({
  initialized,
  pendingRecoveryKey,
  onSetupComplete,
  onRecoveryConfirmed,
  onUnlocked,
}: Props) {
  const [mode, setMode] = useState<Mode>(() => {
    if (pendingRecoveryKey) return "show-recovery";
    return initialized ? "unlock" : "setup";
  });
  const [password, setPassword] = useState("");
  const [password2, setPassword2] = useState("");
  const [recoveryInput, setRecoveryInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const [confirmedSave, setConfirmedSave] = useState(false);

  useEffect(() => {
    if (pendingRecoveryKey) setMode("show-recovery");
  }, [pendingRecoveryKey]);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      if (mode === "setup") {
        if (password.length < 8) {
          throw new Error("Password must be at least 8 characters.");
        }
        if (password !== password2) {
          throw new Error("Passwords do not match.");
        }
        const key = await api.setup(password);
        if (!key || !key.trim()) {
          throw new Error("Setup succeeded but no recovery key was returned.");
        }
        setPassword("");
        setPassword2("");
        setMode("show-recovery");
        onSetupComplete(key);
        return;
      }
      if (mode === "unlock") {
        await api.unlock(password);
        setPassword("");
        onUnlocked();
        return;
      }
      if (mode === "recovery") {
        await api.unlockRecovery(recoveryInput.trim());
        setRecoveryInput("");
        onUnlocked();
      }
    } catch (err) {
      setError(formatError(err));
    } finally {
      setBusy(false);
    }
  }

  async function copyRecovery() {
    if (!pendingRecoveryKey) return;
    try {
      const res = await copySecret(pendingRecoveryKey);
      if (!res.ok) throw new Error(res.error ?? "copy failed");
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      setError(formatError(err));
    }
  }

  function finishRecovery() {
    if (!confirmedSave) {
      setError("Confirm that you saved the recovery key before continuing.");
      return;
    }
    onRecoveryConfirmed();
  }

  if (mode === "show-recovery") {
    return (
      <div className="gate">
        <div className="gate-card">
          <div className="logo-mark" aria-hidden>
            <img src={appMark} alt="" width={44} height={44} draggable={false} />
          </div>
          <h1>Save your recovery key</h1>
          <p className="muted">
            This is the <strong>only</strong> time SecretFolder will show this key. Store it
            offline — it unlocks the vault if you forget the master password. Anyone with this
            key can open your vault.
          </p>
          <pre className="recovery" role="status">
            {pendingRecoveryKey || "..."}
          </pre>
          <div className="gate-actions">
            <button type="button" className="primary" onClick={() => void copyRecovery()}>
              {copied ? "Copied" : "Copy recovery key"}
            </button>
          </div>
          <label className="check-row">
            <input
              type="checkbox"
              checked={confirmedSave}
              onChange={(e) => {
                setConfirmedSave(e.target.checked);
                setError(null);
              }}
            />
            <span>I saved this recovery key somewhere safe</span>
          </label>
          {error && <p className="error">{error}</p>}
          <button
            type="button"
            className="primary"
            disabled={!confirmedSave || !pendingRecoveryKey}
            onClick={finishRecovery}
            style={{ width: "100%", marginTop: "0.35rem" }}
          >
            Continue to vault
          </button>
          <p className="fineprint">
            Protects data at rest on disk. Does not protect a fully compromised PC while unlocked.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="gate">
      <div className="gate-card">
        <div className="logo-mark" aria-hidden>
          <img src={appMark} alt="" width={44} height={44} draggable={false} />
        </div>
        <h1>
          {mode === "setup"
            ? "Create your vault"
            : mode === "recovery"
              ? "Recovery unlock"
              : "Unlock SecretFolder"}
        </h1>
        <p className="muted">
          {mode === "setup"
            ? "Choose a strong master password. Files are encrypted on disk."
            : mode === "recovery"
              ? "Unlock with your recovery key."
              : "Enter your master password to open your encrypted vault."}
        </p>
        <form onSubmit={(e) => void onSubmit(e)} className="gate-form">
          {mode !== "recovery" ? (
            <>
              <label>
                Master password
                <input
                  type="password"
                  autoFocus
                  autoComplete={mode === "setup" ? "new-password" : "current-password"}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  disabled={busy}
                />
              </label>
              {mode === "setup" && (
                <label>
                  Confirm password
                  <input
                    type="password"
                    autoComplete="new-password"
                    value={password2}
                    onChange={(e) => setPassword2(e.target.value)}
                    disabled={busy}
                  />
                </label>
              )}
            </>
          ) : (
            <label>
              Recovery key
              <input
                type="text"
                autoFocus
                autoComplete="off"
                spellCheck={false}
                value={recoveryInput}
                onChange={(e) => setRecoveryInput(e.target.value)}
                disabled={busy}
              />
            </label>
          )}
          {error && <p className="error">{error}</p>}
          <button type="submit" className="primary" disabled={busy}>
            {busy
              ? "Working…"
              : mode === "setup"
                ? "Create vault"
                : mode === "recovery"
                  ? "Unlock with recovery key"
                  : "Unlock"}
          </button>
        </form>
        {initialized && mode !== "setup" && (
          <div className="gate-actions">
            {mode === "unlock" ? (
              <button
                type="button"
                className="ghost"
                onClick={() => {
                  setMode("recovery");
                  setError(null);
                }}
              >
                Use recovery key
              </button>
            ) : (
              <button
                type="button"
                className="ghost"
                onClick={() => {
                  setMode("unlock");
                  setError(null);
                }}
              >
                Use master password
              </button>
            )}
          </div>
        )}
        <p className="fineprint">Local-only encrypted vault. No cloud sync. Signed app updates only.</p>
      </div>
    </div>
  );
}
