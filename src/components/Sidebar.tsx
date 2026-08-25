/** Left rail: projects, sessions, CLI status, nav */

import { useEffect, useRef, useState } from "react";
import { OverflowMenu } from "./Dialog";
import { IconGrid, IconMore, IconPlus, IconSettings } from "./Icons";
import { statusLabel } from "../lib/i18n";
import type {
  AgentProfile,
  LiveSession,
  CloudUpdateInfo,
  Project,
  SessionMeta,
} from "../lib/types";

function relativeTime(iso?: string | null): string {
  if (!iso) return "";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "";
  const sec = Math.round((Date.now() - t) / 1000);
  if (sec < 60) return "刚刚";
  if (sec < 3600) return `${Math.floor(sec / 60)} 分钟前`;
  if (sec < 86400) return `${Math.floor(sec / 3600)} 小时前`;
  if (sec < 86400 * 7) return `${Math.floor(sec / 86400)} 天前`;
  return iso.slice(0, 10);
}

/** 会话行内联改名输入框：Enter/失焦提交，Esc 取消 */
function RenameInput({
  initial,
  onCommit,
  onCancel,
}: {
  initial: string;
  onCommit: (v: string) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState(initial);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const t = window.setTimeout(() => {
      ref.current?.focus();
      ref.current?.select();
    }, 0);
    return () => window.clearTimeout(t);
  }, []);
  const commit = () => {
    const v = value.trim();
    if (v && v !== initial) onCommit(v);
    else onCancel();
  };
  return (
    <input
      ref={ref}
      className="session-rename-input"
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onBlur={commit}
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          commit();
        } else if (e.key === "Escape") {
          e.preventDefault();
          onCancel();
        }
      }}
    />
  );
}

export type SidebarProps = {
  appVersion: string;
  cloudUpdate: CloudUpdateInfo | null;
  updateBusy: boolean;
  onLaunchUpdate: () => void;
  projects: Project[];
  activeProjectId: string | null;
  onSelectProject: (id: string) => void;
  onAddProject: () => void;
  onCreateSession: (project?: Project | null) => void;
  onRemoveProject: (id: string, name: string) => void;
  projectMenuId: string | null;
  setProjectMenuId: (id: string | null | ((cur: string | null) => string | null)) => void;
  sessionMenuId: string | null;
  setSessionMenuId: (id: string | null | ((cur: string | null) => string | null)) => void;
  setTopMenuOpen: (v: boolean | ((b: boolean) => boolean)) => void;
  sessionQuery: string;
  setSessionQuery: (q: string) => void;
  fromLive: LiveSession[];
  fromMeta: SessionMeta[];
  activeSessionId: string | null;
  busy: boolean;
  /** 可选智能体档案（新建会话用）与当前选中 */
  enabledAgents: AgentProfile[];
  selectedAgentId: string;
  onSelectAgent: (id: string) => void;
  /** 会话行的 agent 显示名（null = grok 默认，不显示徽标） */
  agentName: (id?: string | null) => string | null;
  onSelectLive: (id: string) => void;
  onResumeMeta: (meta: SessionMeta) => void;
  /** 提交内联改名（新标题已 trim 非空） */
  onCommitRename: (id: string, nextTitle: string) => void;
  onHibernate: (id: string) => void;
  onDeleteSession: (id: string, title: string) => void;
  onPurgePlaceholders: () => void;
  onLoadDiskHistory: () => void;
  onShowDashboard: () => void;
  onShowSettings: () => void;
};

export function Sidebar({
  appVersion,
  cloudUpdate,
  updateBusy,
  onLaunchUpdate,
  projects,
  activeProjectId,
  onSelectProject,
  onAddProject,
  onCreateSession,
  onRemoveProject,
  projectMenuId,
  setProjectMenuId,
  sessionMenuId,
  setSessionMenuId,
  setTopMenuOpen,
  sessionQuery,
  setSessionQuery,
  fromLive,
  fromMeta,
  activeSessionId,
  busy,
  enabledAgents,
  selectedAgentId,
  onSelectAgent,
  agentName,
  onSelectLive,
  onResumeMeta,
  onCommitRename,
  onHibernate,
  onDeleteSession,
  onPurgePlaceholders,
  onLoadDiskHistory,
  onShowDashboard,
  onShowSettings,
}: SidebarProps) {
  const [renaming, setRenaming] = useState<{ id: string; title: string } | null>(
    null
  );
  const [agentMenuOpen, setAgentMenuOpen] = useState(false);
  const [maintainOpen, setMaintainOpen] = useState(false);
  const sessionListRef = useRef<HTMLDivElement>(null);
  const selectedAgentName =
    enabledAgents.find((a) => a.id === selectedAgentId)?.name ?? null;

  // 会话列表 ↑↓ 键在会话行之间移动焦点（键盘主路径）
  const onSessionListKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const target = e.target as HTMLElement;
    if (!target.classList.contains("session-item-body")) return;
    e.preventDefault();
    const bodies = Array.from(
      sessionListRef.current?.querySelectorAll<HTMLButtonElement>(
        ".session-item-body:not([disabled])"
      ) ?? []
    );
    const idx = bodies.indexOf(target as HTMLButtonElement);
    if (idx < 0) return;
    const next =
      e.key === "ArrowDown"
        ? Math.min(bodies.length - 1, idx + 1)
        : Math.max(0, idx - 1);
    bodies[next]?.focus();
  };

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <div className="logo-mark">G</div>
        <div className="logo-text">
          <strong>GrokFree</strong>
          <span className="logo-version-row">
            <span>
              Desktop · v{cloudUpdate?.currentVersion || appVersion}
            </span>
            {cloudUpdate?.isNewer && (
              <button
                type="button"
                className="version-update-btn"
                disabled={updateBusy}
                title={`发现新版本 v${cloudUpdate.version} — 点击一键更新`}
                onClick={() => void onLaunchUpdate()}
              >
                {updateBusy ? "…" : "更新"}
              </button>
            )}
          </span>
        </div>
      </div>

      <div className="sidebar-section">
        <div className="section-label">
          项目
          <button
            type="button"
            className="icon-btn"
            title="添加项目"
            onClick={onAddProject}
          >
            <IconPlus />
          </button>
        </div>
        <div className="project-list" style={{ maxHeight: 160 }}>
          {projects.length === 0 && (
            <div className="list-empty">暂无项目，请添加文件夹</div>
          )}
          {projects.map((p) => (
            <div
              key={p.id}
              className={`project-item ${
                p.id === activeProjectId ? "active" : ""
              }`}
            >
              <button
                type="button"
                className="project-item-body"
                onClick={() => onSelectProject(p.id)}
                onDoubleClick={() => void onCreateSession(p)}
                title="双击新建会话"
              >
                <span className="name">{p.name}</span>
                <span className="path">{p.cwd}</span>
              </button>
              <div className="menu-shell">
                <button
                  type="button"
                  className="icon-btn more"
                  title="项目操作"
                  onClick={(e) => {
                    e.stopPropagation();
                    setProjectMenuId((cur) => (cur === p.id ? null : p.id));
                    setSessionMenuId(null);
                    setTopMenuOpen(false);
                  }}
                >
                  <IconMore />
                </button>
                <OverflowMenu
                  open={projectMenuId === p.id}
                  onClose={() => setProjectMenuId(null)}
                  align="right"
                  items={[
                    {
                      id: "new",
                      label: "新建会话",
                      onSelect: () => void onCreateSession(p),
                    },
                    {
                      id: "remove",
                      label: "移除项目",
                      danger: true,
                      onSelect: () => onRemoveProject(p.id, p.name),
                    },
                  ]}
                />
              </div>
            </div>
          ))}
        </div>
      </div>

      <div
        className="sidebar-section"
        style={{
          flex: 1,
          minHeight: 0,
          display: "flex",
          flexDirection: "column",
        }}
      >
        <div className="section-label">
          会话
          <div className="menu-shell">
            <button
              type="button"
              className="icon-btn"
              title="会话维护"
              onClick={(e) => {
                e.stopPropagation();
                setMaintainOpen((v) => !v);
              }}
            >
              <IconMore size={14} />
            </button>
            <OverflowMenu
              open={maintainOpen}
              onClose={() => setMaintainOpen(false)}
              align="right"
              items={[
                {
                  id: "purge",
                  label: "清理空/占位会话",
                  onSelect: () => void onPurgePlaceholders(),
                },
              ]}
            />
          </div>
        </div>
        <input
          className="session-search"
          placeholder="搜索会话…"
          value={sessionQuery}
          onChange={(e) => setSessionQuery(e.target.value)}
        />
        <div className="new-session-split">
          <button
            type="button"
            className="btn primary split-main"
            title="在当前项目新建会话 (Ctrl+N)"
            disabled={!projects.find((p) => p.id === activeProjectId) || busy}
            onClick={() => void onCreateSession()}
          >
            <IconPlus size={13} /> 新建
            {selectedAgentName ? ` · ${selectedAgentName}` : ""}
          </button>
          <div className="menu-shell">
            <button
              type="button"
              className="btn primary split-drop"
              title="选择小精灵（设置 → 小精灵 中管理）"
              disabled={enabledAgents.length === 0}
              onClick={(e) => {
                e.stopPropagation();
                setAgentMenuOpen((v) => !v);
              }}
            >
              ▾
            </button>
            <OverflowMenu
              open={agentMenuOpen}
              onClose={() => setAgentMenuOpen(false)}
              align="right"
              items={enabledAgents.map((a) => ({
                id: a.id,
                label: `${a.id === selectedAgentId ? "✓ " : ""}${a.name}${
                  a.defaultModel ? ` · ${a.defaultModel}` : ""
                }`,
                onSelect: () => onSelectAgent(a.id),
              }))}
            />
          </div>
        </div>
        <div
          className="session-list"
          style={{ flex: 1 }}
          ref={sessionListRef}
          onKeyDown={onSessionListKeyDown}
        >
          {fromLive.length === 0 && fromMeta.length === 0 && (
            <div className="list-empty">
              暂无会话。选中项目后点「新建会话」
            </div>
          )}
          {fromLive.length > 0 && (
            <div className="session-group-label">运行中</div>
          )}
          {fromLive.map((s) => (
            <div
              key={s.id}
              className={`session-item ${
                s.id === activeSessionId ? "active" : ""
              }`}
            >
              {renaming?.id === s.id ? (
                <div className="session-item-body session-item-editing">
                  <RenameInput
                    initial={renaming.title}
                    onCommit={(v) => {
                      setRenaming(null);
                      onCommitRename(s.id, v);
                    }}
                    onCancel={() => setRenaming(null)}
                  />
                </div>
              ) : (
                <button
                  type="button"
                  className="session-item-body"
                  onClick={() => onSelectLive(s.id)}
                  onDoubleClick={() => setRenaming({ id: s.id, title: s.title })}
                  title="双击重命名 · Ctrl+1..9 快速切换 · ↑↓ 切换会话"
                >
                  <span className="name">
                    <span
                      className={`status-dot ${
                        s.status === "running" || s.status === "starting"
                          ? "running"
                          : s.status === "error"
                            ? "error"
                            : s.status === "waiting_permission"
                              ? "needs-input"
                              : ""
                      }`}
                    />
                    {s.title}
                  </span>
                  <span className="meta">
                    {agentName(s.agentId) ? `${agentName(s.agentId)} · ` : ""}
                    {statusLabel(s.status)}
                  </span>
                </button>
              )}
              <div className="menu-shell">
                <button
                  type="button"
                  className="session-item-del"
                  title="更多"
                  onClick={(e) => {
                    e.stopPropagation();
                    setSessionMenuId((cur) => (cur === s.id ? null : s.id));
                    setProjectMenuId(null);
                    setTopMenuOpen(false);
                  }}
                >
                  <IconMore size={14} />
                </button>
                <OverflowMenu
                  open={sessionMenuId === s.id}
                  onClose={() => setSessionMenuId(null)}
                  align="right"
                  items={[
                    {
                      id: "rename",
                      label: "重命名",
                      onSelect: () => setRenaming({ id: s.id, title: s.title }),
                    },
                    {
                      id: "hibernate",
                      label: "休眠",
                      onSelect: () => void onHibernate(s.id),
                    },
                    {
                      id: "delete",
                      label: "删除",
                      danger: true,
                      onSelect: () => void onDeleteSession(s.id, s.title),
                    },
                  ]}
                />
              </div>
            </div>
          ))}
          {fromMeta.length > 0 && (
            <div className="session-group-label">休眠</div>
          )}
          {fromMeta.map((s) => (
            <div key={s.id} className="session-item">
              {renaming?.id === s.id ? (
                <div className="session-item-body session-item-editing">
                  <RenameInput
                    initial={renaming.title}
                    onCommit={(v) => {
                      setRenaming(null);
                      onCommitRename(s.id, v);
                    }}
                    onCancel={() => setRenaming(null)}
                  />
                </div>
              ) : (
                <button
                  type="button"
                  className="session-item-body"
                  disabled={busy}
                  onClick={() => void onResumeMeta(s)}
                  onDoubleClick={() =>
                    setRenaming({ id: s.id, title: s.title })
                  }
                  title={busy ? "正在恢复…" : "点击恢复 · 双击重命名"}
                >
                  <span className="name">
                    <span className="status-dot" />
                    {s.title}
                  </span>
                  <span className="meta">
                    {agentName(s.agentId) ? `${agentName(s.agentId)} · ` : ""}
                    休眠
                    {s.lastActiveAt ? ` · ${relativeTime(s.lastActiveAt)}` : ""}
                  </span>
                </button>
              )}
              <div className="menu-shell">
                <button
                  type="button"
                  className="session-item-del"
                  title="更多"
                  onClick={(e) => {
                    e.stopPropagation();
                    setSessionMenuId((cur) => (cur === s.id ? null : s.id));
                    setProjectMenuId(null);
                    setTopMenuOpen(false);
                  }}
                >
                  <IconMore size={14} />
                </button>
                <OverflowMenu
                  open={sessionMenuId === s.id}
                  onClose={() => setSessionMenuId(null)}
                  align="right"
                  items={[
                    {
                      id: "resume",
                      label: "恢复",
                      onSelect: () => void onResumeMeta(s),
                    },
                    {
                      id: "rename",
                      label: "重命名",
                      onSelect: () => setRenaming({ id: s.id, title: s.title }),
                    },
                    {
                      id: "delete",
                      label: "删除",
                      danger: true,
                      onSelect: () => void onDeleteSession(s.id, s.title),
                    },
                  ]}
                />
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="sidebar-footer">
        <div className="sidebar-footer-actions">
          <button
            type="button"
            className="btn"
            onClick={onShowDashboard}
            title="指挥中心"
          >
            <IconGrid size={14} />
            总览
          </button>
          <button
            type="button"
            className="btn"
            onClick={onShowSettings}
          >
            <IconSettings size={14} />
            设置
          </button>
        </div>
      </div>
    </aside>
  );
}
