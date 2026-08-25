import type { RefObject } from "react";
import { OverflowMenu } from "./Dialog";
import { IconMore, IconReview } from "./Icons";
import { permissionModeLabel, sandboxModeLabel } from "../lib/i18n";
import type {
  DesktopPrefs,
  LiveSession,
  Project,
} from "../lib/types";

/** 顶栏：标题 / 权限胶囊 / 审查开关 / 新建会话 / 更多菜单 + 会话出错横条 */
export function Topbar(props: {
  showDashboard: boolean;
  liveCount: number;
  activeLive: LiveSession | null;
  activeProject: Project | null;
  cwd: string;
  prefs: DesktopPrefs;
  /** 当前会话的智能体显示名 */
  agentLabel?: string | null;
  /** 新建按钮显示的智能体名（跟侧栏选中档案） */
  createAgentLabel?: string | null;
  pendingDiffCount: number;
  showReview: boolean;
  busy: boolean;
  topMenuOpen: boolean;
  setTopMenuOpen: (v: boolean | ((b: boolean) => boolean)) => void;
  setProjectMenuId: (id: string | null) => void;
  setSessionMenuId: (id: string | null) => void;
  onToggleReview: () => void;
  onCreateSession: () => void;
  onLoadDiskHistory: () => void;
  onOpenSettings: () => void;
  onOpenTerminal: (cwd: string) => void;
  onOpenEditor: (cwd: string) => void;
  onRestartSession: () => void;
  onRevealLogs: () => void;
  onExportDiagnostics: () => void;
}) {
  const {
    showDashboard,
    liveCount,
    activeLive,
    activeProject,
    cwd,
    prefs,
    agentLabel,
    createAgentLabel,
    pendingDiffCount,
    showReview,
    busy,
    topMenuOpen,
    setTopMenuOpen,
    setProjectMenuId,
    setSessionMenuId,
    onToggleReview,
    onCreateSession,
    onLoadDiskHistory,
    onOpenSettings,
    onOpenTerminal,
    onOpenEditor,
    onRestartSession,
    onRevealLogs,
    onExportDiagnostics,
  } = props;
  return (
    <>
      <div className="topbar">
        <div style={{ minWidth: 0 }}>
          <h1>
            {showDashboard
              ? "指挥中心"
              : activeLive?.title ||
                activeProject?.name ||
                "GrokFree"}
          </h1>
          <div className="cwd" title={cwd}>
            {showDashboard
              ? `${liveCount} 个活跃会话`
              : cwd || "请选择项目，然后启动会话"}
          </div>
        </div>
        <div className="topbar-actions">
          {agentLabel && (
            <span className="mode-pill agent-pill" title="当前小精灵">
              {agentLabel}
            </span>
          )}
          <span
            className="mode-pill"
            title={
              prefs.sandboxMode !== "off"
                ? `沙箱偏好（仅说明）：${sandboxModeLabel(prefs.sandboxMode)}`
                : "权限模式"
            }
          >
            {permissionModeLabel(prefs.permissionMode)}
          </span>
          <button
            type="button"
            className={`btn sm icon-label ${showReview ? "primary" : ""}`}
            onClick={onToggleReview}
            title="Ctrl+B"
          >
            <IconReview size={14} />
            审查
            {pendingDiffCount > 0 && (
              <span className="badge-count">{pendingDiffCount}</span>
            )}
          </button>
          <button
            type="button"
            className="btn sm primary"
            disabled={!activeProject || busy}
            onClick={onCreateSession}
          >
            {busy ? "启动中…" : `新建${createAgentLabel ? ` · ${createAgentLabel}` : ""}`}
          </button>
          <div className="menu-shell">
            <button
              type="button"
              className="btn sm"
              title="更多操作"
              onClick={() => {
                setTopMenuOpen((v) => !v);
                setProjectMenuId(null);
                setSessionMenuId(null);
              }}
            >
              <IconMore />
            </button>
            <OverflowMenu
              open={topMenuOpen}
              onClose={() => setTopMenuOpen(false)}
              align="right"
              items={[
                ...(cwd
                  ? [
                      {
                        id: "terminal",
                        label: "外部终端",
                        onSelect: () => onOpenTerminal(cwd),
                      },
                      {
                        id: "editor",
                        label: "打开编辑器",
                        onSelect: () => onOpenEditor(cwd),
                      },
                    ]
                  : []),
                /* 会话操作（重命名/休眠/删除）唯一入口在侧栏会话菜单，顶栏不再重复 */
                {
                  id: "history",
                  label: "磁盘历史",
                  onSelect: onLoadDiskHistory,
                },
                {
                  id: "settings",
                  label: "设置",
                  onSelect: onOpenSettings,
                },
              ]}
            />
          </div>
        </div>
      </div>

      {activeLive?.status === "error" && (
        <div className="error-bar">
          <span className="msg">
            会话出错：{activeLive.error || "未知错误"}
          </span>
          <button
            type="button"
            className="btn sm"
            disabled={busy}
            onClick={onRestartSession}
          >
            重启会话
          </button>
          <button type="button" className="btn sm" onClick={onRevealLogs}>
            查看日志
          </button>
          <button type="button" className="btn sm" onClick={onExportDiagnostics}>
            导出诊断
          </button>
        </div>
      )}
    </>
  );
}
