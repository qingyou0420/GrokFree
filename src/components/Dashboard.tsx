import { statusLabel } from "../lib/i18n";
import type { LiveSession, Project } from "../lib/types";

function statusRank(status: string): number {
  if (status === "waiting_permission") return 0;
  if (status === "error") return 1;
  if (status === "running" || status === "starting") return 2;
  return 3;
}

export function Dashboard({
  live,
  projects,
  activeSessionId,
  onSelect,
  onNew,
  onHibernate,
  onDelete,
}: {
  live: LiveSession[];
  projects: Project[];
  activeSessionId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onHibernate: (id: string) => void;
  onDelete: (id: string, title: string) => void;
}) {
  const projectName = (id: string) =>
    projects.find((p) => p.id === id)?.name ?? "项目";

  if (live.length === 0) {
    return (
      <div className="dashboard empty">
        <div className="empty-illustration" aria-hidden>
          <span className="empty-orb" />
        </div>
        <h2>指挥中心</h2>
        <p>
          当前没有运行中的会话。选择项目后新建会话，多会话状态会按优先级汇总在此。
        </p>
        <button type="button" className="btn primary" onClick={onNew}>
          新建会话
        </button>
      </div>
    );
  }

  const sorted = [...live].sort(
    (a, b) => statusRank(a.status) - statusRank(b.status)
  );
  const running = live.filter(
    (s) => s.status === "running" || s.status === "starting"
  ).length;
  const waiting = live.filter((s) => s.status === "waiting_permission").length;
  const errors = live.filter((s) => s.status === "error").length;

  return (
    <div className="dashboard">
      <div className="dashboard-head">
        <div>
          <h2 className="dashboard-title">指挥中心</h2>
          <p className="dashboard-sub">
            待授权与错误优先展示，便于快速处理多会话注意力。
          </p>
        </div>
        <button type="button" className="btn primary sm" onClick={onNew}>
          新建会话
        </button>
      </div>

      <div className="dashboard-summary">
        <div className="dash-stat">
          <strong>{live.length}</strong>
          <span>活跃</span>
        </div>
        <div className="dash-stat">
          <strong className={running ? "accent" : ""}>{running}</strong>
          <span>运行中</span>
        </div>
        <div className="dash-stat">
          <strong className={waiting ? "warn" : ""}>{waiting}</strong>
          <span>待授权</span>
        </div>
        <div className="dash-stat">
          <strong className={errors ? "danger" : ""}>{errors}</strong>
          <span>错误</span>
        </div>
      </div>

      {waiting + errors > 0 && (
        <div className="attention-strip">
          需要关注：
          {waiting > 0 && <span className="tag warn">{waiting} 待授权</span>}
          {errors > 0 && <span className="tag danger">{errors} 错误</span>}
        </div>
      )}

      <div className="dashboard-grid">
        {sorted.map((s) => (
          <button
            key={s.id}
            type="button"
            className={`dash-card ${s.id === activeSessionId ? "active" : ""} status-${s.status}`}
            onClick={() => onSelect(s.id)}
          >
            <div className="dash-card-top">
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
              <strong>{s.title}</strong>
              <span className="dash-badge">{statusLabel(s.status)}</span>
            </div>
            <div className="dash-meta">{projectName(s.projectId)}</div>
            <div className="dash-cwd" title={s.cwd}>
              {s.cwd}
            </div>
            {s.error && <div className="dash-error">{s.error}</div>}
            <div className="dash-actions" onClick={(e) => e.stopPropagation()}>
              <button
                type="button"
                className="btn sm"
                onClick={() => onSelect(s.id)}
              >
                打开
              </button>
              <button
                type="button"
                className="btn sm ghost"
                onClick={() => onHibernate(s.id)}
              >
                休眠
              </button>
              <button
                type="button"
                className="btn sm danger"
                onClick={() => onDelete(s.id, s.title)}
              >
                删除
              </button>
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}
