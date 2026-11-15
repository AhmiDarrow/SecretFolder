import { FormEvent, useEffect, useState } from "react";
import { api } from "../api";
import { copySecret } from "../clipboard";
import { formatUserError } from "../errors";
import { RichText } from "../i18n/RichText";
import { useI18n } from "../i18n";
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
  const { t } = useI18n();
  const [mode, setMode] = useState<Mode>(() => {
    if (pendingRecoveryKey) return "show-recovery";
    return initialized ? "unlock" : "setup";
  });
  const [password, setPassword] = useState("");
  const [password2, setPassword2] = useState("");
  const [recoveryInput, setRecoveryInput] = useState("");
  /** Local copy so setup can show the key before parent re-renders with pendingRecoveryKey. */
  const [localRecoveryKey, setLocalRecoveryKey] = useState<string | null>(
    pendingRecoveryKey,
  );
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const [confirmedSave, setConfirmedSave] = useState(false);

  const displayedRecoveryKey = localRecoveryKey ?? pendingRecoveryKey;

  useEffect(() => {
    if (pendingRecoveryKey) {
      setLocalRecoveryKey(pendingRecoveryKey);
      setMode("show-recovery");
    }
  }, [pendingRecoveryKey]);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      if (mode === "setup") {
        if (password.length < 12) {
          throw new Error(t("unlock.pwShort"));
        }
        if (password !== password2) {
          throw new Error(t("unlock.pwMismatch"));
        }
        const key = await api.setup(password);
        if (!key || !key.trim()) {
          throw new Error(t("unlock.noKey"));
        }
        setPassword("");
        setPassword2("");
        setLocalRecoveryKey(key);
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
      setError(formatUserError(err));
    } finally {
      setBusy(false);
    }
  }

  async function copyRecovery() {
    if (!displayedRecoveryKey) return;
    try {
      const res = await copySecret(displayedRecoveryKey);
      if (!res.ok) throw new Error(res.error ?? "copy failed");
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      setError(formatUserError(err));
    }
  }

  function finishRecovery() {
    if (!confirmedSave) {
      setError(t("unlock.confirmSaveError"));
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
          <h1>{t("recovery.title")}</h1>
          <p className="muted">
            <RichText text={t("recovery.body")} />
          </p>
          <pre className="recovery" role="status">
            {displayedRecoveryKey || "..."}
          </pre>
          <div className="gate-actions">
            <button type="button" className="primary" onClick={() => void copyRecovery()}>
              {copied ? t("recovery.copied") : t("recovery.copy")}
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
            <span>{t("recovery.savedCheck")}</span>
          </label>
          {error && <p className="error">{error}</p>}
          <button
            type="button"
            className="primary"
            disabled={!confirmedSave || !displayedRecoveryKey}
            onClick={finishRecovery}
            style={{ width: "100%", marginTop: "0.35rem" }}
          >
            {t("recovery.continue")}
          </button>
          <p className="fineprint">{t("recovery.fineprint")}</p>
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
            ? t("unlock.titleSetup")
            : mode === "recovery"
              ? t("unlock.titleRecovery")
              : t("unlock.titleUnlock")}
        </h1>
        <p className="muted">
          {mode === "setup"
            ? t("unlock.leadSetup")
            : mode === "recovery"
              ? t("unlock.leadRecovery")
              : t("unlock.leadUnlock")}
        </p>
        <form onSubmit={(e) => void onSubmit(e)} className="gate-form">
          {mode !== "recovery" ? (
            <>
              <label>
                {t("unlock.masterPassword")}
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
                  {t("unlock.confirmPassword")}
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
              {t("unlock.recoveryKey")}
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
              ? t("unlock.working")
              : mode === "setup"
                ? t("unlock.createVault")
                : mode === "recovery"
                  ? t("unlock.unlockRecovery")
                  : t("unlock.unlock")}
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
                {t("unlock.useRecovery")}
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
                {t("unlock.usePassword")}
              </button>
            )}
          </div>
        )}
        <p className="fineprint">{t("unlock.fineprint")}</p>
      </div>
    </div>
  );
}
