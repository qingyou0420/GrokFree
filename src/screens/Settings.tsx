import {
  memo,
  useEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import { api } from "../lib/api";
import { AgentsTab } from "./AgentsTab";
import type {
  AgentProfile,
  CloudUpdateInfo,
  DesktopPrefs,
  GrokEnvironment,
  SkillsMcpSnapshot,
  UpdateProgress,
} from "../lib/types";

export type SettingsModalProps = {
  prefs: DesktopPrefs;
  env: GrokEnvironment | null;
  onClose: () => void;
  onSave: (p: DesktopPrefs) => Promise<void>;
  onOpenConfig: () => void;
  onRevealLogs: () => void;
  onExportDiagnostics?: () => Promise<void>;
  cloudUpdate?: CloudUpdateInfo | null;
  updateProgress?: UpdateProgress | null;
  onRefreshCloudUpdate?: () => void | Promise<void>;
  onLaunchCloudUpdate?: () => void | Promise<void>;
  updateBusy?: boolean;
  onFlash?: (text: string, kind?: "info" | "error" | "success") => void;
  /** 智能体档案（agents.json）与保存回调 */
  agents?: AgentProfile[];
  onSaveAgents?: (profiles: AgentProfile[]) => Promise<unknown>;
};

/**
 * Portal + memo: isolate from App high-frequency re-renders (stream/live),
 * which otherwise close native <select> menus mid-click.
 */
export const SettingsModal = memo(function SettingsModal({
  prefs,
  env,
  onClose,
  onSave,
  onOpenConfig,
  onRevealLogs,
  onExportDiagnostics,
  cloudUpdate = null,
  updateProgress = null,
  onRefreshCloudUpdate,
  onLaunchCloudUpdate,
  updateBusy = false,
  onFlash,
  agents = [],
  onSaveAgents,
}: SettingsModalProps) {
  const [draft, setDraft] = useState(prefs);
  const [saving, setSaving] = useState(false);
  const [skillsMcp, setSkillsMcp] = useState<SkillsMcpSnapshot | null>(null);
  const [appInfo, setAppInfo] = useState<Record<string, string> | null>(null);
  const [tab, setTab] = useState<"agents" | "appearance" | "about">("agents");
  const [installersBusy, setInstallersBusy] = useState(false);

  const caps = env?.capabilities;

  const onRefreshCloudUpdateRef = useRef(onRefreshCloudUpdate);
  onRefreshCloudUpdateRef.current = onRefreshCloudUpdate;

  useEffect(() => {
    document.body.classList.add("modal-open");
    return () => document.body.classList.remove("modal-open");
  }, []);

  useEffect(() => {
    void api.listSkillsMcp().then(setSkillsMcp).catch(() => setSkillsMcp(null));
    void api.appInfo().then(setAppInfo).catch(() => setAppInfo(null));
  }, []);

  useEffect(() => {
    if (tab === "about") void onRefreshCloudUpdateRef.current?.();
  }, [tab]);

  const save = async () => {
    // always-approve requires explicit confirm
    if (
      draft.permissionMode === "always-approve" &&
      prefs.permissionMode !== "always-approve"
    ) {
      const ok = window.confirm(
        "「始终允许」会跳过所有权限确认，小精灵可在工作区内自由执行命令与改文件。\n\n确定要开启吗？"
      );
      if (!ok) return;
    }
    setSaving(true);
    try {
      await onSave(draft);
      onClose();
    } finally {
      setSaving(false);
    }
  };

  const modal = (
    <div
      className="modal-backdrop settings-backdrop"
      onClick={onClose}
      role="presentation"
    >
      <div
        className="modal wide"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="设置"
      >
        <header>
          <span>设置</span>
          <button className="icon-btn" onClick={onClose} title="关闭">
            ✕
          </button>
        </header>

        <div className="settings-tabs">
          <button
            className={tab === "agents" ? "active" : ""}
            onClick={() => setTab("agents")}
          >
            小精灵
          </button>
          <button
            className={tab === "appearance" ? "active" : ""}
            onClick={() => setTab("appearance")}
          >
            外观
          </button>
          <button
            className={tab === "about" ? "active" : ""}
            onClick={() => setTab("about")}
          >
            关于
          </button>
        </div>

        <div className="body">
          {tab === "agents" && (
            <>
              {onSaveAgents && (
                <AgentsTab agents={agents} onSaveAgents={onSaveAgents} onFlash={onFlash} />
              )}

              <div className="settings-section-label">会话行为</div>
              <div className="field">
                <label>权限模式</label>
                <select
                  value={draft.permissionMode}
                  onChange={(e) =>
                    setDraft({ ...draft, permissionMode: e.target.value })
                  }
                >
                  <option value="ask">每次询问（默认，推荐）</option>
                  <option value="auto">自动</option>
                  <option value="always-approve">始终允许（危险）</option>
                </select>
                <div className="help">对新启动的进程生效。</div>
              </div>

              <div className="field">
              <label>
                  沙箱模式（CLI）
                  {!caps?.agentSandboxFlag && (
                    <span className="cap-badge off"> agent 不支持此标志</span>
                  )}
                </label>
                <select
                  value={draft.sandboxMode}
                  disabled={!caps?.agentSandboxFlag}
                  onChange={(e) =>
                    setDraft({ ...draft, sandboxMode: e.target.value })
                  }
                  title={
                    caps?.agentSandboxFlag
                      ? undefined
                      : "当前 grok agent 子命令不接受 --sandbox"
                  }
                >
                  <option value="off">关闭</option>
                  <option value="workspace">工作区</option>
                  <option value="read-only">只读</option>
                  <option value="strict">严格</option>
                </select>
                <div className="help">
                  {caps?.notes?.[0] ||
                    "沙箱请在 CLI 全局配置中设置。Desktop 不会向 agent 传递无效参数。"}
                </div>
              </div>

              <div className="field">
              <label>Agent 文件范围</label>
                <select
                  value={draft.fsScope || "workspace"}
                  onChange={(e) =>
                    setDraft({ ...draft, fsScope: e.target.value })
                  }
                >
                  <option value="workspace">仅项目工作区（默认）</option>
                  <option value="unrestricted">不限制（全盘，慎用）</option>
                </select>
                <div className="help">
                  限制 ACP 读写是否必须落在当前会话 cwd 下。仅个人自用需要写项目外路径时再放开。
                </div>
              </div>

              <div className="field">
              <label>Shell 偏好（Agent 工具 + 外部终端）</label>
                <select
                  value={draft.defaultShell}
                  onChange={(e) =>
                    setDraft({ ...draft, defaultShell: e.target.value })
                  }
                >
                  <option value="powershell">
                    PowerShell（推荐，与 Agent 一致）
                  </option>
                  <option value="pwsh">PowerShell 7 (pwsh)</option>
                  <option value="cmd">命令提示符 (cmd)</option>
                  <option value="gitbash">Git Bash（外部终端）</option>
                  <option value="wsl">WSL（外部终端）</option>
                </select>
                <div className="help">
                  Agent 的终端命令经此解释器执行。默认 PowerShell，避免 cmd
                  无法识别 <code>Set-Location</code> 等语法。
                </div>
              </div>

              
              <div className="settings-section-label">Grok CLI</div>
{env && !env.cliVersionOk && env.grokExists && (
                <div className="settings-banner warn">
                  CLI 版本可能过旧（当前 {env.grokVersion || "未知"}，需要 ≥{" "}
                  {env.minCliVersion}）。请升级后再启动新会话。
                </div>
              )}
              {env && !env.grokExists && (
                <div className="settings-banner error">
                  未检测到 Grok CLI。安装：
                  <code>irm https://x.ai/cli/install.ps1 | iex</code>
                </div>
              )}

              <div className="field">
              <label>Grok CLI 路径</label>
                <input
                  value={draft.grokPath}
                  placeholder={
                    env?.grokPath || "%USERPROFILE%\\.grok\\bin\\grok.exe"
                  }
                  onChange={(e) =>
                    setDraft({ ...draft, grokPath: e.target.value })
                  }
                />
                <div className="help">
                  检测到：{env?.grokVersion || "未找到"} · 主目录 {env?.grokHome}
                  {env?.cliVersionOk === false && env.grokExists
                    ? " · ⚠ 版本门禁未通过"
                    : env?.grokExists
                      ? " · ✓ 版本可用"
                      : ""}
                </div>
              </div>

              <div className="field">
              <label>最低 CLI 版本（门禁）</label>
                <input
                  value={draft.minCliVersion}
                  placeholder="0.2.0"
                  onChange={(e) =>
                    setDraft({ ...draft, minCliVersion: e.target.value })
                  }
                />

              </div>

              
              <div className="settings-section-label">Skills / MCP（只读，由 ~/.grok 加载）</div>
              <div className="field">
                <label>
                  Skills（{skillsMcp?.skills.length ?? 0}）
                </label>
                {!skillsMcp ? (
                  <div className="help">加载中…</div>
                ) : skillsMcp.skills.length === 0 ? (
                  <div className="help">未发现 Skills</div>
                ) : (
                  <ul className="cap-list">
                    {skillsMcp.skills.map((s) => (
                      <li key={`${s.source}-${s.name}`}>
                        <strong>{s.name}</strong>
                        <span className="meta">
                          {s.source} · {s.path}
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
              <div className="field">
              <label>
                  MCP 服务器（{skillsMcp?.mcpServers.length ?? 0}）
                </label>
                {!skillsMcp ? (
                  <div className="help">加载中…</div>
                ) : skillsMcp.mcpServers.length === 0 ? (
                  <div className="help">未在 config.toml 中发现 MCP 配置</div>
                ) : (
                  <ul className="cap-list">
                    {skillsMcp.mcpServers.map((m) => (
                      <li key={m.name}>
                        <strong>{m.name}</strong>
                        <span className="meta">{m.detail || "—"}</span>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
              {skillsMcp?.notes?.map((n, i) => (
                <div key={i} className="help">
                  {n}
                </div>
              ))}
              
<button className="btn" onClick={onOpenConfig}>
                在配置中编辑
              </button>
            
            </>
          )}

          {tab === "appearance" && (
            <>
              <div className="field">
                <label>主题</label>
                <select
                  value={draft.theme || "light"}
                  onChange={(e) => setDraft({ ...draft, theme: e.target.value })}
                >
                  <option value="light">浅色（默认）</option>
                  <option value="dark">深色</option>
                </select>
              </div>

              <div className="field">
              <label>历史首屏条数</label>
                <select
                  value={String(draft.historyInitialVisible ?? 50)}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      historyInitialVisible: Number(e.target.value),
                    })
                  }
                >
                  <option value="30">30（更快）</option>
                  <option value="50">50（默认）</option>
                  <option value="100">100（更全）</option>
                </select>
                <div className="help">
                  打开/恢复长会话时先只渲染最近 N 条，可点「加载更早」展开。改后对新恢复的会话生效。
                </div>
              </div>

              <div className="field">
              <label>会话切换遮罩</label>
                <select
                  value={draft.chatMaskQuiet ? "quiet" : "full"}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      chatMaskQuiet: e.target.value === "quiet",
                    })
                  }
                >
                  <option value="full">显示「整理对话…」（默认）</option>
                  <option value="quiet">静默遮罩（仅挡闪烁，无文字）</option>
                </select>

              </div>

              <div className="field">
              <label>未知 ACP 事件</label>
                <select
                  value={draft.showRawAcpEvents ? "show" : "hide"}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      showRawAcpEvents: e.target.value === "show",
                    })
                  }
                >
                  <option value="hide">隐藏（默认，避免刷屏）</option>
                  <option value="show">显示折叠块（调试）</option>
                </select>

              </div>

              <div className="field">
              <label>默认项目文件夹</label>
                <input
                  value={draft.defaultProjectsDir ?? "D:\\Grok Build"}
                  placeholder="D:\Grok Build"
                  onChange={(e) =>
                    setDraft({ ...draft, defaultProjectsDir: e.target.value })
                  }
                />

              </div>

              <div className="field">
              <label>默认编辑器</label>
                <select
                  value={draft.defaultEditor}
                  onChange={(e) =>
                    setDraft({ ...draft, defaultEditor: e.target.value })
                  }
                >
                  <option value="code">VS Code</option>
                  <option value="cursor">Cursor</option>
                  <option value="notepad">记事本</option>
                </select>
              </div>

              <div className="field">
              <label>快捷键</label>
                <div className="help">
                  Ctrl+N 新建 · Ctrl+B 审查 · Ctrl+D 总览 · Ctrl+, 设置 ·
                  Ctrl+1..9 切换会话 · Enter 发送 · 点击 Toast 跳转会话
                </div>
              </div>
            
            </>
          )}

          {tab === "about" && (
            <>
              <div className="about-grid">
                <div>
                  <label>产品</label>
                  <div>GrokFree</div>
                </div>
                <div>
                  <label>Desktop 版本</label>
                  <div>{appInfo?.version || "0.9.6"}</div>
                </div>
                <div>
                  <label>CLI 版本</label>
                  <div>
                    {env?.grokVersion || "未检测到"}
                    {env?.cliVersionOk === false && env.grokExists
                      ? " ⚠"
                      : env?.grokExists
                        ? " ✓"
                        : ""}
                  </div>
                </div>
                <div>
                  <label>最低 CLI</label>
                  <div>{env?.minCliVersion || draft.minCliVersion}</div>
                </div>
                <div>
                  <label>标识符</label>
                  <div className="mono-sm">
                    {appInfo?.identifier || "app.grokfree.desktop"}
                  </div>
                </div>
                <div>
                  <label>数据目录</label>
                  <div className="mono-sm">
                    {env?.desktopDataDir || appInfo?.desktopDataDir || "—"}
                  </div>
                </div>
                <div>
                  <label>Grok 主目录</label>
                  <div className="mono-sm">
                    {env?.grokHome || appInfo?.grokHome || "—"}
                  </div>
                </div>
                <div>
                  <label>安装目录</label>
                  <div className="mono-sm">{appInfo?.installDir || "—"}</div>
                </div>
                <div>
                  <label>可执行文件</label>
                  <div className="mono-sm">
                    {appInfo?.executablePath || "—"}
                  </div>
                </div>
                <div>
                  <label>安装包目录</label>
                  <div className="mono-sm">
                    {appInfo?.installersDir || "—"}
                  </div>
                </div>
              </div>
              <div className="row" style={{ marginBottom: 12, gap: 8 }}>
                <button
                  type="button"
                  className="btn"
                  disabled={installersBusy}
                  onClick={() => {
                    void (async () => {
                      setInstallersBusy(true);
                      try {
                        const dir = await api.openInstallersDir();
                        onFlash?.(`已打开：${dir}`, "success");
                      } catch (e) {
                        onFlash?.(`打开失败：${e}`, "error");
                      } finally {
                        setInstallersBusy(false);
                      }
                    })();
                  }}
                >
                  {installersBusy ? "打开中…" : "打开安装包目录"}
                </button>
                {(appInfo?.desktopDataDir || env?.desktopDataDir) && (
                  <button
                    type="button"
                    className="btn"
                    onClick={() => {
                      const dir =
                        appInfo?.desktopDataDir || env?.desktopDataDir || "";
                      if (dir) void api.openPath(dir);
                    }}
                  >
                    打开数据目录
                  </button>
                )}
                <button className="btn" onClick={onOpenConfig}>
                  config.toml
                </button>
                <button className="btn" onClick={onRevealLogs}>
                  日志目录
                </button>
              </div>
              {onExportDiagnostics && (
                <div className="row" style={{ marginBottom: 12 }}>
                  <button
                    className="btn"
                    onClick={() => void onExportDiagnostics()}
                  >
                    导出诊断包（不含密钥）
                  </button>
                </div>
              )}
              <details className="field">
                <summary style={{ cursor: "pointer" }}>诊断信息（路径 / 标识符 / 能力标志）</summary>
                <ul className="cap-flags">
                  <li>
                    Agent 沙箱标志：{" "}
                    {caps?.agentSandboxFlag ? "支持" : "不支持（诚实）"}
                  </li>
                  <li>
                    权限模式：{caps?.permissionModes ? "支持" : "不支持"}
                  </li>
                  <li>
                    模型覆盖：{caps?.modelOverride ? "支持" : "不支持"}
                  </li>
                  <li>
                    终端宿主：{caps?.terminalHost ? "已实现" : "未实现"}
                  </li>
                  <li>
                    会话恢复：{caps?.sessionResume ? "支持" : "不支持"}
                  </li>
                </ul>
              </details>
              <div className="field" style={{ marginTop: 12 }}>
                <label>云端更新（GitHub Releases）</label>

                {cloudUpdate ? (
                  <div className="local-update-card">
                    <div>
                      <strong>
                        {cloudUpdate.isNewer
                          ? "发现新版本"
                          : "已是最新"}{" "}
                        v{cloudUpdate.version}
                      </strong>
                      <div className="meta mono-sm" title={cloudUpdate.htmlUrl}>
                        {cloudUpdate.fileName}
                      </div>
                      <div className="help">
                        当前运行：v{cloudUpdate.currentVersion}
                        {cloudUpdate.isNewer
                          ? " · 可一键从 GitHub 下载并安装"
                          : " · 无需更新"}
                        {cloudUpdate.publishedAt
                          ? ` · 发布于 ${cloudUpdate.publishedAt.slice(0, 19).replace("T", " ")}`
                          : ""}
                      </div>
                      {cloudUpdate.notes && (
                        <div className="help update-notes">
                          {cloudUpdate.notes.slice(0, 280)}
                          {cloudUpdate.notes.length > 280 ? "…" : ""}
                        </div>
                      )}
                      {updateBusy && updateProgress && (
                        <div className="update-progress" aria-hidden>
                          <span
                            style={{
                              width: `${
                                updateProgress.phase === "download"
                                  ? updateProgress.percent
                                  : updateProgress.phase === "launch"
                                    ? 100
                                    : 8
                              }%`,
                            }}
                          />
                        </div>
                      )}
                    </div>
                    <div className="local-update-actions">
                      {cloudUpdate.htmlUrl && (
                        <button
                          className="btn"
                          type="button"
                          onClick={() => void shellOpen(cloudUpdate.htmlUrl)}
                        >
                          打开发布页
                        </button>
                      )}
                      <button
                        className="btn primary"
                        type="button"
                        disabled={
                          updateBusy ||
                          !cloudUpdate.isNewer ||
                          !onLaunchCloudUpdate
                        }
                        onClick={() => void onLaunchCloudUpdate?.()}
                      >
                        {updateBusy
                          ? updateProgress?.phase === "download"
                            ? `下载中 ${updateProgress.percent}%`
                            : "启动安装…"
                          : `一键更新 v${cloudUpdate.version}`}
                      </button>
                    </div>
                  </div>
                ) : (
                  <div className="help">
                    尚未查到云端版本。发布后会出现在{" "}
                    <code>github.com/qingyou0420/GrokFree/releases</code>
                    。侧栏仅在有<strong>更高版本</strong>时显示「更新」。
                  </div>
                )}
                <button
                  className="btn"
                  type="button"
                  style={{ marginTop: 8 }}
                  disabled={updateBusy}
                  onClick={() => void onRefreshCloudUpdate?.()}
                >
                  {updateBusy ? "检查中…" : "检查更新"}
                </button>
              </div>
              <div className="help">v0.9.6 · 一键云端更新</div>
            </>
          )}
        </div>
        <div className="footer">
          <button className="btn ghost" onClick={onClose}>
            取消
          </button>
          {(tab === "agents" || tab === "appearance") && (
            <button className="btn primary" disabled={saving} onClick={save}>
              {saving ? "保存中…" : "保存偏好"}
            </button>
          )}
          {tab === "about" && (
            <button className="btn primary" onClick={onClose}>
              关闭
            </button>
          )}
        </div>
      </div>
    </div>
  );

  return createPortal(modal, document.body);
});
