export type DesktopPrefs = {
  grokPath: string;
  permissionMode: string;
  sandboxMode: string;
  model: string;
  theme: string;
  defaultEditor: string;
  defaultShell: string;
  minCliVersion: string;
  /** 添加项目时文件夹选择器默认打开的目录 */
  defaultProjectsDir: string;
  /** 默认 false：折叠/隐藏未知 ACP 事件，避免刷屏 */
  showRawAcpEvents?: boolean;
  /** ACP 文件范围：workspace | unrestricted */
  fsScope?: string;
  /** 历史会话首屏可见条数：30 | 50 | 100 */
  historyInitialVisible?: number;
  /** 会话切换遮罩不显示文字（仅挡闪） */
  chatMaskQuiet?: boolean;
};

export type Project = {
  id: string;
  name: string;
  cwd: string;
  createdAt: string;
  lastOpenedAt: string;
};

export type SessionMeta = {
  id: string;
  projectId: string;
  title: string;
  cwd: string;
  grokSessionId?: string | null;
  /** 使用的小精灵档案 id（agents.json），默认 "grok" */
  agentId?: string;
  status: string;
  createdAt: string;
  lastActiveAt: string;
  /** 历史委托标记（兼容旧状态） */
  delegatedBy?: string | null;
  jobId?: string | null;
};

export type DesktopState = {
  prefs: DesktopPrefs;
  projects: Project[];
  sessions: SessionMeta[];
  onboardingDone: boolean;
};

export type LiveSession = {
  id: string;
  projectId: string;
  title: string;
  cwd: string;
  grokSessionId?: string | null;
  /** 使用的小精灵档案 id（agents.json） */
  agentId?: string;
  status: string;
  error?: string | null;
  delegatedBy?: string | null;
  jobId?: string | null;
};

/** 小精灵档案：如何启动一个说 ACP 的进程（agents.json） */
export type AgentProfile = {
  id: string;
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  models: string[];
  defaultModel: string;
  isGrok: boolean;
  supportsResume: boolean;
  enabled: boolean;
  note: string;
};

/** Latest GitHub Release installer (cloud update). */
export type CloudUpdateInfo = {
  version: string;
  currentVersion: string;
  isNewer: boolean;
  canInstall: boolean;
  downloadUrl: string;
  fileName: string;
  htmlUrl: string;
  notes?: string | null;
  sizeBytes: number;
  publishedAt?: string | null;
  localPath?: string | null;
};

export type UpdateProgress = {
  phase: string;
  received: number;
  total: number;
  percent: number;
};

export type CliCapabilities = {
  agentSandboxFlag: boolean;
  permissionModes: boolean;
  modelOverride: boolean;
  terminalHost: boolean;
  sessionResume: boolean;
  skillsMcpReadonly: boolean;
  notes: string[];
};

export type GrokEnvironment = {
  grokHome: string;
  grokPath: string;
  grokExists: boolean;
  grokVersion: string | null;
  cliVersionOk: boolean;
  minCliVersion: string;
  configPath: string;
  configExists: boolean;
  authPath: string;
  authExists: boolean;
  authLoggedIn: boolean;
  sessionsDir: string;
  desktopDataDir: string;
  capabilities: CliCapabilities;
};

export type SkillInfo = {
  name: string;
  path: string;
  source: string;
};

export type McpServerInfo = {
  name: string;
  detail: string;
};

export type SkillsMcpSnapshot = {
  skills: SkillInfo[];
  mcpServers: McpServerInfo[];
  configPath: string;
  skillsDir: string;
  notes: string[];
};

export type DiskSession = {
  id: string;
  title: string;
  cwd?: string | null;
  createdAt?: string | null;
  updatedAt?: string | null;
  path: string;
  source: string;
  messageCount?: number | null;
};

export type GitEntry = {
  path: string;
  status: string;
};

export type GitStatus = {
  isRepo: boolean;
  branch?: string | null;
  ahead?: number | null;
  behind?: number | null;
  dirty: boolean;
  entries: GitEntry[];
  message: string;
};

export type ApplyDiffResult = {
  ok: boolean;
  path: string;
  method: string;
  message: string;
};

export type DiffItem = {
  path: string;
  patch: string;
  /** accepted | rejected | pending */
  decision?: "accepted" | "rejected" | "pending";
};

export type ChatBlock =
  | { kind: "user"; id: string; text: string }
  | { kind: "assistant"; id: string; text: string }
  | { kind: "thought"; id: string; text: string }
  | {
      kind: "tool";
      id: string;
      toolCallId: string;
      title: string;
      status: string;
      input?: unknown;
      output?: unknown;
      /** Detected subagent / nested task */
      subagent?: boolean;
    }
  | { kind: "plan"; id: string; text: string; entries?: PlanEntry[] }
  | { kind: "system"; id: string; text: string }
  | { kind: "diff"; id: string; path: string; patch: string }
  | { kind: "raw"; id: string; method: string; text: string };

export type PlanEntry = {
  content: string;
  status?: string;
  priority?: string;
};

export type PermissionReq = {
  sessionId: string;
  id: unknown;
  method: string;
  params: Record<string, unknown>;
  /** 后端计算的权限范围键（「本会话内允许」按此聚类缓存） */
  scopeKey?: string;
};

export type ToastState = {
  text: string;
  kind: "info" | "error" | "success";
  sessionId?: string | null;
};
