import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import {
  ChatBlocks,
  PlanBanner,
  ReviewPane,
  TranscriptToolbar,
} from "./components/ChatBlocks";
import { Composer } from "./components/Composer";
import { Dashboard } from "./components/Dashboard";
import {
  ConfirmDialog,
  InputDialog,
} from "./components/Dialog";
import { DiskHistoryModal } from "./components/DiskHistoryModal";
import { EmptyWorkspace } from "./components/EmptyWorkspace";
import { HistoryLoadEarlier } from "./components/HistoryLoadEarlier";
import { PermissionModal } from "./components/PermissionModal";
import { Sidebar } from "./components/Sidebar";
import { ToastStack } from "./components/ToastStack";
import { Topbar } from "./components/Topbar";
import { api, errorText } from "./lib/api";
import { initJournalSync } from "./lib/journalSync";
import { isPlaceholderTitle, latestPlan, scrubTranscript } from "./lib/acp-parse";
import { applyTheme } from "./lib/theme";
import { statusLabel } from "./lib/i18n";
import type {
  ChatBlock,
  DesktopPrefs,
  DesktopState,
  DiffItem,
  GitStatus,
  GrokEnvironment,
} from "./lib/types";
import { Onboarding } from "./screens/Onboarding";
import { SettingsModal } from "./screens/Settings";
import { usePermission } from "./hooks/usePermission";
import { useResumeSession } from "./hooks/useResumeSession";
import { useAgentEvents } from "./hooks/useAgentEvents";
import { useAgents } from "./hooks/useAgents";
import { useChatScroll } from "./hooks/useChatScroll";
import { useComposerActions } from "./hooks/useComposerActions";
import { useDiffActions } from "./hooks/useDiffActions";
import { useCloudUpdate } from "./hooks/useCloudUpdate";
import { useSettingsHandlers } from "./hooks/useSettingsHandlers";
import {
  HISTORY_INITIAL_DEFAULT,
  useSessionActions,
} from "./hooks/useSessionActions";
import { useToast } from "./hooks/useToast";
import {
  useSessionStore,
  useUiStore,
  type ConfirmState,
} from "./state";

const APP_VERSION = "0.9.5";

const defaultPrefs: DesktopPrefs = {
  grokPath: "",
  permissionMode: "ask",
  sandboxMode: "off",
  model: "",
  theme: "light",
  defaultEditor: "code",
  defaultShell: "powershell",
  minCliVersion: "0.2.0",
  defaultProjectsDir: "D:\\Grok Build",
  showRawAcpEvents: false,
  fsScope: "workspace",
  historyInitialVisible: HISTORY_INITIAL_DEFAULT,
  chatMaskQuiet: false,
};

/** 空数组常量：选择器稳定返回同一引用，避免无关会话流式更新触发重渲染 */
const EMPTY_BLOCKS: ChatBlock[] = [];

/**
 * App 是纯编排层：拉起各领域 hook，拼装布局。
 * 领域逻辑住在 src/hooks/*，展示组件住在 src/components/*。
 */
export default function App() {
  // —— store 切片
  const live = useSessionStore((s) => s.live);
  const setLive = useSessionStore((s) => s.setLive);
  const activeProjectId = useSessionStore((s) => s.activeProjectId);
  const setActiveProjectId = useSessionStore((s) => s.setActiveProjectId);
  const activeSessionId = useSessionStore((s) => s.activeSessionId);
  const setActiveSessionId = useSessionStore((s) => s.setActiveSessionId);
  const setTranscripts = useSessionStore((s) => s.setTranscripts);
  // 只订阅当前会话的 transcript：后台会话的流式事件不再触发 App 重渲染
  const blocks = useSessionStore((s) =>
    activeSessionId
      ? s.transcripts[activeSessionId] ?? EMPTY_BLOCKS
      : EMPTY_BLOCKS
  );
  // 忙时排队 / 静默看门狗（当前会话）
  const queuedCount = useSessionStore((s) =>
    activeSessionId ? s.sendQueue[activeSessionId]?.length ?? 0 : 0
  );
  const activeStall = useSessionStore((s) =>
    activeSessionId ? s.stall[activeSessionId] ?? null : null
  );
  const clearStall = useSessionStore((s) => s.clearStall);
  const historyTail = useUiStore((s) => s.historyTail);
  const showSettings = useUiStore((s) => s.showSettings);
  const setShowSettings = useUiStore((s) => s.setShowSettings);
  const showReview = useUiStore((s) => s.showReview);
  const setShowReview = useUiStore((s) => s.setShowReview);
  const showDashboard = useUiStore((s) => s.showDashboard);
  const setShowDashboard = useUiStore((s) => s.setShowDashboard);
  const topMenuOpen = useUiStore((s) => s.topMenuOpen);
  const setTopMenuOpen = useUiStore((s) => s.setTopMenuOpen);
  const projectMenuId = useUiStore((s) => s.projectMenuId);
  const setProjectMenuId = useUiStore((s) => s.setProjectMenuId);
  const sessionMenuId = useUiStore((s) => s.sessionMenuId);
  const setSessionMenuId = useUiStore((s) => s.setSessionMenuId);
  const transcriptFilter = useUiStore((s) => s.transcriptFilter);
  const setTranscriptFilter = useUiStore((s) => s.setTranscriptFilter);
  const confirm = useUiStore((s) => s.confirm);
  const setConfirm = useUiStore((s) => s.setConfirm);
  const confirmBusy = useUiStore((s) => s.confirmBusy);
  const setConfirmBusy = useUiStore((s) => s.setConfirmBusy);
  const reviewWidth = useUiStore((s) => s.reviewWidth);
  const setReviewWidth = useUiStore((s) => s.setReviewWidth);
  const showDiskHistory = useUiStore((s) => s.showDiskHistory);
  const setShowDiskHistory = useUiStore((s) => s.setShowDiskHistory);

  // —— 本地状态
  const [state, setState] = useState<DesktopState | null>(null);
  const [env, setEnv] = useState<GrokEnvironment | null>(null);
  const [busy, setBusy] = useState(false);
  const [planDismissed, setPlanDismissed] = useState<Record<string, boolean>>(
    {}
  );
  const [sessionQuery, setSessionQuery] = useState("");
  const [git, setGit] = useState<GitStatus | null>(null);

  // —— refs
  const booted = useRef(false);
  const prefsRef = useRef(defaultPrefs);
  const focusRef = useRef<(id: string) => void>(() => {});
  /** 防止连点恢复会话（与 useResumeSession / useChatScroll 共用） */
  const resumeInFlightRef = useRef(false);
  const resizing = useRef(false);

  // —— 派生
  const projects = state?.projects ?? [];
  const prefs = state?.prefs ?? defaultPrefs;
  prefsRef.current = prefs;
  const activeProject =
    projects.find((p) => p.id === activeProjectId) ?? null;
  const activeLive = live.find((s) => s.id === activeSessionId) ?? null;
  const historyInitial = (() => {
    const n = Number(prefs.historyInitialVisible);
    if (n === 30 || n === 50 || n === 100) return n;
    return HISTORY_INITIAL_DEFAULT;
  })();
  const tailLimit = activeSessionId
    ? (historyTail[activeSessionId] ?? historyInitial)
    : historyInitial;
  const earlierCount = Math.max(0, blocks.length - tailLimit);
  const displayBlocks =
    earlierCount > 0 ? blocks.slice(blocks.length - tailLimit) : blocks;
  const cwd = activeLive?.cwd || activeProject?.cwd || "";
  const plan = activeSessionId ? latestPlan(blocks) : null;
  const showPlanBanner =
    !!plan && activeSessionId && !planDismissed[activeSessionId];

  const projectSessions = useMemo(() => {
    const q = sessionQuery.trim().toLowerCase();
    const match = (title: string, meta?: string) =>
      !q ||
      title.toLowerCase().includes(q) ||
      (meta ?? "").toLowerCase().includes(q);

    const fromLive = live.filter(
      (s) =>
        (activeProjectId ? s.projectId === activeProjectId : true) &&
        match(s.title, s.cwd)
    );
    const fromMeta = (state?.sessions ?? []).filter(
      (s) =>
        (activeProjectId ? s.projectId === activeProjectId : true) &&
        !fromLive.some((l) => l.id === s.id) &&
        !isPlaceholderTitle(s.title) &&
        match(s.title, s.cwd)
    );
    // Sort meta by lastActiveAt desc when available
    const metaSorted = [...fromMeta].sort((a, b) =>
      (b.lastActiveAt || "").localeCompare(a.lastActiveAt || "")
    );
    return { fromLive, fromMeta: metaSorted };
  }, [live, state?.sessions, activeProjectId, sessionQuery]);

  const recentSessionIds = useMemo(() => {
    const ids = [
      ...projectSessions.fromLive.map((s) => s.id),
      ...projectSessions.fromMeta.map((s) => s.id),
    ];
    return ids.slice(0, 9);
  }, [projectSessions]);

  // —— toast 队列
  const { flash } = useToast();

  // —— 智能体档案（agents.json）
  const {
    agents,
    enabledAgents,
    selectedAgentId,
    setSelectedAgentId,
    refresh: refreshAgents,
    save: saveAgents,
    agentName,
  } = useAgents({ flash });

  // —— 权限弹窗
  const { permission, setPermission, respondPermission } = usePermission(
    (sessionId) => focusRef.current(sessionId),
    (_sessionId, message) => {
      flash(`发送授权应答失败：${message}，请重试`, "error");
    }
  );

  // —— git 状态
  const lastGitDir = useRef<string | null>(null);
  const refreshGit = useCallback(
    async (dir?: string, force = false) => {
      const d = dir || cwd;
      if (!d) {
        lastGitDir.current = null;
        setGit(null);
        return;
      }
      if (!force && lastGitDir.current === d) {
        return;
      }
      lastGitDir.current = d;
      try {
        const st = await api.gitStatus(d);
        setGit(st);
      } catch {
        setGit(null);
      }
    },
    [cwd]
  );

  // —— 滚动状态机
  const {
    scrollRef,
    onChatScroll,
    chatPaintReady,
    scrollToBottom,
    revealChatAfterPaint,
    beginAtBottom,
    stickToBottom,
  } = useChatScroll({
    blocks,
    activeSessionId,
    activeStatus: activeLive?.status,
    resumeInFlightRef,
  });

  // —— 输入 / 润色 / 发送（不再持全局 busy，按会话状态门禁）
  const { input, setInput, sendPrompt, polishPrompt, polishing } =
    useComposerActions({
    stickToBottom,
    scrollToBottom,
    flash,
  });

  // —— diff 审查
  const {
    diffDecisions,
    diffBusyPath,
    batchBusy,
    forgetSession: forgetDiffDecisions,
    acceptDiff,
    rejectDiff,
    acceptAllPending,
    rejectAllPending,
    copyPatch,
  } = useDiffActions({ cwd, blocks, refreshGit, flash });

  const diffs: DiffItem[] = useMemo(() => {
    const decisions = activeSessionId
      ? diffDecisions[activeSessionId] ?? {}
      : {};
    return blocks
      .filter((b): b is Extract<ChatBlock, { kind: "diff" }> => b.kind === "diff")
      .map((b) => ({
        path: b.path,
        patch: b.patch,
        decision: decisions[b.path] ?? "pending",
      }));
  }, [blocks, activeSessionId, diffDecisions]);
  const pendingDiffCount = diffs.filter(
    (d) => !d.decision || d.decision === "pending"
  ).length;

  // —— 会话 CRUD
  const onSessionDeleted = useCallback(
    (id: string) => {
      forgetDiffDecisions(id);
      setPlanDismissed((prev) => {
        if (!(id in prev)) return prev;
        const next = { ...prev };
        delete next[id];
        return next;
      });
    },
    [forgetDiffDecisions]
  );
  const onSessionCreated = useCallback(() => {}, []);
  const askConfirm = useCallback(
    (c: ConfirmState) => setConfirm(c),
    [setConfirm]
  );
  const {
    addProject,
    createSession,
    loadTranscriptForSession,
    loadDiskHistory,
    deleteDiskSession,
    purgePlaceholderSessions,
    commitSessionRename,
    openRenameDialog,
    renameDialog,
    renameBusy,
    closeRenameDialog,
    submitRenameDialog,
    hibernate,
    deleteSession,
    removeProjectById,
    diskSessions,
    diskFilterProject,
    setDiskFilterProject,
  } = useSessionActions({
    env,
    projects,
    sessions: state?.sessions ?? [],
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
  });

  // —— 恢复会话（依赖 loadTranscriptForSession，必须在 useSessionActions 之后）
  const { resumeMeta, resumeDiskSession, resuming } = useResumeSession({
    busy,
    setBusy,
    projects,
    activeProject,
    setLive,
    setActiveSessionId,
    setActiveProjectId,
    setShowDashboard,
    setShowDiskHistory,
    setState: (st) => setState(st as DesktopState),
    loadTranscriptForSession,
    inFlightRef: resumeInFlightRef,
    onBeforeResume: beginAtBottom,
    onResumed: () => {},
    onResumeError: () => {
      revealChatAfterPaint(useSessionStore.getState().activeSessionId);
    },
    flash,
  });

  const restartSession = async () => {
    if (!activeLive) return;
    const meta = {
      id: activeLive.id,
      grokSessionId: activeLive.grokSessionId,
      projectId: activeLive.projectId,
      cwd: activeLive.cwd,
      title: activeLive.title,
      agentId: activeLive.agentId ?? "grok",
    };
    await hibernate(activeLive.id);
    if (meta.grokSessionId) {
      await resumeMeta(meta);
    } else {
      const proj = projects.find((p) => p.id === meta.projectId);
      if (proj) await createSession(proj, meta.agentId);
    }
  };

  // —— 云端更新（GitHub Releases）
  const {
    cloudUpdate,
    updateBusy,
    updateProgress,
    refreshCloudUpdate,
    launchCloudUpdate,
  } = useCloudUpdate({
    askConfirm,
    flash,
  });

  // —— 跨组件入口
  const focusSession = useCallback(
    async (sessionId: string) => {
      setActiveSessionId(sessionId);
      setShowDashboard(false);
      const s = live.find((x) => x.id === sessionId);
      if (s) setActiveProjectId(s.projectId);
      else {
        const meta = state?.sessions.find((x) => x.id === sessionId);
        if (meta) setActiveProjectId(meta.projectId);
      }
      try {
        await api.focusMainWindow(sessionId);
      } catch {
        /* optional */
      }
    },
    [live, state?.sessions, setActiveSessionId, setShowDashboard, setActiveProjectId]
  );

  const notifyOs = useCallback(async (title: string, body: string) => {
    try {
      let granted = await isPermissionGranted();
      if (!granted) {
        const p = await requestPermission();
        granted = p === "granted";
      }
      if (granted) {
        sendNotification({ title, body });
      }
    } catch {
      /* optional */
    }
  }, []);

  const refresh = useCallback(async () => {
    const [st, e, l] = await Promise.all([
      api.getAppState(),
      api.probeEnvironment(),
      api.listLiveSessions(),
    ]);
    setState(st);
    setEnv(e);
    setLive(l);
    setActiveProjectId((cur) => cur ?? st.projects[0]?.id ?? null);
    applyTheme(st.prefs?.theme || "light");
  }, [setLive, setActiveProjectId]);

  const runConfirm = useCallback(async () => {
    if (!confirm) return;
    setConfirmBusy(true);
    try {
      await confirm.onConfirm();
      setConfirm(null);
    } catch (e) {
      // 确认类操作失败必须反馈；否则弹窗关掉且用户不知情
      flash(`操作失败：${errorText(e)}`, "error");
    } finally {
      setConfirmBusy(false);
    }
  }, [confirm, flash, setConfirm, setConfirmBusy]);

  const openSettings = useCallback(() => {
    // 关掉所有 overflow 菜单，避免 menu-backdrop 挡住设置交互
    setTopMenuOpen(false);
    setProjectMenuId(null);
    setSessionMenuId(null);
    setShowSettings(true);
  }, [setShowSettings, setTopMenuOpen, setProjectMenuId, setSessionMenuId]);

  // —— 事件订阅（六个 api.on + 任务完成 toast）
  useAgentEvents({
    setState,
    setLive,
    setPermission,
    prefsRef,
    focusRef,
    notifyOs,
    flash,
  });

  // —— 手工 ref 同步（快捷键 / 事件订阅读到最新闭包）
  const createSessionRef = useRef(createSession);
  createSessionRef.current = createSession;
  const recentRef = useRef(recentSessionIds);
  recentRef.current = recentSessionIds;
  focusRef.current = focusSession;
  const resumeMetaRef = useRef(resumeMeta);
  resumeMetaRef.current = resumeMeta;

  // Keyboard shortcuts
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "n") {
        e.preventDefault();
        void createSessionRef.current(
          null,
          useUiStore.getState().selectedAgentId ?? "grok"
        );
      }
      if (mod && e.key.toLowerCase() === ",") {
        e.preventDefault();
        openSettings();
      }
      if (mod && e.key.toLowerCase() === "b") {
        e.preventDefault();
        setShowReview((v) => !v);
      }
      if (mod && e.key.toLowerCase() === "d") {
        e.preventDefault();
        setShowDashboard(true);
        setActiveSessionId(null);
      }
      // Ctrl+1..9 switch recent sessions
      if (mod && e.key >= "1" && e.key <= "9") {
        e.preventDefault();
        const idx = Number(e.key) - 1;
        const id = recentRef.current[idx];
        if (!id) return;
        const isLive = live.some((s) => s.id === id);
        if (isLive) {
          void focusRef.current(id);
        } else {
          const meta = state?.sessions.find((s) => s.id === id);
          if (meta) void resumeMetaRef.current(meta);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [live, state?.sessions, openSettings, setShowReview, setShowDashboard, setActiveSessionId]);

  useEffect(() => {
    if (booted.current) return;
    booted.current = true;
    // 自有日志节流落盘（≥500ms；实时事件为真值，CLI 历史降级为兜底）
    initJournalSync();
    refresh().catch((e) => flash(String(e), "error"));
    void refreshAgents();
  }, [refresh, flash, refreshAgents]);

  // Apply review width CSS var
  useEffect(() => {
    document.documentElement.style.setProperty(
      "--review-w",
      `${reviewWidth}px`
    );
    try {
      localStorage.setItem("reviewWidth", String(reviewWidth));
    } catch {
      /* ignore */
    }
  }, [reviewWidth]);

  // Theme when prefs change
  useEffect(() => {
    applyTheme(prefs.theme || "light");
  }, [prefs.theme]);

  // Tray aggregate status + focus target for tray / second-instance.
  // 防抖 250ms：live 在流式/启动期间高频变化，逐次 IPC 更新托盘会
  // 给主线程事件泵添堵（托盘调用最终都在主线程执行）。
  useEffect(() => {
    const t = window.setTimeout(() => {
      const waiting = live.filter((s) => s.status === "waiting_permission");
      const running = live.filter((s) => s.status === "running");
      const errors = live.filter((s) => s.status === "error");
      let level = "idle";
      let detail = "";
      let focusId: string | null = null;
      if (waiting.length) {
        level = "needs_attention";
        detail = waiting[0].title;
        focusId = waiting[0].id;
      } else if (errors.length) {
        level = "error";
        detail = errors[0].title;
        focusId = errors[0].id;
      } else if (running.length) {
        level = "running";
        detail = `${running.length} 个任务`;
        focusId = running[0].id;
      } else if (live.length) {
        detail = `${live.length} 个会话`;
        focusId = live[0].id;
      }
      void api.updateTrayStatus(level, detail, focusId).catch(() => {});
    }, 250);
    return () => window.clearTimeout(t);
  }, [live]);

  // Strip silent / hidden raw events when switching sessions or prefs change
  useEffect(() => {
    if (!activeSessionId) return;
    const showRaw = prefs.showRawAcpEvents === true;
    setTranscripts((prev) => {
      const cur = prev[activeSessionId];
      if (!cur) return prev;
      const cleaned = scrubTranscript(cur, showRaw);
      if (cleaned.length === cur.length && cleaned.every((b, i) => b === cur[i])) {
        return prev;
      }
      return { ...prev, [activeSessionId]: cleaned };
    });
  }, [activeSessionId, prefs.showRawAcpEvents, setTranscripts]);

  useEffect(() => {
    void refreshGit(cwd);
  }, [cwd, refreshGit]);

  // Review pane drag resize
  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!resizing.current) return;
      const w = window.innerWidth - e.clientX;
      setReviewWidth(Math.min(640, Math.max(280, w)));
    };
    const onUp = () => {
      resizing.current = false;
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [setReviewWidth]);

  // —— Settings 回调（稳定引用，避免弹窗重渲染关掉原生 <select>）
  const settingsHandlers = useSettingsHandlers({
    setState,
    setEnv,
    refreshCloudUpdate,
    launchCloudUpdate,
    flash,
  });

  if (state && !state.onboardingDone) {
    return (
      <div className="app" style={{ gridTemplateColumns: "1fr" }}>
        <Onboarding
          env={env}
          onRefresh={() => void refresh()}
          onContinue={async () => {
            const st = await api.setOnboardingDone(true);
            setState(st);
          }}
        />
      </div>
    );
  }

  return (
    <div className={`app ${showReview ? "with-review" : ""}`}>
      <Sidebar
        appVersion={APP_VERSION}
        cloudUpdate={cloudUpdate}
        updateBusy={updateBusy}
        onLaunchUpdate={() => void launchCloudUpdate()}
        projects={projects}
        activeProjectId={activeProjectId}
        onSelectProject={(id) => {
          // 点项目 = 切换工作区：若当前会话属于别的项目，让位给该项目的空态引导
          const cur = live.find((s) => s.id === activeSessionId);
          if (cur && cur.projectId !== id) setActiveSessionId(null);
          setActiveProjectId(id);
          setShowDashboard(false);
          setProjectMenuId(null);
          // 进程回收：休眠其他项目的空闲会话（运行中/待授权不动），
          // 再按后端真值刷新活跃列表，防止 grok 进程随切换次数堆积
          void api
            .setActiveProject(id)
            .then(() => api.listLiveSessions())
            .then(setLive)
            .catch(() => {});
        }}
        onAddProject={() => void addProject()}
        onCreateSession={(p) => void createSession(p, selectedAgentId)}
        onRemoveProject={removeProjectById}
        projectMenuId={projectMenuId}
        setProjectMenuId={setProjectMenuId}
        sessionMenuId={sessionMenuId}
        setSessionMenuId={setSessionMenuId}
        setTopMenuOpen={setTopMenuOpen}
        sessionQuery={sessionQuery}
        setSessionQuery={setSessionQuery}
        fromLive={projectSessions.fromLive}
        fromMeta={projectSessions.fromMeta}
        activeSessionId={activeSessionId}
        busy={busy}
        enabledAgents={enabledAgents}
        selectedAgentId={selectedAgentId}
        onSelectAgent={setSelectedAgentId}
        agentName={agentName}
        onSelectLive={(id) => {
          setActiveSessionId(id);
          setShowDashboard(false);
        }}
        onResumeMeta={(s) => void resumeMeta(s)}
        onCommitRename={(id, title) => void commitSessionRename(id, title)}
        onHibernate={(id) => void hibernate(id)}
        onDeleteSession={(id, title) => void deleteSession(id, title)}
        onPurgePlaceholders={() => void purgePlaceholderSessions()}
        onLoadDiskHistory={() => void loadDiskHistory()}
        onShowDashboard={() => {
          setShowDashboard(true);
          setActiveSessionId(null);
        }}
        onShowSettings={openSettings}
      />

      <main className="main">
        <Topbar
          showDashboard={showDashboard}
          liveCount={live.length}
          activeLive={activeLive}
          activeProject={activeProject}
          cwd={cwd}
          prefs={prefs}
          agentLabel={agentName(activeLive?.agentId) ?? agentName(selectedAgentId)}
          createAgentLabel={agentName(selectedAgentId)}
          pendingDiffCount={pendingDiffCount}
          showReview={showReview}
          busy={busy}
          topMenuOpen={topMenuOpen}
          setTopMenuOpen={setTopMenuOpen}
          setProjectMenuId={setProjectMenuId}
          setSessionMenuId={setSessionMenuId}
          onToggleReview={() => setShowReview((v) => !v)}
          onCreateSession={() => void createSession(null, selectedAgentId)}
          onLoadDiskHistory={() => void loadDiskHistory()}
          onOpenSettings={openSettings}
          onOpenTerminal={(c) =>
            void api
              .openExternalTerminal(c)
              .catch((e) => flash(String(e), "error"))
          }
          onOpenEditor={(c) =>
            void api.openInEditor(c).catch((e) => flash(String(e), "error"))
          }
          onRestartSession={() => void restartSession()}
          onRevealLogs={() => void api.revealLogs()}
          onExportDiagnostics={() => void settingsHandlers.onExportDiagnostics()}
        />

        <div
          className="chat-scroll"
          ref={scrollRef}
          onScroll={onChatScroll}
        >
          {!showDashboard && activeSessionId && !chatPaintReady && (
            <div className="chat-paint-mask" aria-busy="true" aria-live="polite">
              <div className="chat-paint-mask-inner">
                {!prefs.chatMaskQuiet && (
                  <>
                    <span className="chat-paint-spinner" />
                    <span>整理对话…</span>
                  </>
                )}
              </div>
            </div>
          )}
          <div
            className={
              !showDashboard && activeSessionId
                ? `chat-scroll-body${chatPaintReady ? " is-ready" : " is-painting"}`
                : "chat-scroll-body is-ready"
            }
          >
          {showDashboard ? (
            <Dashboard
              live={live}
              projects={projects}
              activeSessionId={activeSessionId}
              onSelect={(id) => {
                setActiveSessionId(id);
                setShowDashboard(false);
              }}
              onNew={() => void createSession(null, selectedAgentId)}
              onHibernate={(id) => void hibernate(id)}
              onDelete={(id, title) => void deleteSession(id, title)}
            />
          ) : !activeSessionId ? (
            <EmptyWorkspace
              activeProject={activeProject}
              busy={busy}
              onCreateSession={() => void createSession(null, selectedAgentId)}
              onLoadDiskHistory={() => void loadDiskHistory()}
              onShowDashboard={() => setShowDashboard(true)}
            />
          ) : (
            <>
              {showPlanBanner && plan && (
                <PlanBanner
                  plan={plan}
                  onDismiss={() =>
                    setPlanDismissed((p) => ({
                      ...p,
                      [activeSessionId!]: true,
                    }))
                  }
                  onConfirm={() => {
                    setInput("请按计划执行");
                    setPlanDismissed((p) => ({
                      ...p,
                      [activeSessionId!]: true,
                    }));
                    flash("已填入确认指令，按 Enter 发送", "info");
                  }}
                />
              )}
              {blocks.length > 0 && (
                <TranscriptToolbar
                  filter={transcriptFilter}
                  onFilter={setTranscriptFilter}
                  count={blocks.length}
                  onScrollBottom={() => {
                    stickToBottom();
                    scrollToBottom(true);
                  }}
                />
              )}
              {activeSessionId && earlierCount > 0 && (
                <HistoryLoadEarlier
                  sessionId={activeSessionId}
                  total={blocks.length}
                  shown={displayBlocks.length}
                  earlierCount={earlierCount}
                  historyInitial={historyInitial}
                  scrollRef={scrollRef}
                />
              )}
              <ChatBlocks
                blocks={displayBlocks}
                showRawAcpEvents={prefs.showRawAcpEvents === true}
                filter={transcriptFilter}
              />
            </>
          )}
          </div>
        </div>

        {activeStall && activeLive && (
          <div className="stall-banner" role="status">
            <span>
              小精灵已静默约 {Math.max(1, Math.round(activeStall.silentSecs / 60))}{" "}
              分钟。可能仍在思考/等待长任务；不会自动取消。
            </span>
            <span className="spacer" />
            <button
              type="button"
              className="btn sm"
              onClick={() => {
                clearStall(activeLive.id);
                void api
                  .stallKeepWaiting(activeLive.id)
                  .catch((e) => flash(String(e), "error"));
              }}
            >
              继续等待
            </button>
            <button
              type="button"
              className="btn sm danger"
              onClick={() => {
                clearStall(activeLive.id);
                void api
                  .cancelPrompt(activeLive.id)
                  .then(() => flash("已请求结束本轮", "info"))
                  .catch((e) => flash(String(e), "error"));
              }}
            >
              结束本轮
            </button>
          </div>
        )}

        <Composer
          activeSessionId={activeSessionId}
          agentName={agentName(activeLive?.agentId)}
          input={input}
          setInput={setInput}
          busy={busy}
          turnRunning={
            activeLive?.status === "running" ||
            activeLive?.status === "waiting_permission"
          }
          queuedCount={queuedCount}
          statusHint={statusLabel(activeLive?.status)}
          showStop={
            !!(
              activeLive &&
              (activeLive.status === "running" || busy)
            )
          }
          onStop={() => {
            if (!activeLive) return;
            void api
              .cancelPrompt(activeLive.id)
              .then(() => flash("已请求停止", "info"))
              .catch((e) => flash(String(e), "error"));
          }}
          onSend={() => void sendPrompt()}
          polishing={polishing}
          onPolish={() => void polishPrompt()}
        />
      </main>

      {resuming && (
        <div className="resume-banner" role="status">
          <span className="chat-paint-spinner" />
          <span>正在恢复会话「{resuming.title}」…</span>
          <button
            type="button"
            className="btn sm"
            onClick={() => {
              void api
                .cancelStart(resuming.id)
                .then(() => flash("已取消恢复", "info"))
                .catch((e) => flash(String(e), "error"));
            }}
          >
            取消
          </button>
        </div>
      )}

      {showReview && (
        <div className="review-col">
          <div
            className="review-resize"
            onMouseDown={(e) => {
              e.preventDefault();
              resizing.current = true;
            }}
            title="拖拽调整审查面板宽度"
          />
          <ReviewPane
            diffs={diffs}
            git={git}
            busyPath={diffBusyPath}
            batchBusy={batchBusy}
            onClose={() => setShowReview(false)}
            onOpen={(path) => void api.openInEditor(path)}
            onAccept={(path, patch) => void acceptDiff(path, patch)}
            onReject={(path) => void rejectDiff(path)}
            onAcceptAll={() => void acceptAllPending()}
            onRejectAll={() => void rejectAllPending()}
            onRefreshGit={() => void refreshGit(undefined, true)}
            onOpenProject={
              cwd
                ? () =>
                    void api
                      .openInEditor(cwd)
                      .catch((e) => flash(String(e), "error"))
                : undefined
            }
            onCopyPatch={(path, patch) => void copyPatch(path, patch)}
          />
        </div>
      )}

      <ConfirmDialog
        open={!!confirm}
        title={confirm?.title || ""}
        message={confirm?.message || ""}
        danger={confirm?.danger}
        confirmLabel={confirm?.confirmLabel}
        busy={confirmBusy}
        onCancel={() => {
          if (!confirmBusy) setConfirm(null);
        }}
        onConfirm={() => void runConfirm()}
      />

      <InputDialog
        open={!!renameDialog}
        title="重命名会话"
        initialValue={renameDialog?.title ?? ""}
        busy={renameBusy}
        onSubmit={(v) => void submitRenameDialog(v)}
        onCancel={() => {
          if (!renameBusy) closeRenameDialog();
        }}
      />

      {showSettings && state && (
        <SettingsModal
          prefs={prefs}
          env={env}
          cloudUpdate={cloudUpdate}
          updateProgress={updateProgress}
          updateBusy={updateBusy}
          onRefreshCloudUpdate={settingsHandlers.onRefreshCloudUpdate}
          onLaunchCloudUpdate={settingsHandlers.onLaunchCloudUpdate}
          onClose={settingsHandlers.onClose}
          onOpenConfig={settingsHandlers.onOpenConfig}
          onRevealLogs={settingsHandlers.onRevealLogs}
          onExportDiagnostics={settingsHandlers.onExportDiagnostics}
          onFlash={settingsHandlers.onFlash}
          agents={agents}
          onSaveAgents={async (profiles) => {
            await saveAgents(profiles);
          }}
          onSave={settingsHandlers.onSave}
        />
      )}

      {showDiskHistory && (
        <DiskHistoryModal
          sessions={diskSessions}
          filterByProject={diskFilterProject}
          onFilterByProjectChange={setDiskFilterProject}
          canFilterProject={!!activeProject}
          busy={busy}
          onResume={(d) => void resumeDiskSession(d)}
          onDelete={(d) => void deleteDiskSession(d)}
          onClose={() => setShowDiskHistory(false)}
          onRefresh={(v) => void loadDiskHistory(v)}
        />
      )}

      {permission && (
        <PermissionModal
          request={permission}
          onRespond={(allow, optionId, rememberSession) => {
            void respondPermission(allow, optionId, rememberSession);
          }}
        />
      )}

      <ToastStack onFocusSession={(id) => void focusSession(id)} />
    </div>
  );
}
