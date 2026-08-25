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
        await api.on<{
          sessionId: string;
          method: string;
          params: Record<string, unknown>;
        }>("agent://stream", (p) => {
          setTranscripts((prev) => {
            const cur = prev[p.sessionId] ?? [];
            const showRaw = prefsRef.current.showRawAcpEvents === true;
            const next = applyStream(cur, p.method, p.params || {}, {
              showRawAcpEvents: showRaw,
            });
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
            setLive((prev) => {
              const id = (p as LiveSession).id;
              if (!id) return prev;
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

  // Task complete toast when status leaves running
  const live = useSessionStore((s) => s.live);
  const prevStatus = useRef<Record<string, string>>({});
  useEffect(() => {
    for (const s of live) {
      const prev = prevStatus.current[s.id];
      if (prev === "running" && s.status === "idle") {
        flash(`任务完成：${s.title} — 点击跳转`, "success", s.id);
        void notifyOs("任务完成", s.title);
      }
      if (s.status === "error" && prev !== "error") {
        flash(s.error || `会话出错：${s.title}`, "error", s.id);
      }
      prevStatus.current[s.id] = s.status;
    }
  }, [live, flash, notifyOs]);
}
