/**
 * 自有会话日志的节流落盘（借鉴 grok-app journal throttle：≥500ms）。
 *
 * 实时事件折叠出的 transcript 是真值；本模块订阅 store，把有变化的会话
 * 快照节流写入 Rust 侧 journals/（异步 command + spawn_blocking，不碰主线程）。
 * CLI 的 chat_history.jsonl 从此只是导入/兜底来源。
 */
import { api } from "./api";
import { useSessionStore } from "../state";
import type { ChatBlock } from "./types";

/** 单会话两次落盘的最小间隔 */
const FLUSH_MS = 500;

const timers = new Map<string, number>();
const lastSaved = new Map<string, ChatBlock[]>();
let started = false;

export function initJournalSync() {
  if (started) return;
  started = true;
  useSessionStore.subscribe((state, prev) => {
    if (state.transcripts === prev.transcripts) return;
    for (const sid of Object.keys(state.transcripts)) {
      const cur = state.transcripts[sid];
      if (!cur || prev.transcripts[sid] === cur) continue;
      schedule(sid);
    }
  });
}

function schedule(sid: string) {
  // trailing 节流：已有定时器则等它触发（触发时取最新快照）
  if (timers.has(sid)) return;
  const t = window.setTimeout(() => {
    timers.delete(sid);
    const blocks = useSessionStore.getState().transcripts[sid];
    if (!blocks || blocks.length === 0) return;
    if (blocks === lastSaved.get(sid)) return;
    lastSaved.set(sid, blocks);
    void api.saveJournal(sid, blocks).catch((e) => {
      // 日志是增强，不该打断使用；失败仅记控制台
      console.error("journal save failed", sid, e);
    });
  }, FLUSH_MS);
  timers.set(sid, t);
}
