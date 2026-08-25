import { describe, expect, it } from "vitest";
import {
  applyStream,
  isPlaceholderTitle,
  latestPlan,
  scrubTranscript,
} from "./acp-parse";
import type { ChatBlock } from "./types";

/**
 * applyStream 是转录渲染的正确性核心，且调用方（agent://stream 订阅）
 * 依赖「无变化 → 返回同一数组引用」跳过 store 更新与重渲染。
 * 这套测试锁住两件事：块变换语义 + copy-on-write 引用相等约定。
 * 后者被重构改坏是静默的（只在流式高频更新时变卡），必须显式断言。
 */

const upd = (update: Record<string, unknown>) => ({
  update,
});

describe("applyStream · copy-on-write 引用相等", () => {
  it("静默事件（usage_update 等）必须返回同一引用", () => {
    const blocks: ChatBlock[] = [{ kind: "user", id: "u1", text: "hi" }];
    const r = applyStream(blocks, "session/update", {
      update: { sessionUpdate: "usage_update" },
    });
    expect(r).toBe(blocks);
  });

  it("空 chunk（无文本）返回同一引用，不追加空块", () => {
    const blocks: ChatBlock[] = [{ kind: "assistant", id: "a1", text: "hel" }];
    const r = applyStream(blocks, "session/update", {
      update: { sessionUpdate: "agent_message_chunk", content: {} },
    });
    expect(r).toBe(blocks);
  });

  it("未匹配的 tool_call_update 返回同一引用", () => {
    const blocks: ChatBlock[] = [{ kind: "user", id: "u1", text: "hi" }];
    const r = applyStream(blocks, "session/update", {
      update: { sessionUpdate: "tool_call_update", toolCallId: "nope", status: "success" },
    });
    expect(r).toBe(blocks);
  });

  it("showRaw 关闭时未知事件返回同一引用", () => {
    const blocks: ChatBlock[] = [{ kind: "user", id: "u1", text: "hi" }];
    const r = applyStream(blocks, "session/update", {
      update: { sessionUpdate: "some_unknown_kind", foo: 1 },
    });
    expect(r).toBe(blocks);
  });

  it("无 raw 块时 scrubTranscript 路径返回同一引用", () => {
    const blocks: ChatBlock[] = [
      { kind: "assistant", id: "a1", text: "x" },
      { kind: "tool", id: "t1", toolCallId: "tc1", title: "bash", status: "pending" },
    ];
    // tool_call_update 命中后走修改分支，先经过 scrub（无 raw → 同引用起点）
    const r = applyStream(blocks, "session/update", {
      update: { sessionUpdate: "tool_call_update", toolCallId: "tc1", status: "success" },
    });
    expect(r).not.toBe(blocks);
    expect((r[1] as { status: string }).status).toBe("success");
    expect((blocks[1] as { status: string }).status).toBe("pending");
  });
});

describe("applyStream · assistant / thought chunk 合并", () => {
  it("连续 assistant chunk 拼接到同一块，不产生新块", () => {
    let blocks: ChatBlock[] = [];
    blocks = applyStream(blocks, "session/update", upd({
      sessionUpdate: "agent_message_chunk",
      content: { text: "你好" },
    }));
    blocks = applyStream(blocks, "session/update", upd({
      sessionUpdate: "agent_message_chunk",
      content: { text: "，世界" },
    }));
    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toMatchObject({ kind: "assistant", text: "你好，世界" });
  });

  it("assistant 块后插入 tool，再来的 assistant chunk 开新块", () => {
    let blocks: ChatBlock[] = applyStream([], "session/update", upd({
      sessionUpdate: "agent_message_chunk",
      content: { text: "先说" },
    }));
    blocks = applyStream(blocks, "session/update", upd({
      sessionUpdate: "tool_call",
      toolCallId: "tc1",
      title: "bash",
    }));
    blocks = applyStream(blocks, "session/update", upd({
      sessionUpdate: "agent_message_chunk",
      content: { text: "后说" },
    }));
    expect(blocks.map((b) => b.kind)).toEqual(["assistant", "tool", "assistant"]);
  });

  it("thought chunk 连续拼接；thought 与 assistant 互不合并", () => {
    let blocks: ChatBlock[] = applyStream([], "session/update", upd({
      sessionUpdate: "agent_thought_chunk",
      content: { text: "想" },
    }));
    blocks = applyStream(blocks, "session/update", upd({
      sessionUpdate: "agent_thought_chunk",
      content: { text: "一想" },
    }));
    blocks = applyStream(blocks, "session/update", upd({
      sessionUpdate: "agent_message_chunk",
      content: { text: "说" },
    }));
    expect(blocks.map((b) => b.kind)).toEqual(["thought", "assistant"]);
    expect(blocks[0]).toMatchObject({ kind: "thought", text: "想一想" });
  });

  it("拼接时原块对象不可变（copy-on-write）", () => {
    const original: ChatBlock[] = [{ kind: "assistant", id: "a1", text: "hel" }];
    const r = applyStream(original, "session/update", upd({
      sessionUpdate: "agent_message_chunk",
      content: { text: "lo" },
    }));
    expect(r).not.toBe(original);
    expect(r[0]).not.toBe(original[0]);
    expect(r[0]).toMatchObject({ text: "hello" });
    expect(original[0]).toMatchObject({ text: "hel" });
  });
});

describe("applyStream · tool_call / tool_call_update", () => {
  it("tool_call 追加块并携带 toolCallId", () => {
    const r = applyStream([], "session/update", upd({
      sessionUpdate: "tool_call",
      toolCallId: "tc1",
      title: "bash",
      rawInput: { cmd: "ls" },
    }));
    expect(r).toHaveLength(1);
    expect(r[0]).toMatchObject({
      kind: "tool",
      toolCallId: "tc1",
      title: "bash",
      status: "pending",
    });
  });

  it("tool_call_update 按 toolCallId 匹配（而非块 id）", () => {
    const blocks: ChatBlock[] = [
      { kind: "user", id: "u1", text: "hi" },
      { kind: "tool", id: "blk-1", toolCallId: "tc-A", title: "bash", status: "pending" },
      { kind: "tool", id: "blk-2", toolCallId: "tc-B", title: "read", status: "pending" },
    ];
    const r = applyStream(blocks, "session/update", upd({
      sessionUpdate: "tool_call_update",
      toolCallId: "tc-B",
      status: "success",
      rawOutput: "done",
    }));
    expect(r[1]).toMatchObject({ toolCallId: "tc-A", status: "pending" });
    expect(r[2]).toMatchObject({ toolCallId: "tc-B", status: "success", output: "done" });
    // 未命中的块保持原对象引用
    expect(r[1]).toBe(blocks[1]);
    expect(r[2]).not.toBe(blocks[2]);
  });

  it("tool_call_update 无 status/output 时保留原值", () => {
    const blocks: ChatBlock[] = [
      { kind: "tool", id: "t1", toolCallId: "tc1", title: "bash", status: "running", output: "partial" },
    ];
    const r = applyStream(blocks, "session/update", upd({
      sessionUpdate: "tool_call_update",
      toolCallId: "tc1",
    }));
    expect(r[0]).toMatchObject({ status: "running", output: "partial" });
  });
});

describe("applyStream · plan / diff / raw", () => {
  it("plan 事件替换最后一个 plan 块（同 id），而非追加", () => {
    let blocks: ChatBlock[] = applyStream([], "session/update", upd({
      sessionUpdate: "plan",
      entries: [
        { content: "步骤一", status: "pending" },
        { content: "步骤二", status: "pending" },
      ],
    }));
    const firstId = blocks[0]!.id;
    blocks = applyStream(blocks, "session/update", upd({
      sessionUpdate: "plan",
      entries: [
        { content: "步骤一", status: "completed" },
        { content: "步骤二", status: "pending" },
      ],
    }));
    expect(blocks).toHaveLength(1);
    expect(blocks[0]!.id).toBe(firstId);
    expect(blocks[0]).toMatchObject({ kind: "plan" });
    expect(latestPlan(blocks)?.entries?.[0]).toMatchObject({
      content: "步骤一",
      status: "completed",
    });
  });

  it("默认分支：带 path+patch 的未知事件落为 diff 块", () => {
    const r = applyStream([], "session/update", upd({
      sessionUpdate: "fs_patch",
      path: "src/main.rs",
      patch: "--- a\n+++ b\n",
    }));
    expect(r).toHaveLength(1);
    expect(r[0]).toMatchObject({ kind: "diff", path: "src/main.rs" });
  });

  it("showRaw 开启时未知事件落为 raw 块，连续同 method 合并", () => {
    let blocks: ChatBlock[] = applyStream(
      [],
      "session/update",
      upd({ sessionUpdate: "custom_kind", foo: 1 }),
      { showRawAcpEvents: true }
    );
    blocks = applyStream(
      blocks,
      "session/update",
      upd({ sessionUpdate: "custom_kind", foo: 2 }),
      { showRawAcpEvents: true }
    );
    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toMatchObject({ kind: "raw", method: "custom_kind" });
    expect((blocks[0] as { text: string }).text).toContain('"foo": 2');
  });

  it("showRaw 开启时 raw 块数量封顶，超出返回同一引用", () => {
    let blocks: ChatBlock[] = [];
    for (let i = 0; i < 5; i++) {
      blocks = applyStream(
        blocks,
        "session/update",
        upd({ sessionUpdate: `kind_${i}`, foo: i }),
        { showRawAcpEvents: true }
      );
    }
    expect(blocks.filter((b) => b.kind === "raw")).toHaveLength(5);
    const r = applyStream(
      blocks,
      "session/update",
      upd({ sessionUpdate: "kind_overflow", foo: 99 }),
      { showRawAcpEvents: true }
    );
    expect(r).toBe(blocks);
  });
});

describe("applyStream · params 形态兼容", () => {
  it("顶层字符串 sessionUpdate 也能解析出 kind", () => {
    const r = applyStream([], "session/update", {
      sessionUpdate: "agent_message_chunk",
      content: { text: "hello" },
    });
    expect(r).toHaveLength(1);
    expect(r[0]).toMatchObject({ kind: "assistant", text: "hello" });
  });

  it("params.sessionUpdate 为字符串时不得把字符串当 update 对象", () => {
    // 回归：曾把 truthy 字符串当成 update，导致 kind 解析错乱
    const r = applyStream([], "session/update", {
      sessionUpdate: "agent_message_chunk",
    });
    // 无 content → 无文本 → 同引用（空数组起点也是无操作）
    expect(r).toHaveLength(0);
  });
});

describe("scrubTranscript", () => {
  it("无 raw 块返回同一引用", () => {
    const blocks: ChatBlock[] = [{ kind: "assistant", id: "a1", text: "x" }];
    expect(scrubTranscript(blocks)).toBe(blocks);
    expect(scrubTranscript(blocks, true)).toBe(blocks);
  });

  it("showRaw 关闭时移除全部 raw；开启时保留但封顶", () => {
    const raws: ChatBlock[] = Array.from({ length: 7 }, (_, i) => ({
      kind: "raw",
      id: `r${i}`,
      method: `kind_${i}`,
      text: "{}",
    }));
    expect(scrubTranscript(raws, false)).toHaveLength(0);
    expect(scrubTranscript(raws, true).filter((b) => b.kind === "raw")).toHaveLength(5);
  });

  it("静默 method 的 raw 块总是被移除", () => {
    const blocks: ChatBlock[] = [
      { kind: "raw", id: "r1", method: "usage_update", text: "{}" },
      { kind: "assistant", id: "a1", text: "x" },
      { kind: "raw", id: "r2", method: "custom_kind", text: "{}" },
    ];
    const r = scrubTranscript(blocks, true);
    expect(r.map((b) => b.kind)).toEqual(["assistant", "raw"]);
  });
});

describe("isPlaceholderTitle", () => {
  it.each([
    ["", true],
    ["新会话", true],
    ["Untitled", true],
    ["正常标题", false],
    ["a1b2c3d4-e5f6-7890-abcd-ef1234567890", true],
    ["a1b2c3d4e5f6…", true],
  ])("%s → %s", (title, expected) => {
    expect(isPlaceholderTitle(title)).toBe(expected);
  });
});
