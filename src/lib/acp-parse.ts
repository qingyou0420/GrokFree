import type { ChatBlock, PlanEntry } from "./types";

/**
 * sessionUpdate kinds that must not become "未识别事件" raw blocks.
 * Includes standard ACP noise + known Grok CLI / xAI vendor extensions
 * (`_x.ai/session/update`). True unknowns still surface as foldable raw
 * when prefs.showRawAcpEvents is on.
 */
const SILENT_SESSION_UPDATES = new Set([
  "",
  // Standard ACP / client noise
  "session_info_update",
  "current_mode_update",
  "user_message_chunk",
  "available_commands_update",
  "config_option_update",
  "usage_update",
  "token_usage",
  "rate_limit",
  "progress",
  "notify",
  "notification",
  "resource_update",
  "mode_update",
  "tool_call_progress",
  "stream_event",
  "heartbeat",
  "ping",
  // Grok CLI xAI extensions (observed in ~/.grok/sessions/*/updates.jsonl)
  "turn_completed",
  "turn_started",
  "task_backgrounded",
  "task_completed",
  "session_recap",
  "subagent_spawned",
  "subagent_finished",
  "workflow_updated",
  "monitor_updated",
  "capability_update",
  "model_update",
]);

/** Max distinct raw blocks kept when showRaw is on (prevents flood). */
const MAX_RAW_BLOCKS = 5;

/** Lifecycle / vendor noise patterns (prefix or suffix). */
function isSilentSessionUpdateKind(k: string, method = ""): boolean {
  if (!k && (method.startsWith("_x.ai/") || method === "session/update")) {
    return true;
  }
  if (SILENT_SESSION_UPDATES.has(k)) return true;
  // Subagent / task / workflow lifecycle
  if (
    k.startsWith("subagent_") ||
    k.startsWith("task_") ||
    k.startsWith("workflow_") ||
    k.startsWith("monitor_") ||
    k.startsWith("usage_") ||
    k.startsWith("token_") ||
    k.endsWith("_completed") ||
    k.endsWith("_backgrounded") ||
    k.endsWith("_spawned") ||
    k.endsWith("_finished") ||
    k.endsWith("_update") ||
    k.endsWith("_updated")
  ) {
    return true;
  }
  // Vendor envelope with no nested kind we care about
  if (method.startsWith("_x.ai/") && !k) return true;
  if (method.startsWith("fs/") || method.startsWith("terminal/")) return true;
  return false;
}

/** Export for UI: hide raw blocks already buffered under silent methods. */
export function isSilentRawMethod(method: string): boolean {
  return isSilentSessionUpdateKind(method, method);
}

/**
 * Strip noise from a transcript (silent raw + optional hide-all raw).
 * Used when switching sessions and when rendering.
 *
 * Hot path: this runs for every stream event, so it must return the SAME
 * array reference when there is nothing to strip — callers rely on reference
 * equality to skip store updates and re-renders.
 */
export function scrubTranscript(
  blocks: ChatBlock[],
  showRawAcpEvents = false
): ChatBlock[] {
  let hasRaw = false;
  for (const b of blocks) {
    if (b.kind === "raw") {
      hasRaw = true;
      break;
    }
  }
  if (!hasRaw) return blocks;
  const withoutSilent = blocks.filter(
    (b) => !(b.kind === "raw" && isSilentRawMethod(b.method))
  );
  // filter 总是新建数组：没有剥掉任何块时必须交回原引用，
  // 否则含 raw 的转录在每个后续流式事件上都会白拷贝（applyStream 热路径）
  const kept =
    withoutSilent.length === blocks.length ? blocks : withoutSilent;
  if (!showRawAcpEvents) {
    const noRaw = kept.filter((b) => b.kind !== "raw");
    return noRaw.length === kept.length ? kept : noRaw;
  }
  // Cap raw flood: keep last MAX_RAW_BLOCKS raw items.
  // 先数后剪：需要剪时才复制，保证无变化路径返回原引用
  let overflow = 0;
  let rawCount = 0;
  for (let i = kept.length - 1; i >= 0; i--) {
    if (kept[i].kind === "raw" && ++rawCount > MAX_RAW_BLOCKS) overflow++;
  }
  if (overflow === 0) return kept;
  const next = [...kept];
  rawCount = 0;
  for (let i = next.length - 1; i >= 0; i--) {
    if (next[i].kind === "raw") {
      rawCount++;
      if (rawCount > MAX_RAW_BLOCKS) {
        next.splice(i, 1);
      }
    }
  }
  return next;
}

function uid(p = "b") {
  return `${p}_${Date.now()}_${Math.random().toString(36).slice(2, 7)}`;
}

function textFrom(obj: unknown): string {
  if (!obj) return "";
  if (typeof obj === "string") return obj;
  const o = obj as Record<string, unknown>;
  if (typeof o.text === "string") return o.text;
  if (o.content && typeof (o.content as { text?: string }).text === "string") {
    return (o.content as { text: string }).text;
  }
  return "";
}

function looksLikeSubagent(title: string, update: Record<string, unknown>): boolean {
  const t = (title || "").toLowerCase();
  if (
    t.includes("subagent") ||
    t.includes("sub-agent") ||
    t.includes("子代理") ||
    t.includes("spawn") ||
    t.includes("task")
  ) {
    return true;
  }
  const name = String(update.name || update.toolName || update.kind || "").toLowerCase();
  return (
    name.includes("subagent") ||
    name === "task" ||
    name === "spawn_subagent" ||
    name === "agent"
  );
}

function parsePlanEntries(update: Record<string, unknown>): PlanEntry[] | undefined {
  const raw = update.entries ?? update.plan ?? update.items;
  if (!Array.isArray(raw)) return undefined;
  const entries: PlanEntry[] = [];
  for (const item of raw) {
    if (typeof item === "string") {
      entries.push({ content: item });
      continue;
    }
    if (item && typeof item === "object") {
      const o = item as Record<string, unknown>;
      const content =
        (typeof o.content === "string" && o.content) ||
        (typeof o.text === "string" && o.text) ||
        (typeof o.title === "string" && o.title) ||
        JSON.stringify(o);
      entries.push({
        content,
        status: typeof o.status === "string" ? o.status : undefined,
        priority: typeof o.priority === "string" ? o.priority : undefined,
      });
    }
  }
  return entries.length ? entries : undefined;
}

export function applyStream(
  blocks: ChatBlock[],
  method: string,
  params: Record<string, unknown>,
  opts?: { showRawAcpEvents?: boolean }
): ChatBlock[] {
  const showRaw = opts?.showRawAcpEvents === true;

  // Prefer nested `params.update`; only use `params.sessionUpdate` when it is an object
  // (a string kind at that key must not become `update` itself — truthy strings broke kind parse).
  const updateCandidate =
    (params.update && typeof params.update === "object"
      ? params.update
      : null) ||
    (params.sessionUpdate && typeof params.sessionUpdate === "object"
      ? params.sessionUpdate
      : null) ||
    params;

  const update = (
    updateCandidate && typeof updateCandidate === "object"
      ? updateCandidate
      : {}
  ) as Record<string, unknown>;

  const kind =
    (typeof update.sessionUpdate === "string" && update.sessionUpdate) ||
    (typeof update.type === "string" && update.type) ||
    (typeof params.sessionUpdate === "string" ? params.sessionUpdate : "") ||
    (method.includes("update") ? "" : method) ||
    "";

  const k = kind;
  // Drop lifecycle noise already buffered under older parsers (other live sessions).
  // scrubTranscript returns `blocks` itself when there is nothing to strip —
  // callers rely on reference equality to skip no-op updates, so every branch
  // that mutates MUST copy first (copy-on-write) and never touch `blocks`.
  let next = scrubTranscript(blocks, showRaw);

  switch (k) {
    case "agent_message_chunk":
    case "agent_message":
    case "message": {
      const chunk = textFrom(update.content) || textFrom(update);
      if (!chunk) return next;
      if (next === blocks) next = [...blocks];
      const last = next[next.length - 1];
      if (last?.kind === "assistant") {
        next[next.length - 1] = { ...last, text: last.text + chunk };
      } else {
        next.push({ kind: "assistant", id: uid("a"), text: chunk });
      }
      return next;
    }
    case "agent_thought_chunk":
    case "agent_thought":
    case "thought": {
      const chunk = textFrom(update.content) || textFrom(update);
      if (!chunk) return next;
      if (next === blocks) next = [...blocks];
      const last = next[next.length - 1];
      if (last?.kind === "thought") {
        next[next.length - 1] = { ...last, text: last.text + chunk };
      } else {
        next.push({ kind: "thought", id: uid("t"), text: chunk });
      }
      return next;
    }
    case "tool_call": {
      const toolCallId =
        (update.toolCallId as string) ||
        (update.tool_call_id as string) ||
        (update.id as string) ||
        uid("tool");
      const title = (update.title as string) || (update.name as string) || "tool";
      if (next === blocks) next = [...blocks];
      next.push({
        kind: "tool",
        id: uid("tool"),
        toolCallId,
        title,
        status: (update.status as string) || "pending",
        input: update.rawInput ?? update.input ?? update.arguments,
        subagent: looksLikeSubagent(title, update),
      });
      return next;
    }
    case "tool_call_update": {
      const toolCallId =
        (update.toolCallId as string) ||
        (update.tool_call_id as string) ||
        (update.id as string);
      const idx = next.findIndex(
        (b) => b.kind === "tool" && b.toolCallId === toolCallId
      );
      const target = idx >= 0 ? next[idx] : undefined;
      if (target && target.kind === "tool") {
        if (next === blocks) next = [...blocks];
        const title = (update.title as string) || target.title;
        next[idx] = {
          ...target,
          status: (update.status as string) || target.status,
          output: update.rawOutput ?? update.output ?? update.content ?? target.output,
          title,
          subagent: target.subagent || looksLikeSubagent(title, update),
        };
      }
      return next;
    }
    case "plan":
    case "agent_plan": {
      const entries = parsePlanEntries(update);
      const text =
        textFrom(update) ||
        (entries
          ? entries.map((e, i) => `${i + 1}. ${e.content}${e.status ? ` [${e.status}]` : ""}`).join("\n")
          : update.entries
            ? JSON.stringify(update.entries, null, 2)
            : "");
      if (!text && !entries?.length) return next;
      // Replace last open plan or append
      if (next === blocks) next = [...blocks];
      const lastPlan = [...next].reverse().findIndex((b) => b.kind === "plan");
      if (lastPlan >= 0) {
        const idx = next.length - 1 - lastPlan;
        next[idx] = {
          kind: "plan",
          id: (next[idx] as { id: string }).id,
          text: text || (next[idx] as { text: string }).text,
          entries: entries || (next[idx] as { entries?: PlanEntry[] }).entries,
        };
      } else {
        next.push({ kind: "plan", id: uid("plan"), text, entries });
      }
      return next;
    }
    default: {
      // Known noise / xAI vendor extensions — do not clutter transcript
      if (isSilentSessionUpdateKind(k, method)) {
        return next;
      }
      if (update && typeof update === "object" && update.path && (update.patch || update.diff)) {
        if (next === blocks) next = [...blocks];
        next.push({
          kind: "diff",
          id: uid("diff"),
          path: String(update.path),
          patch: String(update.patch || update.diff),
        });
        return next;
      }
      // Unknown ACP event → raw only when user opted in
      if (!showRaw) {
        return next;
      }
      if (
        k &&
        k !== "session/update" &&
        !method.startsWith("fs/") &&
        !method.startsWith("terminal/") &&
        !method.startsWith("_x.ai/")
      ) {
        const snippet = JSON.stringify(update, null, 2);
        if (snippet && snippet !== "{}" && snippet.length < 8000) {
          // Avoid flooding: merge consecutive raw of same method
          const last = next[next.length - 1];
          if (last?.kind === "raw" && last.method === k) {
            if (next === blocks) next = [...blocks];
            next[next.length - 1] = { ...last, text: snippet };
          } else if (
            update &&
            typeof update === "object" &&
            Object.keys(update as object).length > 0
          ) {
            const rawCount = next.filter((b) => b.kind === "raw").length;
            if (rawCount >= MAX_RAW_BLOCKS) {
              return next;
            }
            if (next === blocks) next = [...blocks];
            next.push({
              kind: "raw",
              id: uid("raw"),
              method: k || method,
              text: snippet,
            });
          }
        }
      }
      return next;
    }
  }
}

/** Latest plan block for the Plan banner. */
export function latestPlan(blocks: ChatBlock[]): Extract<ChatBlock, { kind: "plan" }> | null {
  for (let i = blocks.length - 1; i >= 0; i--) {
    const b = blocks[i];
    if (b.kind === "plan") return b;
  }
  return null;
}

/** Placeholder / uuid-only session title (mirrors Rust is_placeholder_title). */
export function isPlaceholderTitle(title: string): boolean {
  const t = (title || "").trim();
  if (!t) return true;
  if (
    t === "新会话" ||
    t === "New session" ||
    t === "session" ||
    t === "Session" ||
    t === "untitled" ||
    t === "Untitled"
  ) {
    return true;
  }
  const base = t.replace(/…+$/, "").replace(/\.+$/, "");
  if (
    base.length >= 6 &&
    base.length <= 36 &&
    /^[0-9a-fA-F-]+$/.test(base) &&
    (t.endsWith("…") || t.endsWith("...") || base.length >= 20)
  ) {
    return true;
  }
  if (t.length >= 20 && t.length <= 40 && /^[0-9a-fA-F-]+$/.test(t)) {
    return true;
  }
  return false;
}
