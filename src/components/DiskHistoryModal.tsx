import { useState } from "react";
import type { DiskSession } from "../lib/types";

/** 磁盘会话历史浏览 / 搜索 / 恢复 / 永久删除（~/.grok/sessions） */
export function DiskHistoryModal(props: {
  sessions: DiskSession[];
  filterByProject: boolean;
  onFilterByProjectChange: (v: boolean) => void;
  canFilterProject: boolean;
  busy: boolean;
  onResume: (d: DiskSession) => void;
  onDelete: (d: DiskSession) => void;
  onClose: () => void;
  onRefresh: (filterByProject: boolean) => void;
}) {
  const {
    sessions,
    filterByProject,
    onFilterByProjectChange,
    canFilterProject,
    busy,
    onResume,
    onDelete,
    onClose,
    onRefresh,
  } = props;
  const [query, setQuery] = useState("");

  const q = query.trim().toLowerCase();
  const filtered = sessions.filter(
    (d) =>
      !q ||
      d.title.toLowerCase().includes(q) ||
      d.id.toLowerCase().includes(q) ||
      (d.cwd || "").toLowerCase().includes(q)
  );

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal wide" onClick={(e) => e.stopPropagation()}>
        <header>
          <span>磁盘会话历史 · ~/.grok/sessions</span>
          <button className="icon-btn" onClick={onClose}>
            ✕
          </button>
        </header>
        <div className="body">
          <div className="disk-history-toolbar">
            <label className="disk-filter-check">
              <input
                type="checkbox"
                checked={filterByProject}
                onChange={(e) => {
                  const v = e.target.checked;
                  onFilterByProjectChange(v);
                  onRefresh(v);
                }}
                disabled={!canFilterProject}
              />
              仅当前项目
            </label>
            <input
              className="session-search"
              placeholder="搜索标题 / 路径…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
          <div className="help" style={{ marginBottom: 8 }}>
            空会话（仅 session/new、无真实对话）已自动隐藏。删除为永久操作。
          </div>
          {filtered.length === 0 ? (
            <div className="help">
              {sessions.length === 0
                ? "未找到有效会话。使用 CLI 或 Desktop 创建会话后会出现在此。"
                : "没有匹配搜索条件的会话。"}
            </div>
          ) : (
            <div className="disk-session-list">
              {filtered.map((d) => (
                <div key={`${d.id}-${d.path}`} className="disk-session-row">
                  <button
                    type="button"
                    className="disk-session-item"
                    disabled={busy}
                    onClick={() => onResume(d)}
                  >
                    <strong>{d.title}</strong>
                    <span className="meta">
                      {d.id.slice(0, 12)}
                      {d.cwd ? ` · ${d.cwd}` : ""}
                      {d.updatedAt ? ` · ${d.updatedAt.slice(0, 19)}` : ""}
                      {d.messageCount != null ? ` · ${d.messageCount} 条` : ""}
                    </span>
                  </button>
                  <button
                    type="button"
                    className="btn sm danger"
                    title="从磁盘永久删除"
                    disabled={busy}
                    onClick={() => onDelete(d)}
                  >
                    删除
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
        <div className="footer">
          <button className="btn ghost" onClick={onClose}>
            关闭
          </button>
          <button className="btn" onClick={() => onRefresh(filterByProject)}>
            刷新
          </button>
        </div>
      </div>
    </div>
  );
}
