import { memo, useMemo, useState } from "react";
import type { DiffItem, GitStatus, PlanEntry } from "../lib/types";
import type { ChatBlock } from "../lib/types";
import { statusLabel } from "../lib/i18n";
import { isSilentRawMethod } from "../lib/acp-parse";
import { Markdown } from "./Markdown";
import { IconAgent, IconTool } from "./Icons";

function colorize(patch: string) {
  return patch.split("\n").map((line, i) => {
    let cls = "";
    if (line.startsWith("+") && !line.startsWith("+++")) cls = "diff-line-add";
    else if (line.startsWith("-") && !line.startsWith("---")) cls = "diff-line-del";
    return (
      <div key={i} className={cls}>
        {line || " "}
      </div>
    );
  });
}

function pathBase(p: string) {
  const parts = p.replace(/\\/g, "/").split("/");
  return parts[parts.length - 1] || p;
}

function pathDir(p: string) {
  const norm = p.replace(/\\/g, "/");
  const i = norm.lastIndexOf("/");
  return i >= 0 ? norm.slice(0, i) : "";
}

function PlanEntries({ entries }: { entries: PlanEntry[] }) {
  return (
    <ol className="plan-entries">
      {entries.map((e, i) => (
        <li key={i} className={e.status ? `plan-st-${e.status}` : undefined}>
          <span>{e.content}</span>
          {e.status && (
            <span className="plan-entry-st">{statusLabel(e.status)}</span>
          )}
        </li>
      ))}
    </ol>
  );
}

export function PlanBanner({
  plan,
  onDismiss,
  onConfirm,
}: {
  plan: Extract<ChatBlock, { kind: "plan" }>;
  onDismiss?: () => void;
  onConfirm?: () => void;
}) {
  return (
    <div className="plan-banner">
      <div className="plan-banner-top">
        <strong>计划模式</strong>
        <span className="plan-banner-hint">小精灵已产出执行计划（只读）</span>
        <span className="spacer" />
        {onConfirm && (
          <button type="button" className="btn sm primary" onClick={onConfirm}>
            确认执行
          </button>
        )}
        {onDismiss && (
          <button type="button" className="btn sm ghost" onClick={onDismiss}>
            收起
          </button>
        )}
      </div>
      {plan.entries && plan.entries.length > 0 ? (
        <PlanEntries entries={plan.entries} />
      ) : (
        <pre className="plan-banner-body">{plan.text}</pre>
      )}
    </div>
  );
}

export type TranscriptFilter = "all" | "chat" | "tools" | "plan";

function matchesFilter(b: ChatBlock, filter: TranscriptFilter): boolean {
  if (filter === "all") return true;
  if (filter === "chat") return b.kind === "user" || b.kind === "assistant";
  if (filter === "tools") return b.kind === "tool" || b.kind === "diff";
  if (filter === "plan") return b.kind === "plan" || b.kind === "thought";
  return true;
}

function toolIsActive(status: string) {
  const s = status.toLowerCase();
  return (
    s === "pending" ||
    s === "in_progress" ||
    s === "running" ||
    s === "started" ||
    s === ""
  );
}

function toolFailed(status: string) {
  const s = status.toLowerCase();
  return s === "failed" || s === "error" || s === "cancelled";
}

/** memo：applyStream 只更新目标 tool 块的对象引用，其余工具卡跳过重渲染 */
const ToolCard = memo(function ToolCard({
  b,
}: {
  b: Extract<ChatBlock, { kind: "tool" }>;
}) {
  const body =
    b.output !== undefined
      ? typeof b.output === "string"
        ? b.output
        : JSON.stringify(b.output, null, 2)
      : b.input !== undefined
        ? typeof b.input === "string"
          ? b.input
          : JSON.stringify(b.input, null, 2)
        : "";
  const active = toolIsActive(b.status);
  // 历史/已完成工具默认折叠，减少打开会话时的视觉噪声；运行中保持展开
  const defaultOpen = active;

  return (
    <div className="msg msg-agent msg-tool">
      <div className="bubble bubble-agent">
        <details
          className={`tool-card ${b.subagent ? "subagent-card" : ""}`}
          open={defaultOpen}
        >
          <summary className="tool-card-summary">
            <span className="tool-icon" aria-hidden>
              {b.subagent ? <IconAgent size={14} /> : <IconTool size={14} />}
            </span>
            <span className="tool-title">
              {b.subagent ? "子代理 · " : ""}
              {b.title}
            </span>
            <span className={`badge ${b.status}`}>{statusLabel(b.status)}</span>
          </summary>
          {body ? <pre>{body}</pre> : null}
        </details>
      </div>
    </div>
  );
});

/** memo：大补丁的 colorize 每行建元素，避免无关更新重复执行 */
const DiffCard = memo(function DiffCard({
  b,
}: {
  b: Extract<ChatBlock, { kind: "diff" }>;
}) {
  return (
    <div className="msg msg-agent msg-diff">
      <div className="bubble bubble-agent">
        <div className="diff-card">
          <header>
            <span className="review-path-base">
              {pathBase(b.path)}
            </span>
            <span className="review-path-dir">
              {pathDir(b.path)}
            </span>
          </header>
          <pre>{colorize(b.patch)}</pre>
        </div>
      </div>
    </div>
  );
});

export const ChatBlocks = memo(function ChatBlocks({
  blocks,
  showRawAcpEvents = false,
  filter = "all",
}: {
  blocks: ChatBlock[];
  showRawAcpEvents?: boolean;
  filter?: TranscriptFilter;
}) {
  const visible = useMemo(
    () => blocks.filter((b) => matchesFilter(b, filter)),
    [blocks, filter]
  );

  return (
    <div className="transcript" data-block-count={visible.length}>
      {visible.map((b) => {
        switch (b.kind) {
          case "user":
            return (
              <div key={b.id} className="msg msg-user">
                <div className="msg-user-col">
                  <div className="msg-role">Master</div>
                  <div className="bubble bubble-user">{b.text}</div>
                </div>
              </div>
            );
          case "assistant":
            return (
              <div key={b.id} className="msg msg-agent msg-assistant">
                <div className="msg-role">
                  <span className="msg-role-dot" />
                  Grok
                </div>
                <div className="bubble bubble-agent block assistant">
                  <Markdown text={b.text} />
                </div>
              </div>
            );
          case "thought":
            return (
              <div key={b.id} className="msg msg-agent">
                <div className="bubble bubble-agent">
                  <details className="block thought" open={false}>
                    <summary>思考过程</summary>
                    <div className="thought-body">{b.text}</div>
                  </details>
                </div>
              </div>
            );
          case "tool":
            return <ToolCard key={b.id} b={b} />;
          case "plan":
            return (
              <div key={b.id} className="msg msg-agent msg-plan">
                <div className="bubble bubble-agent">
                  <div className="plan-card">
                    <strong>计划</strong>
                    {b.entries && b.entries.length > 0 ? (
                      <PlanEntries entries={b.entries} />
                    ) : (
                      <div className="plan-card-body">{b.text}</div>
                    )}
                  </div>
                </div>
              </div>
            );
          case "diff":
            return <DiffCard key={b.id} b={b} />;
          case "system":
            return (
              <div key={b.id} className="msg msg-system">
                <div className="block system">{b.text}</div>
              </div>
            );
          case "raw":
            if (!showRawAcpEvents || isSilentRawMethod(b.method)) return null;
            return (
              <div key={b.id} className="msg msg-agent">
                <div className="bubble bubble-agent">
                  <details className="block raw-block">
                    <summary>
                      未识别事件 · <code>{b.method}</code>
                    </summary>
                    <pre>{b.text}</pre>
                  </details>
                </div>
              </div>
            );
          default:
            return null;
        }
      })}
      {visible.length === 0 && (
        <div className="transcript-empty-filter">
          当前筛选下没有消息。
        </div>
      )}
    </div>
  );
});

export function TranscriptToolbar({
  filter,
  onFilter,
  onScrollBottom,
  count,
}: {
  filter: TranscriptFilter;
  onFilter: (f: TranscriptFilter) => void;
  onScrollBottom: () => void;
  count: number;
}) {
  const tabs: { id: TranscriptFilter; label: string }[] = [
    { id: "all", label: "全部" },
    { id: "chat", label: "对话" },
    { id: "tools", label: "工具" },
    { id: "plan", label: "计划" },
  ];
  return (
    <div className="transcript-toolbar">
      <div className="segmented">
        {tabs.map((t) => (
          <button
            key={t.id}
            type="button"
            className={filter === t.id ? "active" : ""}
            onClick={() => onFilter(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>
      <span className="transcript-count">{count} 条</span>
      <span className="spacer" />
      <button type="button" className="btn sm ghost" onClick={onScrollBottom}>
        滚到底
      </button>
    </div>
  );
}

export function ReviewPane({
  diffs,
  git,
  busyPath,
  batchBusy,
  onClose,
  onOpen,
  onAccept,
  onReject,
  onAcceptAll,
  onRejectAll,
  onRefreshGit,
  onOpenProject,
  onCopyPatch,
}: {
  diffs: DiffItem[];
  git: GitStatus | null;
  busyPath: string | null;
  batchBusy?: boolean;
  onClose: () => void;
  onOpen: (path: string) => void;
  onAccept: (path: string, patch: string) => void;
  onReject: (path: string) => void;
  onAcceptAll?: () => void;
  onRejectAll?: () => void;
  onRefreshGit: () => void;
  onOpenProject?: () => void;
  onCopyPatch?: (path: string, patch: string) => void;
}) {
  const [tab, setTab] = useState<"changes" | "git">("changes");
  const pending = diffs.filter((d) => !d.decision || d.decision === "pending");
  const decided = diffs.filter(
    (d) => d.decision === "accepted" || d.decision === "rejected"
  );

  return (
    <aside className="review">
      <header>
        <span>变更</span>
        <button type="button" className="icon-btn" onClick={onClose} title="关闭">
          ✕
        </button>
      </header>

      <div className="review-tabs">
        <button
          type="button"
          className={tab === "changes" ? "active" : ""}
          onClick={() => setTab("changes")}
        >
          变更
          {pending.length > 0 && (
            <span className="tab-badge">{pending.length}</span>
          )}
        </button>
        <button
          type="button"
          className={tab === "git" ? "active" : ""}
          onClick={() => setTab("git")}
        >
          Git
          {git?.dirty ? <span className="tab-badge muted">•</span> : null}
        </button>
      </div>

      {tab === "git" && (
        <div className="git-strip">
          <div className="git-strip-row">
            <span className="git-branch">
              {git?.isRepo ? git.branch || "HEAD" : "非 Git 仓库"}
              {git?.isRepo && (git.ahead || git.behind)
                ? ` · ↑${git.ahead ?? 0} ↓${git.behind ?? 0}`
                : ""}
            </span>
            <button
              type="button"
              className="btn sm ghost"
              onClick={onRefreshGit}
              title="刷新 git status"
            >
              刷新
            </button>
          </div>
          <div className="git-msg">{git?.message || "—"}</div>
          {git && git.entries.length > 0 && (
            <ul className="git-list">
              {git.entries.slice(0, 20).map((e) => (
                <li key={`${e.status}-${e.path}`}>
                  <code className="git-st">{e.status}</code>
                  <span title={e.path}>
                    <span className="review-path-base">{pathBase(e.path)}</span>
                    {pathDir(e.path) ? (
                      <span className="review-path-dir"> {pathDir(e.path)}</span>
                    ) : null}
                  </span>
                </li>
              ))}
              {git.entries.length > 20 && (
                <li className="git-more">…另有 {git.entries.length - 20} 个</li>
              )}
            </ul>
          )}
        </div>
      )}

      {tab === "changes" && (
        <>
          {pending.length > 1 && (onAcceptAll || onRejectAll) && (
            <div className="review-batch">
              {onAcceptAll && (
                <button
                  type="button"
                  className="btn sm primary"
                  disabled={!!batchBusy || !!busyPath}
                  onClick={onAcceptAll}
                >
                  {batchBusy ? "处理中…" : `全部接受 (${pending.length})`}
                </button>
              )}
              {onRejectAll && (
                <button
                  type="button"
                  className="btn sm ghost"
                  disabled={!!batchBusy || !!busyPath}
                  onClick={onRejectAll}
                >
                  全部忽略
                </button>
              )}
            </div>
          )}

          <div className="review-body">
            {diffs.length === 0 ? (
              <div className="review-empty">
                小精灵产生的文件变更会显示在这里。
                <br />
                可「接受」写入磁盘，或「忽略」丢弃。
                {onOpenProject && (
                  <button
                    type="button"
                    className="btn sm"
                    style={{ marginTop: 12 }}
                    onClick={onOpenProject}
                  >
                    在编辑器中打开项目
                  </button>
                )}
              </div>
            ) : (
              <>
                {pending.length > 0 && (
                  <div className="review-section-label">
                    待处理 · {pending.length}
                    {decided.length ? ` · 已处理 ${decided.length}` : ""}
                  </div>
                )}
                {diffs.map((d, i) => {
                  const decision = d.decision || "pending";
                  const busy = busyPath === d.path;
                  const openDefault =
                    decision === "pending" &&
                    i ===
                      diffs.findIndex(
                        (x) => !x.decision || x.decision === "pending"
                      );
                  return (
                    <details
                      key={`${d.path}-${i}`}
                      className={`review-file decision-${decision}`}
                      open={openDefault}
                    >
                      <summary>
                        <span className="review-path-stack">
                          <span className="review-path-base">
                            {pathBase(d.path)}
                          </span>
                          {pathDir(d.path) && (
                            <span className="review-path-dir">
                              {pathDir(d.path)}
                            </span>
                          )}
                        </span>
                        {decision !== "pending" && (
                          <span className={`decision-tag ${decision}`}>
                            {decision === "accepted" ? "已接受" : "已忽略"}
                          </span>
                        )}
                      </summary>
                      <div className="review-file-actions">
                        <button
                          type="button"
                          className="btn sm"
                          onClick={() => onOpen(d.path)}
                        >
                          打开
                        </button>
                        <button
                          type="button"
                          className="btn sm primary"
                          disabled={busy || decision === "accepted"}
                          onClick={() => onAccept(d.path, d.patch)}
                        >
                          {busy ? "…" : "接受"}
                        </button>
                        <button
                          type="button"
                          className="btn sm ghost"
                          disabled={busy || decision === "rejected"}
                          onClick={() => onReject(d.path)}
                        >
                          忽略
                        </button>
                        {onCopyPatch && (
                          <button
                            type="button"
                            className="btn sm ghost"
                            onClick={() => onCopyPatch(d.path, d.patch)}
                            title="复制 unified diff"
                          >
                            复制
                          </button>
                        )}
                      </div>
                      <pre>{colorize(d.patch)}</pre>
                    </details>
                  );
                })}
              </>
            )}
          </div>
        </>
      )}
    </aside>
  );
}
