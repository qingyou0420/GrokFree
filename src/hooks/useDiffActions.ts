import { useCallback, useMemo, useState } from "react";
import { api } from "../lib/api";
import type { ChatBlock } from "../lib/types";
import { useSessionStore } from "../state";

type Flash = (
  text: string,
  kind?: "info" | "success" | "error",
  sessionId?: string | null
) => void;
type RefreshGit = (dir?: string, force?: boolean) => void | Promise<void>;

/**
 * diff 审查操作：单条接受/忽略、批量、复制补丁 + 决策记录。
 * 吃 blocks（而非组装好的 diffs）：diffs 依赖本 hook 的 diffDecisions，避免循环。
 */
export function useDiffActions(opts: {
  cwd: string;
  blocks: ChatBlock[];
  refreshGit: RefreshGit;
  flash: Flash;
}) {
  const { cwd, blocks, refreshGit, flash } = opts;
  const activeSessionId = useSessionStore((s) => s.activeSessionId);
  const [diffDecisions, setDiffDecisions] = useState<
    Record<string, Record<string, "accepted" | "rejected">>
  >({});
  const [diffBusyPath, setDiffBusyPath] = useState<string | null>(null);
  const [batchBusy, setBatchBusy] = useState(false);

  /** 当前会话中仍为 pending 的 diff 块 */
  const pendingDiffs = useMemo(() => {
    const decisions = activeSessionId
      ? diffDecisions[activeSessionId] ?? {}
      : {};
    return blocks.filter(
      (b): b is Extract<ChatBlock, { kind: "diff" }> =>
        b.kind === "diff" && !(b.path in decisions)
    );
  }, [blocks, activeSessionId, diffDecisions]);

  /** 删除会话时清理该会话的决策记录 */
  const forgetSession = useCallback((id: string) => {
    setDiffDecisions((prev) => {
      if (!(id in prev)) return prev;
      const next = { ...prev };
      delete next[id];
      return next;
    });
  }, []);

  const acceptDiff = useCallback(
    async (path: string, patch: string) => {
      if (!cwd || !activeSessionId) {
        flash("无工作目录，无法应用 diff", "error");
        return;
      }
      setDiffBusyPath(path);
      try {
        const r = await api.applyDiff(cwd, path, patch);
        setDiffDecisions((prev) => ({
          ...prev,
          [activeSessionId]: {
            ...(prev[activeSessionId] ?? {}),
            [path]: "accepted",
          },
        }));
        flash(r.message || `已接受：${path}`, "success");
        void refreshGit(cwd, true);
      } catch (e) {
        flash(`接受失败：${e}`, "error");
      } finally {
        setDiffBusyPath(null);
      }
    },
    [cwd, activeSessionId, flash, refreshGit]
  );

  const rejectDiff = useCallback(
    async (path: string) => {
      if (!activeSessionId) return;
      try {
        await api.rejectDiff(path);
        setDiffDecisions((prev) => ({
          ...prev,
          [activeSessionId]: {
            ...(prev[activeSessionId] ?? {}),
            [path]: "rejected",
          },
        }));
        flash(`已忽略：${path}`, "info");
      } catch (e) {
        flash(`操作失败：${e}`, "error");
      }
    },
    [activeSessionId, flash]
  );

  const acceptAllPending = useCallback(async () => {
    if (!cwd || !activeSessionId) return;
    const pending = pendingDiffs;
    if (!pending.length) return;
    setBatchBusy(true);
    let ok = 0;
    let fail = 0;
    for (const d of pending) {
      try {
        await api.applyDiff(cwd, d.path, d.patch);
        setDiffDecisions((prev) => ({
          ...prev,
          [activeSessionId]: {
            ...(prev[activeSessionId] ?? {}),
            [d.path]: "accepted",
          },
        }));
        ok++;
      } catch {
        fail++;
      }
    }
    setBatchBusy(false);
    void refreshGit(cwd, true);
    flash(
      `批量接受：${ok} 成功${fail ? `，${fail} 失败` : ""}`,
      fail ? "error" : "success"
    );
  }, [cwd, activeSessionId, pendingDiffs, flash, refreshGit]);

  const rejectAllPending = useCallback(async () => {
    if (!activeSessionId) return;
    const pending = pendingDiffs;
    for (const d of pending) {
      try {
        await api.rejectDiff(d.path);
        setDiffDecisions((prev) => ({
          ...prev,
          [activeSessionId]: {
            ...(prev[activeSessionId] ?? {}),
            [d.path]: "rejected",
          },
        }));
      } catch {
        /* continue */
      }
    }
    flash(`已忽略 ${pending.length} 个变更`, "info");
  }, [activeSessionId, pendingDiffs, flash]);

  const copyPatch = useCallback(
    async (path: string, patch: string) => {
      try {
        await navigator.clipboard.writeText(patch);
        flash(`已复制补丁：${path}`, "success");
      } catch {
        flash("复制失败", "error");
      }
    },
    [flash]
  );

  return {
    diffDecisions,
    diffBusyPath,
    batchBusy,
    forgetSession,
    acceptDiff,
    rejectDiff,
    acceptAllPending,
    rejectAllPending,
    copyPatch,
  };
}
