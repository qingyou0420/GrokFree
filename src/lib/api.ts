import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AgentProfile,
  ApplyDiffResult,
  ChatBlock,
  CliCapabilities,
  CloudUpdateInfo,
  DesktopPrefs,
  DesktopState,
  DiskSession,
  GitStatus,
  GrokEnvironment,
  LiveSession,
  SkillsMcpSnapshot,
} from "./types";

/** Human-readable message for unknown rejection values (Tauri rejects with strings). */
export function errorText(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  return String(e);
}

export const api = {
  getAppState: () => invoke<DesktopState>("get_app_state"),
  reloadState: () => invoke<DesktopState>("reload_state"),
  probeEnvironment: () => invoke<GrokEnvironment>("probe_environment"),
  updatePrefs: (prefs: DesktopPrefs) =>
    invoke<DesktopState>("update_prefs", { prefs }),
  setOnboardingDone: (done: boolean) =>
    invoke<DesktopState>("set_onboarding_done", { done }),
  addProject: (cwd: string) => invoke<DesktopState>("add_project", { cwd }),
  removeProject: (projectId: string) =>
    invoke<DesktopState>("remove_project", { projectId }),
  createSession: (
    projectId: string,
    cwd: string,
    title?: string,
    agentId?: string
  ) =>
    invoke<LiveSession>("create_session", {
      projectId,
      cwd,
      title,
      agentId: agentId ?? null,
    }),
  resumeSession: (args: {
    desktopSessionId: string;
    grokSessionId: string;
    projectId: string;
    cwd: string;
    title: string;
    agentId?: string | null;
  }) => invoke<LiveSession>("resume_session", args),
  listAgents: () => invoke<AgentProfile[]>("list_agents"),
  saveAgents: (profiles: AgentProfile[]) =>
    invoke<AgentProfile[]>("save_agents", { profiles }),
  listLiveSessions: () => invoke<LiveSession[]>("list_live_sessions"),
  listDiskSessions: (limit?: number, cwd?: string | null) =>
    invoke<DiskSession[]>("list_disk_sessions", {
      limit: limit ?? 80,
      cwd: cwd ?? null,
    }),
  resolveDiskSessionPath: (sessionId: string) =>
    invoke<string | null>("resolve_disk_session_path", { sessionId }),
  loadDiskTranscript: (sessionId: string, path?: string | null) =>
    invoke<ChatBlock[]>("load_disk_transcript", {
      sessionId,
      path: path ?? null,
    }),
  deleteDiskSession: (sessionId: string, path?: string | null) =>
    invoke<void>("delete_disk_session", {
      sessionId,
      path: path ?? null,
    }),
  renameSession: (sessionId: string, title: string) =>
    invoke<DesktopState>("rename_session", { sessionId, title }),
  /** 自有会话日志：读取快照（无日志返回 null，退回 CLI 历史） */
  loadJournal: (sessionId: string) =>
    invoke<ChatBlock[] | null>("load_journal", { sessionId }),
  /** 自有会话日志：保存快照（journalSync 节流后调用） */
  saveJournal: (sessionId: string, blocks: ChatBlock[]) =>
    invoke<void>("save_journal", { sessionId, blocks }),
  removeSessionMeta: (sessionId: string) =>
    invoke<DesktopState>("remove_session_meta", { sessionId }),
  purgeStaleSessionMeta: (projectId?: string | null) =>
    invoke<DesktopState>("purge_stale_session_meta", {
      projectId: projectId ?? null,
    }),
  sendPrompt: (sessionId: string, text: string) =>
    invoke<void>("send_prompt", { sessionId, text }),
  cancelPrompt: (sessionId: string) =>
    invoke<void>("cancel_prompt", { sessionId }),
  respondPermission: (
    sessionId: string,
    requestId: unknown,
    allow: boolean,
    optionId?: string,
    rememberScope?: string | null
  ) =>
    invoke<void>("respond_permission", {
      sessionId,
      requestId,
      allow,
      optionId: optionId ?? null,
      rememberScope: rememberScope ?? null,
    }),
  /** 静默提示「继续等待」：重置该会话的静默计时 */
  stallKeepWaiting: (sessionId: string) =>
    invoke<void>("stall_keep_waiting", { sessionId }),
  /** 取消在途的会话启动/恢复（杀 initialize/load 中的进程） */
  cancelStart: (sessionId: string) =>
    invoke<void>("cancel_start", { sessionId }),
  handleServerRequest: (
    sessionId: string,
    requestId: unknown,
    method: string,
    params: unknown
  ) =>
    invoke<void>("handle_server_request", {
      sessionId,
      requestId,
      method,
      params,
    }),
  hibernateSession: (sessionId: string) =>
    invoke<void>("hibernate_session", { sessionId }),
  /** 项目切换钩子：休眠其他项目的空闲会话（回收 grok 进程），返回回收数 */
  setActiveProject: (projectId: string) =>
    invoke<number>("set_active_project", { projectId }),
  gitStatus: (cwd: string) => invoke<GitStatus>("git_status", { cwd }),
  applyDiff: (cwd: string, path: string, patch: string) =>
    invoke<ApplyDiffResult>("apply_diff", { cwd, path, patch }),
  rejectDiff: (path: string) => invoke<ApplyDiffResult>("reject_diff", { path }),
  exportDiagnostics: () => invoke<string>("export_diagnostics"),
  openExternalTerminal: (cwd: string) =>
    invoke<void>("open_external_terminal", { cwd }),
  openConfigFile: () => invoke<void>("open_config_file"),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  openInEditor: (path: string) => invoke<void>("open_in_editor", { path }),
  revealLogs: () => invoke<void>("reveal_logs"),
  readFile: (path: string) => invoke<string>("read_file", { path }),
  appInfo: () => invoke<Record<string, string>>("app_info"),
  openInstallersDir: () => invoke<string>("open_installers_dir"),
  listSkillsMcp: () => invoke<SkillsMcpSnapshot>("list_skills_mcp"),
  cliCapabilities: () => invoke<CliCapabilities>("cli_capabilities"),
  updateTrayStatus: (
    level: string,
    detail?: string,
    focusSessionId?: string | null
  ) =>
    invoke<void>("update_tray_status", {
      level,
      detail: detail ?? "",
      focusSessionId: focusSessionId ?? null,
    }),
  focusMainWindow: (sessionId?: string | null) =>
    invoke<void>("focus_main_window", { sessionId: sessionId ?? null }),

  checkCloudUpdate: () =>
    invoke<CloudUpdateInfo | null>("check_cloud_update"),
  launchCloudUpdate: () => invoke<CloudUpdateInfo>("launch_cloud_update"),

  getDefaultProjectsDir: () => invoke<string>("get_default_projects_dir"),

  pickDirectory: async (defaultPath?: string) => {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath:
        defaultPath && defaultPath.trim()
          ? defaultPath
          : await invoke<string>("get_default_projects_dir").catch(() => "D:\\Grok Build"),
    });
    if (!selected || Array.isArray(selected)) return null;
    return selected;
  },

  on: async <T>(
    event: string,
    handler: (payload: T) => void
  ): Promise<UnlistenFn> => {
    return listen<T>(event, (e) => handler(e.payload));
  },
};
