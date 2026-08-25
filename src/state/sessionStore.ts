import { create } from "zustand";
import type { ChatBlock, LiveSession } from "../lib/types";

/** 忙时排队的单会话上限（借鉴 grok-app send queue） */
export const SEND_QUEUE_MAX = 10;

export type StallInfo = { title: string; silentSecs: number };

type SessionState = {
  live: LiveSession[];
  activeProjectId: string | null;
  activeSessionId: string | null;
  transcripts: Record<string, ChatBlock[]>;
  /** 忙时排队消息（轮次结束后按序自动发送） */
  sendQueue: Record<string, string[]>;
  /** 静默看门狗提示（agent://stall；「继续等待/结束本轮」清除） */
  stall: Record<string, StallInfo>;
  setLive: (live: LiveSession[] | ((prev: LiveSession[]) => LiveSession[])) => void;
  setActiveProjectId: (
    id: string | null | ((prev: string | null) => string | null)
  ) => void;
  setActiveSessionId: (
    id: string | null | ((prev: string | null) => string | null)
  ) => void;
  setTranscripts: (
    t:
      | Record<string, ChatBlock[]>
      | ((prev: Record<string, ChatBlock[]>) => Record<string, ChatBlock[]>)
  ) => void;
  patchLiveSession: (id: string, patch: Partial<LiveSession>) => void;
  upsertLive: (session: LiveSession) => void;
  removeLive: (id: string) => void;
  clearTranscript: (id: string) => void;
  /** 入队；满员返回 -1，否则返回排队后的长度 */
  enqueueSend: (id: string, text: string) => number;
  /** 取出队首（无则 null） */
  takeNextSend: (id: string) => string | null;
  /** 清空队列，返回被丢弃的消息 */
  clearSendQueue: (id: string) => string[];
  setStall: (id: string, info: StallInfo) => void;
  clearStall: (id: string) => void;
};

function apply<T>(cur: T, next: T | ((prev: T) => T)): T {
  return typeof next === "function" ? (next as (p: T) => T)(cur) : next;
}

export const useSessionStore = create<SessionState>((set, get) => ({
  live: [],
  activeProjectId: null,
  activeSessionId: null,
  transcripts: {},
  sendQueue: {},
  stall: {},
  setLive: (live) => set((s) => ({ live: apply(s.live, live) })),
  setActiveProjectId: (v) =>
    set((s) => ({ activeProjectId: apply(s.activeProjectId, v) })),
  setActiveSessionId: (v) =>
    set((s) => ({ activeSessionId: apply(s.activeSessionId, v) })),
  setTranscripts: (t) =>
    set((s) => ({ transcripts: apply(s.transcripts, t) })),
  patchLiveSession: (id, patch) =>
    set((s) => ({
      live: s.live.map((x) => (x.id === id ? { ...x, ...patch } : x)),
    })),
  upsertLive: (session) =>
    set((s) => {
      const i = s.live.findIndex((x) => x.id === session.id);
      if (i < 0) return { live: [...s.live, session] };
      const live = s.live.slice();
      live[i] = { ...live[i]!, ...session };
      return { live };
    }),
  removeLive: (id) =>
    set((s) => ({ live: s.live.filter((x) => x.id !== id) })),
  clearTranscript: (id) =>
    set((s) => {
      const transcripts = { ...s.transcripts };
      delete transcripts[id];
      return { transcripts };
    }),
  enqueueSend: (id, text) => {
    const cur = get().sendQueue[id] ?? [];
    if (cur.length >= SEND_QUEUE_MAX) return -1;
    const next = [...cur, text];
    set((s) => ({ sendQueue: { ...s.sendQueue, [id]: next } }));
    return next.length;
  },
  takeNextSend: (id) => {
    const cur = get().sendQueue[id] ?? [];
    if (cur.length === 0) return null;
    const [head, ...rest] = cur;
    set((s) => {
      const sendQueue = { ...s.sendQueue };
      if (rest.length > 0) sendQueue[id] = rest;
      else delete sendQueue[id];
      return { sendQueue };
    });
    return head ?? null;
  },
  clearSendQueue: (id) => {
    const dropped = get().sendQueue[id] ?? [];
    if (dropped.length > 0) {
      set((s) => {
        const sendQueue = { ...s.sendQueue };
        delete sendQueue[id];
        return { sendQueue };
      });
    }
    return dropped;
  },
  setStall: (id, info) =>
    set((s) => ({ stall: { ...s.stall, [id]: info } })),
  clearStall: (id) =>
    set((s) => {
      if (!(id in s.stall)) return s;
      const stall = { ...s.stall };
      delete stall[id];
      return { stall };
    }),
}));
