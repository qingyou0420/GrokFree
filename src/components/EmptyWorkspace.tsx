import type { Project } from "../lib/types";

/** 无活跃会话时的空态引导 */
export function EmptyWorkspace(props: {
  activeProject: Project | null;
  busy: boolean;
  onCreateSession: () => void;
  onLoadDiskHistory: () => void;
  onShowDashboard: () => void;
}) {
  const { activeProject, busy, onCreateSession, onLoadDiskHistory, onShowDashboard } =
    props;
  return (
    <div className="empty-workspace">
      <div className="empty-illustration" aria-hidden>
        <span className="empty-orb" />
      </div>
      <h2>开始工作</h2>
      <p>
        {activeProject
          ? `当前项目「${activeProject.name}」。新建会话或从磁盘历史恢复对话。`
          : "请先在左侧添加并选择一个项目，然后新建会话。"}
      </p>
      <div className="empty-workspace-actions">
        <button
          type="button"
          className="btn primary"
          disabled={!activeProject || busy}
          onClick={onCreateSession}
        >
          新建会话
        </button>
        <button type="button" className="btn" onClick={onLoadDiskHistory}>
          磁盘历史
        </button>
        <button type="button" className="btn ghost" onClick={onShowDashboard}>
          打开总览
        </button>
      </div>
    </div>
  );
}
