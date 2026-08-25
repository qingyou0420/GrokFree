import { useMemo, useState } from "react";
import type { PermissionReq } from "../lib/types";

function asRecord(v: unknown): Record<string, unknown> | null {
  return v && typeof v === "object" && !Array.isArray(v)
    ? (v as Record<string, unknown>)
    : null;
}

function pickString(...vals: unknown[]): string | null {
  for (const v of vals) {
    if (typeof v === "string" && v.trim()) return v.trim();
  }
  return null;
}

function summarizePermission(params: Record<string, unknown>) {
  const tool =
    asRecord(params.toolCall) ||
    asRecord(params.tool_call) ||
    asRecord(params.tool) ||
    params;

  const title =
    pickString(tool.title, tool.name, params.title, params.method) ||
    "工具调用";

  const kind =
    pickString(tool.kind, tool.type, params.kind, params.type) || null;

  const input = asRecord(tool.input) || asRecord(tool.arguments) || asRecord(params.input);
  const locations = Array.isArray(tool.locations)
    ? tool.locations
    : Array.isArray(params.locations)
      ? params.locations
      : [];

  const path =
    pickString(
      tool.path,
      tool.filePath,
      tool.file_path,
      input?.path,
      input?.filePath,
      input?.file_path,
      input?.target,
      (locations[0] as Record<string, unknown> | undefined)?.path
    ) || null;

  const command =
    pickString(
      tool.command,
      input?.command,
      input?.cmd,
      Array.isArray(input?.args) ? (input!.args as unknown[]).join(" ") : null
    ) || null;

  const description =
    pickString(tool.description, params.description, tool.detail) || null;

  // Heuristic risk
  const lower = `${title} ${kind || ""} ${command || ""} ${path || ""}`.toLowerCase();
  let risk: "low" | "medium" | "high" = "medium";
  if (
    /write|edit|delete|remove|rm |shell|terminal|exec|run|bash|powershell|install|force/.test(
      lower
    )
  ) {
    risk = "high";
  } else if (/read|list|search|grep|glob|stat|open/.test(lower)) {
    risk = "low";
  }

  return { title, kind, path, command, description, risk };
}

/** 从请求自带的 options 里挑「允许一次」的 optionId，避免猜 CLI 不认识的 id */
function pickAllowOnceId(params: Record<string, unknown>): string {
  const options = Array.isArray(params.options) ? params.options : [];
  for (const raw of options) {
    const opt = asRecord(raw);
    if (!opt) continue;
    const kind = String(opt.kind ?? "").toLowerCase().replace(/-/g, "_");
    const oid = pickString(opt.optionId, opt.option_id, opt.id);
    if (oid && (kind === "allow_once" || oid.toLowerCase() === "allow-once")) {
      return oid;
    }
  }
  for (const raw of options) {
    const opt = asRecord(raw);
    if (!opt) continue;
    const oid = pickString(opt.optionId, opt.option_id, opt.id);
    const l = (oid ?? "").toLowerCase();
    if (oid && l.includes("allow") && !l.includes("always")) return oid;
  }
  return "allow-once";
}

export function PermissionModal({
  request,
  onRespond,
}: {
  request: PermissionReq;
  onRespond: (allow: boolean, optionId?: string, rememberSession?: boolean) => void;
}) {
  const params = useMemo(() => request.params || {}, [request.params]);
  const summary = useMemo(() => summarizePermission(params), [params]);
  const allowOnceId = useMemo(() => pickAllowOnceId(params), [params]);
  const [showRaw, setShowRaw] = useState(false);

  const riskLabel =
    summary.risk === "high"
      ? "较高风险"
      : summary.risk === "low"
        ? "较低风险"
        : "需确认";

  return (
    <div className="modal-backdrop">
      <div className="modal" role="dialog" aria-modal="true">
        <header>
          <span>需要授权</span>
          <span className={`risk-badge risk-${summary.risk}`}>{riskLabel}</span>
        </header>
        <div className="body">
          <div className="perm-summary">
            <div className="perm-title-row">
              小精灵请求执行：
              <strong>{summary.title}</strong>
            </div>
            {summary.kind && (
              <div className="perm-field">
                <span className="perm-label">类型</span>
                <span>{summary.kind}</span>
              </div>
            )}
            {summary.path && (
              <div className="perm-field">
                <span className="perm-label">路径</span>
                <code className="perm-code" title={summary.path}>
                  {summary.path}
                </code>
              </div>
            )}
            {summary.command && (
              <div className="perm-field">
                <span className="perm-label">命令</span>
                <code className="perm-code" title={summary.command}>
                  {summary.command}
                </code>
              </div>
            )}
            {summary.description && (
              <div className="perm-field">
                <span className="perm-label">说明</span>
                <span>{summary.description}</span>
              </div>
            )}
            {!summary.path && !summary.command && !summary.description && (
              <div className="help">
                未解析到路径或命令摘要，可展开技术细节查看完整请求。
              </div>
            )}
          </div>

          <details
            className="perm-raw"
            open={showRaw}
            onToggle={(e) => setShowRaw((e.target as HTMLDetailsElement).open)}
          >
            <summary>技术细节（原始 JSON）</summary>
            <pre className="perm-detail">
              {JSON.stringify(params, null, 2)}
            </pre>
          </details>
        </div>
        <div className="footer">
          <button
            type="button"
            className="btn danger"
            onClick={() => onRespond(false)}
          >
            拒绝
          </button>
          <button
            type="button"
            className="btn"
            onClick={() => onRespond(true, allowOnceId)}
          >
            仅允许一次
          </button>
          <button
            type="button"
            className="btn primary"
            title={
              request.scopeKey
                ? `本会话内自动批准同类操作（${request.scopeKey}）`
                : "本会话内自动批准同类操作"
            }
            onClick={() => onRespond(true, allowOnceId, true)}
          >
            本会话内允许
          </button>
        </div>
      </div>
    </div>
  );
}
