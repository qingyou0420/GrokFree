import { useCallback, useRef, useState, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { api, errorText } from "../lib/api";
import { isPlaceholderTitle } from "../lib/acp-parse";
import type {
  ChatBlock,
  DesktopPrefs,
  DesktopState,
  DiskSession,
  GrokEnvironment,
  Project,
  SessionMeta,
} from "../lib/types";
import { useSessionStore, useUiStore, type ConfirmState } from "../state";

/** 默认首屏历史条数（可被 prefs.historyInitialVisible 覆盖） */
export const HISTORY_INITIAL_DEFAULT = 50;
/** 「加载更早」增加的条数（HistoryLoadEarlier 用） */
export const HISTORY_LOAD_STEP = 80;
/** 「全部展开」的单次渲染上限（无虚拟列表，防止超长会话卡死） */
export const HISTORY_EXPAND_CAP = 500;

function uid(p: string) {
  return `${p}_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`;
}

function normalizeHistoryInitial(prefs: DesktopPrefs): number {
  const n = Number(prefs.historyInitialVisible);
  return n === 30 || n === 50 || n === 100 ? n : HISTORY_INITIAL_DEFAULT;
}

type Flash = (
  text: string,
  kind?: "info" | "success" | "error",
  sessionId?: string | null
) => void;
type RevealChat = (sessionId: string | null) => void;

/**
 * 会话 CRUD：新建 / 恢复装填 / 休眠 / 删除 / 重命名 / 占位清理 / 磁盘历史。
 * 自 App.tsx 原样抽取。改名走侧栏内联（Sidebar）或 InputDialog（顶栏菜单），
 * 不再用 window.prompt（Tauri WebView 支持不可靠）。
 */
export function useSessionActions(opts: {
  env: GrokEnvironment | null;
  projects: Project[];
  sessions: SessionMeta[];
  activeProject: Project | null;
  setState: Dispatch<SetStateAction<DesktopState | null>>;
  prefsRef: MutableRefObject<DesktopPrefs>;
  setBusy: Dispatch<SetStateAction<boolean>>;
  /** 遮罩揭开（loadTranscript / createSession 之后） */
  revealChatAfterPaint: RevealChat;
  /** 新会话强制贴底（createSession 之前） */
  beginAtBottom: () => void;
  askConfirm: (c: ConfirmState) => void;
  flash: Flash;
  /** 会话删除后的级联清理（diff 决策 / plan 关闭标记） */
  onSessionDeleted: (id: string) => void;
  onSessionCreated: () => void;
}) {
  const {
    env,
    projects,
    sessions,
    activeProject,
    setState,
    prefsRef,
    setBusy,
    revealChatAfterPaint,
    beginAtBottom,
    askConfirm,
    flash,
    onSessionDeleted,
    onSessionCreated,
  } = opts;

  const setLive = useSessionStore((s) => s.setLive);
  const setActiveProjectId = useSessionStore((s) => s.setActiveProjectId);
  const activeSessionId = useSessionStore((s) => s.activeSessionId);
  const setActiveSessionId = useSessionStore((s) => s.setActiveSessionId);
  const setTranscripts = useSessionStore((s) => s.setTranscripts);
  const setShowDashboard = useUiStore((s) => s.setShowDashboard);
  const setShowDiskHistory = useUiStore((s) => s.setShowDiskHistory);
  const setHistoryTail = useUiStore((s) => s.setHistoryTail);
  const live = useSessionStore((s) => s.live);

  const [diskSessions, setDiskSessions] = useState<DiskSession[]>([]);
  const [diskFilterProject, setDiskFilterProject] = useState(true);
  const [diskQuery, setDiskQuery] = useState("");
  /** 顶栏菜单「重命名会话」的 InputDialog 状态 */
  const [renameDialog, setRenameDialog] = useState<{
    id: string;
    title: string;
  } | null>(null);
  const [renameBusy, setRenameBusy] = useState(false);

  const addProject = useCallback(async () => {
    try {
      const startDir =
        prefsRef.current.defaultProjectsDir?.trim() ||
        (await api.getDefaultProjectsDir().catch(() => "D:\\Grok Build"));
      const cwdPick = await api.pickDirectory(startDir);
      if (!cwdPick) return;
      const st = await api.addProject(cwdPick);
      setState(st);
      const p = st.projects.find((x) => x.cwd === cwdPick);
      if (p) {
        setActiveProjectId(p.id);
      }
    } catch (e) {
      flash(`添加项目失败：${errorText(e)}`, "error");
    }
  }, [prefsRef, setState, setActiveProjectId, flash]);

  /** 同项目并发新建守卫：双击/连点不再各起一个 grok agent（后端也有守卫，
   * 这里提前拦截避免多余的错误 toast） */
  const createInFlightRef = useRef<Set<string>>(new Set());

  const createSession = useCallback(
    async (project?: Project | null, agentId?: string | null) => {
      const proj = project ?? activeProject;
      if (!proj) {
        flash("请先选择或添加一个项目", "error");
        return;
      }
      if (createInFlightRef.current.has(proj.id)) {
        flash("会话正在启动，请稍候…", "info");
        return;
      }
      const useAgentId = agentId ?? "grok";
      if (useAgentId === "grok") {
        if (env && !env.grokExists) {
          flash("未检测到 Grok CLI，请先安装或在设置中指定路径", "error");
          return;
        }
        if (env && env.grokExists && env.cliVersionOk === false) {
          flash(
            `CLI 版本过旧（${env.grokVersion}，需要 ≥ ${env.minCliVersion}），请升级`,
            "error"
          );
          return;
        }
      }
      createInFlightRef.current.add(proj.id);
      setBusy(true);
      setShowDashboard(false);
      beginAtBottom();
      try {
        const session = await api.createSession(
          proj.id,
          proj.cwd,
          proj.name,
          useAgentId
        );
        setLive((prev) => [session, ...prev.filter((s) => s.id !== session.id)]);
        setActiveSessionId(session.id);
        setActiveProjectId(proj.id);
        setTranscripts((prev) => ({
          ...prev,
          [session.id]: [
            {
              kind: "system",
              id: uid("sys"),
              text: `会话已启动 · 工作目录：${proj.cwd}`,
            },
          ],
        }));
        revealChatAfterPaint(session.id);
        flash(`小精灵已启动：${session.title}`, "success", session.id);
        onSessionCreated();
        const st = await api.getAppState();
        setState(st);
      } catch (e) {
        const msg = String(e);
        console.error("createSession failed", e);
        flash(`启动失败：${msg}`, "error");
      } finally {
        createInFlightRef.current.delete(proj.id);
        setBusy(false);
      }
    },
    [
      activeProject,
      env,
      setBusy,
      setShowDashboard,
      beginAtBottom,
      setLive,
      setActiveSessionId,
      setActiveProjectId,
      setTranscripts,
      revealChatAfterPaint,
      flash,
      onSessionCreated,
      setState,
    ]
  );

  const loadTranscriptForSession = useCallback(
    async (
      desktopSessionId: string,
      grokSessionId: string,
      pathHint?: string | null,
      banner?: string
    ) => {
      // 自有日志优先（实时事件的落盘快照，格式由我们掌控）；
      // 无日志（旧会话 / 首次磁盘恢复）才退回 CLI chat_history 解析。
      // 日志路径不插 banner 块：banner 会被落盘，反复恢复会越积越多。
      try {
        const journal = await api.loadJournal(desktopSessionId).catch(() => null);
        if (journal && journal.length > 0) {
          setTranscripts((prev) => ({ ...prev, [desktopSessionId]: journal }));
          setHistoryTail((prev) => ({
            ...prev,
            [desktopSessionId]: normalizeHistoryInitial(prefsRef.current),
          }));
          revealChatAfterPaint(desktopSessionId);
          return;
        }
      } catch {
        /* 日志读取失败按无日志处理 */
      }
      try {
        const hist = await api.loadDiskTranscript(grokSessionId, pathHint);
        const emptySys =
          hist.length === 1 && hist[0]?.kind === "system" ? hist[0] : null;
        const onlyEmptyHint =
          !!emptySys &&
          (emptySys.id === "sys_empty_hist" ||
            emptySys.id === "sys_empty" ||
            emptySys.text.includes("没有可展示"));
        const withBanner: ChatBlock[] = banner
          ? [{ kind: "system", id: uid("sys"), text: banner }, ...hist]
          : hist;
        setTranscripts((prev) => ({ ...prev, [desktopSessionId]: withBanner }));
        setHistoryTail((prev) => ({
          ...prev,
          [desktopSessionId]: normalizeHistoryInitial(prefsRef.current),
        }));
        revealChatAfterPaint(desktopSessionId);
        if (onlyEmptyHint) {
          flash(
            "未读到可展示的历史消息，请从「磁盘会话历史」重新选择该会话",
            "info",
            desktopSessionId
          );
        }
      } catch (e) {
        setTranscripts((prev) => ({
          ...prev,
          [desktopSessionId]: [
            {
              kind: "system",
              id: uid("sys"),
              text: `会话已连接，但读取历史失败：${e}。请从「磁盘会话历史」重新选择该会话。`,
            },
          ],
        }));
        setHistoryTail((prev) => ({
          ...prev,
          [desktopSessionId]: HISTORY_INITIAL_DEFAULT,
        }));
        revealChatAfterPaint(desktopSessionId);
      }
    },
    [setTranscripts, setHistoryTail, prefsRef, revealChatAfterPaint, flash]
  );

  const loadDiskHistory = useCallback(
    async (filterByProject = diskFilterProject) => {
      try {
        const cwd =
          filterByProject && activeProject?.cwd ? activeProject.cwd : null;
        const list = await api.listDiskSessions(100, cwd);
        setDiskSessions(list);
        setShowDiskHistory(true);
        if (list.length === 0) {
          flash(
            filterByProject && activeProject
              ? `当前项目下未找到磁盘历史（已隐藏空会话）`
              : "未在 ~/.grok/sessions 找到有效历史会话（空会话已隐藏）",
            "info"
          );
        }
      } catch (e) {
        flash(`读取历史失败：${e}`, "error");
      }
    },
    [diskFilterProject, activeProject, setShowDiskHistory, flash]
  );

  const deleteDiskSession = useCallback(
    async (d: DiskSession) => {
      askConfirm({
        title: "永久删除磁盘会话",
        message: `确定从磁盘永久删除「${d.title}」？\n\n将删除 ~/.grok/sessions 下的会话目录，不可恢复。`,
        danger: true,
        confirmLabel: "永久删除",
        onConfirm: async () => {
          try {
            await api.deleteDiskSession(d.id, d.path);
            setDiskSessions((prev) => prev.filter((x) => x.id !== d.id));
            try {
              const st = await api.removeSessionMeta(d.id);
              setState(st);
            } catch {
              /* meta may not exist */
            }
            flash("已从磁盘删除会话", "success");
          } catch (e) {
            flash(`磁盘删除失败：${e}`, "error");
          }
        },
      });
    },
    [askConfirm, setState, flash]
  );

  const purgePlaceholderSessions = useCallback(async () => {
    const pid = useSessionStore.getState().activeProjectId;
    const candidates = sessions.filter(
      (s) => (!pid || s.projectId === pid) && isPlaceholderTitle(s.title)
    );
    if (candidates.length === 0) {
      flash("当前没有可清理的占位会话", "info");
      return;
    }
    askConfirm({
      title: "清理占位会话",
      message: `将从项目列表移除 ${candidates.length} 个空/占位会话（不删磁盘历史）。继续？`,
      confirmLabel: "清理",
      onConfirm: async () => {
        try {
          const st = await api.purgeStaleSessionMeta(pid);
          setState(st);
          flash(`已清理 ${candidates.length} 个占位会话`, "success");
        } catch (e) {
          flash(`清理失败：${e}`, "error");
        }
      },
    });
  }, [sessions, askConfirm, setState, flash]);

  /** 提交改名（侧栏内联 / InputDialog 共用）。标题已 trim 非空。 */
  const commitSessionRename = useCallback(
    async (id: string, nextTitle: string) => {
      try {
        const st = await api.renameSession(id, nextTitle);
        setState(st);
        setLive((prev) =>
          prev.map((s) => (s.id === id ? { ...s, title: nextTitle } : s))
        );
        flash("已重命名", "success");
      } catch (e) {
        flash(`重命名失败：${e}`, "error");
      }
    },
    [setState, setLive, flash]
  );

  /** 顶栏菜单入口：打开 InputDialog（不再用 window.prompt） */
  const openRenameDialog = useCallback((id: string, title: string) => {
    setRenameDialog({ id, title });
  }, []);

  const submitRenameDialog = useCallback(
    async (value: string) => {
      if (!renameDialog) return;
      const v = value.trim();
      if (!v) return;
      setRenameBusy(true);
      try {
        await commitSessionRename(renameDialog.id, v);
        setRenameDialog(null);
      } finally {
        setRenameBusy(false);
      }
    },
    [renameDialog, commitSessionRename]
  );

  const hibernate = useCallback(
    async (id: string) => {
      try {
        await api.hibernateSession(id);
        setLive((prev) => prev.filter((s) => s.id !== id));
        if (activeSessionId === id) setActiveSessionId(null);
        const st = await api.getAppState();
        setState(st);
        flash("会话已休眠", "info");
      } catch (e) {
        flash(`休眠失败：${e}`, "error");
      }
    },
    [setLive, activeSessionId, setActiveSessionId, setState, flash]
  );

  /** Remove session from the project list. Stops a live process first; does not delete ~/.grok/sessions disk files. */
  const deleteSession = useCallback(
    async (id: string, title?: string) => {
      const label = title?.trim() || "该会话";
      askConfirm({
        title: "删除会话",
        message: `确定删除「${label}」？\n\n将从本项目列表移除；若正在运行会先停止进程。\n不会删除 ~/.grok/sessions 中的磁盘历史。`,
        danger: true,
        confirmLabel: "删除",
        onConfirm: async () => {
          try {
            const isLive = live.some((s) => s.id === id);
            if (isLive) {
              await api.hibernateSession(id);
              setLive((prev) => prev.filter((s) => s.id !== id));
            }
            const st = await api.removeSessionMeta(id);
            setState(st);
            setTranscripts((prev) => {
              if (!(id in prev)) return prev;
              const next = { ...prev };
              delete next[id];
              return next;
            });
            onSessionDeleted(id);
            if (useSessionStore.getState().activeSessionId === id) {
              setActiveSessionId(null);
              setShowDashboard(true);
            }
            flash("会话已删除", "success");
          } catch (e) {
            flash(`删除失败：${e}`, "error");
          }
        },
      });
    },
    [askConfirm, live, setLive, setState, setTranscripts, onSessionDeleted, setActiveSessionId, setShowDashboard, flash]
  );

  const removeProjectById = useCallback(
    (projectId: string, name: string) => {
      askConfirm({
        title: "移除项目",
        message: `从列表移除「${name}」？不会删除磁盘上的文件。`,
        danger: true,
        confirmLabel: "移除",
        onConfirm: async () => {
          const st = await api.removeProject(projectId);
          setState(st);
          setActiveProjectId(st.projects[0]?.id ?? null);
          flash("项目已移除", "info");
        },
      });
    },
    [askConfirm, setState, setActiveProjectId, flash]
  );

  return {
    // 磁盘历史
    diskSessions,
    setDiskSessions,
    diskFilterProject,
    setDiskFilterProject,
    diskQuery,
    setDiskQuery,
    loadDiskHistory,
    deleteDiskSession,
    // 会话操作
    addProject,
    createSession,
    loadTranscriptForSession,
    purgePlaceholderSessions,
    commitSessionRename,
    openRenameDialog,
    renameDialog,
    renameBusy,
    closeRenameDialog: () => setRenameDialog(null),
    submitRenameDialog,
    hibernate,
    deleteSession,
    removeProjectById,
  };
}
