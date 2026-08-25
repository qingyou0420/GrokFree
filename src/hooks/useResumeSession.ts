import { useCallback, useRef, type MutableRefObject } from "react";
import { api } from "../lib/api";
import type { DiskSession, LiveSession, Project } from "../lib/types";

function pathsEqual(a: string, b: string) {
  const norm = (p: string) =>
    p.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
  return norm(a) === norm(b);
}

function uid(p: string) {
  return `${p}_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`;
}

type Flash = (text: string, kind?: "info" | "success" | "error", sid?: string) => void;

type LoadTranscript = (
  desktopSessionId: string,
  grokSessionId: string,
  pathHint?: string | null,
  banner?: string
) => Promise<void>;

/**
 * Resume sidebar meta / disk history sessions with in-flight guard.
 */
export function useResumeSession(opts: {
  busy: boolean;
  setBusy: (v: boolean) => void;
  projects: Project[];
  activeProject: Project | null;
  setLive: (
    v: LiveSession[] | ((prev: LiveSession[]) => LiveSession[])
  ) => void;
  setActiveSessionId: (id: string | null) => void;
  setActiveProjectId: (id: string | null) => void;
  setShowDashboard: (v: boolean) => void;
  setShowDiskHistory: (v: boolean) => void;
  setState: (st: unknown) => void;
  loadTranscriptForSession: LoadTranscript;
  /** Shared with scroll paint logic in App */
  inFlightRef?: MutableRefObject<boolean>;
  onBeforeResume?: () => void;
  onResumed?: () => void;
  onResumeError?: () => void;
  flash: Flash;
}) {
  const ownInFlight = useRef(false);
  const inFlight = opts.inFlightRef ?? ownInFlight;

  const resumeMeta = useCallback(
    async (meta: {
      id: string;
      grokSessionId?: string | null;
      projectId: string;
      cwd: string;
      title: string;
      agentId?: string | null;
    }) => {
      if (!meta.grokSessionId) {
        opts.flash("没有可恢复的会话 ID", "error");
        return;
      }
      if (opts.busy || inFlight.current) {
        opts.flash("正在恢复会话，请稍候…", "info");
        return;
      }
      inFlight.current = true;
      opts.onBeforeResume?.();
      opts.setBusy(true);
      opts.setShowDashboard(false);
      try {
        let pathHint: string | null = null;
        try {
          pathHint = await api.resolveDiskSessionPath(meta.grokSessionId);
        } catch {
          pathHint = null;
        }
        const session = await api.resumeSession({
          desktopSessionId: meta.id,
          grokSessionId: meta.grokSessionId,
          projectId: meta.projectId,
          cwd: meta.cwd,
          title: meta.title,
          agentId: meta.agentId ?? "grok",
        });
        opts.setLive((prev) => [
          session,
          ...prev.filter((s) => s.id !== session.id),
        ]);
        opts.setActiveSessionId(session.id);
        opts.setActiveProjectId(meta.projectId);
        const grokId = session.grokSessionId || meta.grokSessionId;
        await opts.loadTranscriptForSession(
          session.id,
          grokId,
          pathHint,
          `已恢复历史会话 · ${meta.title}`
        );
        opts.flash("会话已恢复", "success", session.id);
        opts.onResumed?.();
      } catch (e) {
        opts.flash(`恢复失败：${e}`, "error");
        opts.onResumeError?.();
      } finally {
        inFlight.current = false;
        opts.setBusy(false);
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps -- intentional opts bag
    [opts.busy, opts.projects, opts.activeProject, opts.loadTranscriptForSession, inFlight]
  );

  const resumeDiskSession = useCallback(
    async (d: DiskSession) => {
      if (opts.busy || inFlight.current) {
        opts.flash("正在恢复会话，请稍候…", "info");
        return;
      }
      const project =
        opts.projects.find((p) => d.cwd && pathsEqual(p.cwd, d.cwd)) ||
        opts.projects.find(
          (p) =>
            d.cwd &&
            p.cwd
              .toLowerCase()
              .includes(
                d.cwd.replace(/\\/g, "/").split("/").filter(Boolean).pop() ||
                  ""
              )
        ) ||
        opts.activeProject ||
        opts.projects[0];
      if (!project) {
        opts.flash("请先添加与会话匹配的项目", "error");
        return;
      }
      const cwdUse = d.cwd || project.cwd;
      inFlight.current = true;
      opts.onBeforeResume?.();
      opts.setBusy(true);
      opts.setShowDiskHistory(false);
      opts.setShowDashboard(false);
      try {
        // 磁盘历史即 ~/.grok/sessions，只有 grok 档案能恢复
        const session = await api.resumeSession({
          desktopSessionId: uid("desk"),
          grokSessionId: d.id,
          projectId: project.id,
          cwd: cwdUse,
          title: d.title || project.name,
          agentId: "grok",
        });
        opts.setLive((prev) => [
          session,
          ...prev.filter((s) => s.id !== session.id),
        ]);
        opts.setActiveSessionId(session.id);
        opts.setActiveProjectId(project.id);
        await opts.loadTranscriptForSession(
          session.id,
          d.id,
          d.path,
          `已从磁盘恢复 · ${d.title || d.id} · ${cwdUse}`
        );
        opts.flash(`已恢复磁盘会话：${d.title}`, "success", session.id);
        opts.onResumed?.();
        const st = await api.getAppState();
        opts.setState(st);
      } catch (e) {
        opts.flash(`磁盘会话恢复失败：${e}`, "error");
        opts.onResumeError?.();
      } finally {
        inFlight.current = false;
        opts.setBusy(false);
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [opts.busy, opts.projects, opts.activeProject, opts.loadTranscriptForSession, inFlight]
  );

  return { resumeMeta, resumeDiskSession, resumeInFlightRef: inFlight };
}
