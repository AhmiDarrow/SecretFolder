import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import type { BootInfo } from "./api";
import type { VaultStatus } from "./types";
import { UnlockScreen } from "./screens/UnlockScreen";
import { ExplorerScreen } from "./screens/ExplorerScreen";
import { clearClipboard } from "./clipboard";
import { formatUserError } from "./errors";
import {
  getHostCi,
  hydrateLocale,
  localeReady,
  setHostCi,
  setLocalePersister,
  translate,
  useI18n,
} from "./i18n";
import "./App.css";

type Screen = "loading" | "unlock" | "explorer";

/** Minimum gap between activity pings sent to the backend idle timer. */
const TOUCH_THROTTLE_MS = 5_000;

/**
 * Recovery key is held at App level so UnlockScreen remounts / status refresh
 * cannot drop the one-time key before the user confirms they saved it.
 */
export default function App() {
  const { t } = useI18n();
  const [screen, setScreen] = useState<Screen>("loading");
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [pendingRecoveryKey, setPendingRecoveryKey] = useState<string | null>(
    null,
  );
  const [bootError, setBootError] = useState<string | null>(null);
  const [hostReady, setHostReady] = useState(false);
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
      setBootError(formatUserError(e));
      setScreen("unlock");
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    setLocalePersister((locale) => {
      if (typeof api.setUiLocale === "function") {
        void api.setUiLocale(locale).catch(() => {});
      }
      if (typeof api.setTrayLabels === "function") {
        void api
          .setTrayLabels(
            translate(locale, "tray.show"),
            translate(locale, "tray.lock"),
            translate(locale, "tray.quit"),
          )
          .catch(() => {});
      }
    });
    let cancelled = false;
    void (async () => {
      let info: BootInfo | null = null;
      try {
        if (typeof api.bootInfo === "function") {
          info = await api.bootInfo();
          if (info?.ci) setHostCi(true);
        }
      } catch {
        /* browser tests / non-tauri */
      }
      if (!cancelled) {
        hydrateLocale(info);
        setHostReady(true);
      }
    })();
    return () => {
      cancelled = true;
      setLocalePersister(null);
    };
  }, []);

  useEffect(() => {
    if (!hostReady || !getHostCi() || screen === "loading") return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void (async () => {
        await localeReady;
        if (cancelled || typeof api.ciReport !== "function") return;
        void api.ciReport(document.body.innerText).catch(() => {});
      })();
    }, 250);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [hostReady, screen]);

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
    // Only real user input counts as activity. A periodic timer here would
    // keep the session alive forever and defeat idle auto-lock.
    let lastTouch = 0;
    const onAct = () => {
      const now = Date.now();
      if (now - lastTouch < TOUCH_THROTTLE_MS) return;
      lastTouch = now;
      void api.touch().catch(() => {
        /* locked */
      });
    };
    const evts = ["pointerdown", "keydown", "wheel"] as const;
    evts.forEach((e) => window.addEventListener(e, onAct));
    return () => {
      evts.forEach((e) => window.removeEventListener(e, onAct));
    };
  }, [screen]);

  if (screen === "loading" || !status) {
    return (
      <div className="app-shell center">
        <p className="muted">{t("app.loading")}</p>
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
