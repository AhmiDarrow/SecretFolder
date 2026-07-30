import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import type { VaultStatus } from "./types";
import { UnlockScreen } from "./screens/UnlockScreen";
import { ExplorerScreen } from "./screens/ExplorerScreen";
import { clearClipboard } from "./clipboard";
import { formatError } from "./errors";
import "./App.css";

type Screen = "loading" | "unlock" | "explorer";

/**
 * Recovery key is held at App level so UnlockScreen remounts / status refresh
 * cannot drop the one-time key before the user confirms they saved it.
 */
export default function App() {
  const [screen, setScreen] = useState<Screen>("loading");
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [pendingRecoveryKey, setPendingRecoveryKey] = useState<string | null>(
    null,
  );
  const [bootError, setBootError] = useState<string | null>(null);
  const pendingKeyRef = useRef<string | null>(null);

  const setRecoveryKey = useCallback((key: string | null) => {
    pendingKeyRef.current = key;
    setPendingRecoveryKey(key);
  }, []);

  const refresh = useCallback(async () => {
    try {
      const s = await api.status();
      setStatus(s);
      setBootError(null);
      if (pendingKeyRef.current) {
        setScreen("unlock");
        return;
      }
      setScreen(s.unlocked ? "explorer" : "unlock");
    } catch (e) {
      console.error(e);
      setBootError(formatError(e));
      setScreen("unlock");
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let unsubs: Array<() => void> = [];
    let cancelled = false;
    (async () => {
      const u1 = await listen("vault-locked", () => {
        pendingKeyRef.current = null;
        setPendingRecoveryKey(null);
        void clearClipboard();
        setScreen("unlock");
        void refresh();
      });
      const u2 = await listen("items-changed", () => {
        // Never leave recovery confirmation via a vault event.
        if (pendingKeyRef.current) {
          void api
            .status()
            .then((s) => setStatus(s))
            .catch(() => {});
          setScreen("unlock");
          return;
        }
        void refresh();
      });
      if (cancelled) {
        u1();
        u2();
        return;
      }
      unsubs = [u1, u2];
    })();
    return () => {
      cancelled = true;
      unsubs.forEach((u) => u());
    };
  }, [refresh]);

  useEffect(() => {
    if (screen !== "explorer") return;
    const onAct = () => {
      void api.touch().catch(() => {
        /* locked */
      });
    };
    const evts = ["pointerdown", "keydown", "wheel"] as const;
    evts.forEach((e) => window.addEventListener(e, onAct));
    const iv = window.setInterval(onAct, 30_000);
    return () => {
      evts.forEach((e) => window.removeEventListener(e, onAct));
      window.clearInterval(iv);
    };
  }, [screen]);

  if (screen === "loading" || !status) {
    return (
      <div className="app-shell center">
        <p className="muted">Loading…</p>
        {bootError ? (
          <p className="banner error" role="alert">
            {bootError}
          </p>
        ) : null}
      </div>
    );
  }

  if (screen === "unlock" || pendingRecoveryKey) {
    return (
      <>
        {bootError ? (
          <div className="banner error" role="alert">
            {bootError}
          </div>
        ) : null}
        <UnlockScreen
          initialized={status.initialized}
          pendingRecoveryKey={pendingRecoveryKey}
          onSetupComplete={(key) => {
            setRecoveryKey(key);
            setScreen("unlock");
            void api
              .status()
              .then((s) => setStatus(s))
              .catch(() => {});
          }}
          onRecoveryConfirmed={() => {
            setRecoveryKey(null);
            void refresh();
          }}
          onUnlocked={() => {
            setRecoveryKey(null);
            void refresh();
          }}
        />
      </>
    );
  }

  return (
    <ExplorerScreen
      status={status}
      onLocked={() => {
        setRecoveryKey(null);
        void refresh();
      }}
    />
  );
}
