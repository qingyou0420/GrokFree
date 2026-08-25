import { useCallback, type Dispatch, type SetStateAction } from "react";
import { api, errorText } from "../lib/api";
import { applyTheme } from "../lib/theme";
import type {
  DesktopPrefs,
  DesktopState,
  GrokEnvironment,
} from "../lib/types";
import { useUiStore } from "../state";

type Flash = (
  text: string,
  kind?: "info" | "success" | "error",
  sessionId?: string | null
) => void;

/**
 * SettingsModal 的稳定回调族（避免弹窗重渲染关掉原生 <select>）。
 */
export function useSettingsHandlers(opts: {
  setState: Dispatch<SetStateAction<DesktopState | null>>;
  setEnv: Dispatch<SetStateAction<GrokEnvironment | null>>;
  refreshCloudUpdate: () => Promise<void>;
  launchCloudUpdate: () => Promise<void>;
  flash: Flash;
}) {
  const { setState, setEnv, refreshCloudUpdate, launchCloudUpdate, flash } =
    opts;
  const setShowSettings = useUiStore((s) => s.setShowSettings);

  const onClose = useCallback(() => {
    setShowSettings(false);
  }, [setShowSettings]);

  const onRefreshCloudUpdate = useCallback(() => {
    void refreshCloudUpdate();
  }, [refreshCloudUpdate]);

  const onExportDiagnostics = useCallback(async () => {
    try {
      const dir = await api.exportDiagnostics();
      flash(`诊断包已导出：${dir}`, "success");
    } catch (e) {
      flash(`导出失败：${e}`, "error");
    }
  }, [flash]);

  const onFlash = useCallback(
    (text: string, kind?: "info" | "error" | "success") => {
      flash(text, kind ?? "info");
    },
    [flash]
  );

  const onSave = useCallback(
    async (p: DesktopPrefs) => {
      try {
        const st = await api.updatePrefs({
          ...p,
          sandboxMode: p.sandboxMode || "off",
          fsScope:
            p.fsScope === "unrestricted" ? "unrestricted" : "workspace",
          historyInitialVisible: (() => {
            const n = Number(p.historyInitialVisible);
            return n === 30 || n === 50 || n === 100 ? n : 50;
          })(),
          chatMaskQuiet: p.chatMaskQuiet === true,
        });
        setState(st);
        applyTheme(p.theme || "light");
        flash("设置已保存", "success");
        try {
          const e = await api.probeEnvironment();
          setEnv(e);
        } catch (e) {
          flash(`环境探测失败：${errorText(e)}`, "error");
        }
      } catch (e) {
        flash(`保存设置失败：${errorText(e)}`, "error");
      }
    },
    [setState, flash, setEnv]
  );

  const onOpenConfig = useCallback(() => {
    void api.openConfigFile();
  }, []);

  const onRevealLogs = useCallback(() => {
    void api.revealLogs();
  }, []);

  const onLaunchCloudUpdate = useCallback(() => {
    void launchCloudUpdate();
  }, [launchCloudUpdate]);

  return {
    onClose,
    onRefreshCloudUpdate,
    onExportDiagnostics,
    onFlash,
    onSave,
    onOpenConfig,
    onRevealLogs,
    onLaunchCloudUpdate,
  };
}
