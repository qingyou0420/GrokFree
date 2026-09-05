import { useEffect, useRef, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { api } from "../lib/api";
import { applyStream } from "../lib/acp-parse";
import { applyTheme } from "../lib/theme";
import type {
  DesktopPrefs,
  DesktopState,
  LiveSession,
  PermissionReq,
} from "../lib/types";
import { useSessionStore } from "../state";

type NotifyOs = (title: string, body: string) => void | Promise<void>;
type Flash = (
  text: string,
  kind?: "info" | "success" | "error",
  sessionId?: string | null
) => void;
type SetPermission = (p: PermissionReq | null) => void;

function uid(p: string) {
  return `${p}_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`;
}

/**
 * 六个 api.on 事件的注册/清理 + 状态迁移 toast（任务完成 / 出错）。
 * 自 App.tsx 原样抽取；agent://stream 依赖 applyStream 的
 * 「无变化返回同一引用」跳过 map 更新（见 acp-parse.test.ts）。
 */
export function useAgentEvents(opts: {
  setState: Dispatch<SetStateAction<DesktopState | null>>;
  setLive: (
    v: LiveSession[] | ((prev: LiveSession[]) => LiveSession[])
  ) => void;
  setPermission: SetPermission;
  /** 最新 prefs（只读 showRawAcpEvents），避免订阅重建 */
  prefsRef: MutableRefObject<DesktopPrefs>;
  /** 会话聚焦入口（app://focus-session / tray） */
  focusRef: MutableRefObject<(id: string) => void>;
  notifyOs: NotifyOs;
  flash: Flash;
}) {
  const { setState, setLive, setPermission, prefsRef, focusRef, notifyOs, flash } =
    opts;
  const setTranscripts = useSessionStore((s) => s.setTranscripts);

  useEffect(() => {
    // 异步注册：cleanup 可能先于 await 完成（StrictMode / 快速卸载），
    // 记录 promise 并在 cleanup 里补齐 unlisten，避免监听器泄漏
    const unsubs: Array<() => void> = [];
    let disposed = false;
    (async () => {
      unsubs.push(
        await api.on<DesktopState>("app://state-reloaded", (st) => {
          setState(st);
          useSessionStore
            .getState()
            .setActiveProjectId((cur) => cur ?? st.projects[0]?.id ?? null);
          applyTheme(st.prefs?.theme || "light");
        })
      );
      if (disposed) {
        unsubs.splice(0).forEach((u) => u());
        return;
      }
      unsubs.push(
        // 后端把流式通知按 ~40ms 窗口合批（agent://streamBatch）：
        // 逐 token 逐条 emit 会打满 Windows 主线程事件泵（窗口拖不动）。
        await api.on<{
          sessionId: string;
          events: Array<{ method: string; params: Record<string, unknown> }>;
        }>("agent://streamBatch", (p) => {
          if (!p.events?.length) return;
          setTranscripts((prev) => {
            const cur = prev[p.sessionId] ?? [];
            const showRaw = prefsRef.current.showRawAcpEvents === true;
            let next = cur;
            for (const ev of p.events) {
              next = applyStream(next, ev.method, ev.params || {}, {
                showRawAcpEvents: showRaw,
              });
            }
            // 静默事件 / 无变化：保持原引用，跳过整个 map 更新与重渲染
            if (next === cur) return prev;
            return { ...prev, [p.sessionId]: next };
          });
        })
      );
      if (disposed) {
        unsubs.splice(0).forEach((u) => u());
        return;
      }
      unsubs.push(
        await api.on<string>("app://focus-session", (sessionId) => {
          if (sessionId) void focusRef.current(sessionId);
        })
      );
      unsubs.push(
        await api.on<LiveSession | { id: string; status: string; error?: string }>(
          "agent://state",
          (p) => {
            // 休眠 = 进程已回收：连带清掉排队消息与静默提示
            if ((p as LiveSession).status === "hibernated") {
              const id = (p as LiveSession).id;
              if (id) {
                const store = useSessionStore.getState();
                store.clearStall(id);
                const dropped = store.clearSendQueue(id);
                if (dropped.length > 0) {
                  flash(`会话已休眠，丢弃 ${dropped.length} 条排队消息`, "info", id);
                }
              }
            }
            setLive((prev) => {
              const id = (p as LiveSession).id;
              if (!id) return prev;
              // 休眠 = 进程已回收：从活跃列表移除（切项目自动回收也走这里），
              // 否则带 cwd 的 hibernated 事件会把幽灵会话塞回列表
              if ((p as LiveSession).status === "hibernated") {
                return prev.filter((s) => s.id !== id);
              }
              const idx = prev.findIndex((s) => s.id === id);
              if (idx >= 0) {
                const next = [...prev];
                next[idx] = { ...next[idx], ...p } as LiveSession;
                return next;
              }
              if ((p as LiveSession).cwd) return [p as LiveSession, ...prev];
              return prev;
            });
          }
        )
      );
      unsubs.push(
        await api.on<PermissionReq>("agent://permission", (p) => {
          setPermission(p);
          void notifyOs("GrokFree", "小精灵请求权限批准");
          flash("小精灵等待权限批准 — 点击跳转", "info", p.sessionId);
        })
      );
      unsubs.push(
        // 「本会话内允许」缓存命中：后端已自动批准，仅提示不打断
        await api.on<{ sessionId: string; scope: string }>(
          "agent://permissionAuto",
          (p) => {
            flash(`已按「本会话内允许」自动批准（${p.scope}）`, "info", p.sessionId);
          }
        )
      );
      unsubs.push(
        await api.on<{
          sessionId: string;
          ok?: boolean;
          text?: string;
          error?: string;
          attempt?: number;
        }>("agent://autoContinue", (p) => {
          if (p.ok && p.text) {
            setTranscripts((prev) => ({
              ...prev,
              [p.sessionId]: [
                ...(prev[p.sessionId] ?? []),
                { kind: "system", id: uid("sys"), text: "进程中断，已自动恢复会话" },
                { kind: "user", id: uid("u"), text: p.text as string },
              ],
            }));
            flash(
              `已自动恢复并继续${p.attempt ? `（第 ${p.attempt} 次）` : ""}`,
              "info",
              p.sessionId
            );
          } else {
            flash(
              `自动恢复失败${p.error ? `：${p.error}` : ""}，请手动恢复后发送「继续」`,
              "error",
              p.sessionId
            );
          }
        })
      );
      unsubs.push(
        // 静默看门狗：只提示「继续等待 / 结束本轮」，绝不自动取消
        await api.on<{ sessionId: string; title: string; silentSecs: number }>(
          "agent://stall",
          (p) => {
            useSessionStore.getState().setStall(p.sessionId, {
              title: p.title,
              silentSecs: p.silentSecs,
            });
            flash(
              `「${p.title}」已静默 ${Math.round(p.silentSecs / 60)} 分钟 — 点击查看`,
              "info",
              p.sessionId
            );
          }
        )
      );
      if (disposed) {
        unsubs.splice(0).forEach((u) => u());
        return;
      }
      unsubs.push(
        await api.on<{
          sessionId: string;
          id: unknown;
          method: string;
          params: unknown;
        }>("agent://serverRequest", (p) => {
          api
            .handleServerRequest(p.sessionId, p.id, p.method, p.params)
            .catch((e) => console.error(e));
        })
      );
      if (disposed) {
        unsubs.splice(0).forEach((u) => u());
      }
    })().catch((e) => console.error("事件订阅失败", e));
    return () => {
      disposed = true;
      unsubs.splice(0).forEach((u) => u());
    };
  }, [setState, setLive, setPermission, setTranscripts, prefsRef, focusRef, notifyOs, flash]);

  // Task complete toast when status leaves running + 排队消息接力发送
  const live = useSessionStore((s) => s.live);
  const prevStatus = useRef<Record<string, string>>({});
  useEffect(() => {
    for (const s of live) {
      const prev = prevStatus.current[s.id];
      if (prev === "running" && s.status === "idle") {
        flash(`任务完成：${s.title} — 点击跳转`, "success", s.id);
        void notifyOs("任务完成", s.title);
      }
      // 轮次结束（idle）：清静默提示，接力发送下一条排队消息
      if (
        (prev === "running" || prev === "waiting_permission") &&
        s.status === "idle"
      ) {
        const store = useSessionStore.getState();
        store.clearStall(s.id);
        const next = store.takeNextSend(s.id);
        if (next != null) {
          // 发送时才补用户块，保证 transcript 顺序（排队期间上一轮还在追加）
          store.setTranscripts((prevT) => ({
            ...prevT,
            [s.id]: [
              ...(prevT[s.id] ?? []),
              { kind: "user", id: uid("u"), text: next },
            ],
          }));
          const remain = (store.sendQueue[s.id] ?? []).length;
          flash(
            remain > 0
              ? `发送排队消息（还剩 ${remain} 条）`
              : "发送排队消息",
            "info",
            s.id
          );
          void api.sendPrompt(s.id, next).catch((e) => {
            flash(`排队消息发送失败：${e}`, "error", s.id);
          });
        }
      }
      if (s.status === "error" && prev !== "error") {
        flash(s.error || `会话出错：${s.title}`, "error", s.id);
        // 会话出错：丢弃排队消息并告知（不要静默吞掉）
        const store = useSessionStore.getState();
        store.clearStall(s.id);
        const dropped = store.clearSendQueue(s.id);
        if (dropped.length > 0) {
          flash(`会话出错，已丢弃 ${dropped.length} 条排队消息`, "error", s.id);
        }
      }
      prevStatus.current[s.id] = s.status;
    }
  }, [live, flash, notifyOs]);
}
