import { create } from "zustand";
import type { ChatBlock, LiveSession } from "../lib/types";

type SessionState = {
  live: LiveSession[];
  activeProjectId: string | null;
  activeSessionId: string | null;
  transcripts: Record<string, ChatBlock[]>;
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
};

function apply<T>(cur: T, next: T | ((prev: T) => T)): T {
  return typeof next === "function" ? (next as (p: T) => T)(cur) : next;
}

export const useSessionStore = create<SessionState>((set) => ({
  live: [],
  activeProjectId: null,
  activeSessionId: null,
  transcripts: {},
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
}));
