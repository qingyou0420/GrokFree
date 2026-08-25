import { useCallback, useEffect } from "react";
import { api, errorText } from "../lib/api";
import type { UpdateProgress } from "../lib/types";
import { useUpdateStore } from "../state";
import type { ConfirmState } from "../state";

type Flash = (
  text: string,
  kind?: "info" | "success" | "error",
  sessionId?: string | null
) => void;

const CHECK_INTERVAL_MS = 30 * 60 * 1000;

/**
 * GitHub Releases 云端更新：启动时检查 + 30 分钟轮询 + 一键下载安装。
 */
export function useCloudUpdate(opts: {
  askConfirm: (c: ConfirmState) => void;
  flash: Flash;
}) {
  const { askConfirm, flash } = opts;
  const cloudUpdate = useUpdateStore((s) => s.cloudUpdate);
  const setCloudUpdate = useUpdateStore((s) => s.setCloudUpdate);
  const updateBusy = useUpdateStore((s) => s.updateBusy);
  const setUpdateBusy = useUpdateStore((s) => s.setUpdateBusy);
  const updateProgress = useUpdateStore((s) => s.updateProgress);
  const setUpdateProgress = useUpdateStore((s) => s.setUpdateProgress);

  const refreshCloudUpdate = useCallback(async () => {
    try {
      const info = await api.checkCloudUpdate();
      setCloudUpdate(info);
    } catch {
      setCloudUpdate(null);
    }
  }, [setCloudUpdate]);

  useEffect(() => {
    void refreshCloudUpdate();
    const t = window.setInterval(() => void refreshCloudUpdate(), CHECK_INTERVAL_MS);
    return () => window.clearInterval(t);
  }, [refreshCloudUpdate]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void api.on<UpdateProgress>("app://update-progress", (p) => {
      setUpdateProgress(p);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [setUpdateProgress]);

  const launchCloudUpdate = useCallback(async () => {
    if (updateBusy) return;
    if (!cloudUpdate?.isNewer) {
      flash("当前已是最新版本", "info");
      return;
    }
    const label = `${cloudUpdate.fileName} (v${cloudUpdate.version})`;
    askConfirm({
      title: "一键云端更新",
      message: `将从 GitHub 下载并安装 GrokFree v${cloudUpdate.version}：\n\n${label}\n\n若安装目录在 Program Files，系统可能弹出 UAC，请点「是」。\n安装过程中本程序可能会被关闭。是否继续？`,
      confirmLabel: "下载并安装",
      onConfirm: async () => {
        setUpdateBusy(true);
        setUpdateProgress({ phase: "check", received: 0, total: 0, percent: 0 });
        try {
          const info = await api.launchCloudUpdate();
          flash(
            `已启动安装程序 v${info.version} — 请按安装向导完成更新`,
            "success"
          );
        } catch (e) {
          flash(`更新失败：${errorText(e)}`, "error");
        } finally {
          setUpdateBusy(false);
          setUpdateProgress(null);
        }
      },
    });
  }, [
    updateBusy,
    cloudUpdate,
    flash,
    askConfirm,
    setUpdateBusy,
    setUpdateProgress,
  ]);

  return {
    cloudUpdate,
    updateBusy,
    updateProgress,
    refreshCloudUpdate,
    launchCloudUpdate,
  };
}
