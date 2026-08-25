import { create } from "zustand";
import type { ToastState } from "../lib/types";
import type { TranscriptFilter } from "../components/ChatBlocks";

export type ConfirmState = {
  title: string;
  message: string;
  danger?: boolean;
  confirmLabel?: string;
  onConfirm: () => void | Promise<void>;
};

/** 同屏最多展示的 toast 条数（超出淘汰最旧的非 error） */
export const TOAST_MAX_VISIBLE = 3;

export type ToastItem = ToastState & { id: number };

type UiState = {
  showSettings: boolean;
  showReview: boolean;
  showDashboard: boolean;
  showDiskHistory: boolean;
  topMenuOpen: boolean;
  projectMenuId: string | null;
  sessionMenuId: string | null;
  transcriptFilter: TranscriptFilter;
  toasts: ToastItem[];
  confirm: ConfirmState | null;
  confirmBusy: boolean;
  reviewWidth: number;
  /** 每个会话「从末尾往前」展示的条数上限（未设置时用 prefs.historyInitialVisible） */
  historyTail: Record<string, number>;
  setHistoryTail: (
    v: Record<string, number> | ((prev: Record<string, number>) => Record<string, number>)
  ) => void;
  /** 新建会话选用的智能体档案 id（null = 未从 localStorage 恢复） */
  selectedAgentId: string | null;
  setShowSettings: (v: boolean) => void;
  setShowReview: (v: boolean | ((p: boolean) => boolean)) => void;
  setShowDashboard: (v: boolean) => void;
  setShowDiskHistory: (v: boolean) => void;
  setTopMenuOpen: (v: boolean | ((p: boolean) => boolean)) => void;
  setProjectMenuId: (v: string | null | ((p: string | null) => string | null)) => void;
  setSessionMenuId: (v: string | null | ((p: string | null) => string | null)) => void;
  setTranscriptFilter: (v: TranscriptFilter) => void;
  pushToast: (t: ToastItem) => void;
  dismissToast: (id: number) => void;
  setConfirm: (v: ConfirmState | null) => void;
  setConfirmBusy: (v: boolean) => void;
  setReviewWidth: (v: number | ((p: number) => number)) => void;
  setSelectedAgentId: (id: string) => void;
};

function apply<T>(cur: T, next: T | ((prev: T) => T)): T {
  return typeof next === "function" ? (next as (p: T) => T)(cur) : next;
}

function loadReviewWidth(): number {
  try {
    const n = Number(localStorage.getItem("reviewWidth"));
    return n >= 280 && n <= 640 ? n : 360;
  } catch {
    return 360;
  }
}

export const useUiStore = create<UiState>((set) => ({
  showSettings: false,
  showReview: false,
  showDashboard: false,
  showDiskHistory: false,
  topMenuOpen: false,
  projectMenuId: null,
  sessionMenuId: null,
  transcriptFilter: "all",
  toasts: [],
  confirm: null,
  confirmBusy: false,
  reviewWidth: loadReviewWidth(),
  historyTail: {},
  selectedAgentId: null,
  setHistoryTail: (v) =>
    set((s) => ({ historyTail: apply(s.historyTail, v) })),
  setSelectedAgentId: (selectedAgentId) => set({ selectedAgentId }),
  setShowSettings: (showSettings) => set({ showSettings }),
  setShowReview: (v) => set((s) => ({ showReview: apply(s.showReview, v) })),
  setShowDashboard: (showDashboard) => set({ showDashboard }),
  setShowDiskHistory: (showDiskHistory) => set({ showDiskHistory }),
  setTopMenuOpen: (v) => set((s) => ({ topMenuOpen: apply(s.topMenuOpen, v) })),
  setProjectMenuId: (v) =>
    set((s) => ({ projectMenuId: apply(s.projectMenuId, v) })),
  setSessionMenuId: (v) =>
    set((s) => ({ sessionMenuId: apply(s.sessionMenuId, v) })),
  setTranscriptFilter: (transcriptFilter) => set({ transcriptFilter }),
  pushToast: (t) =>
    set((s) => {
      let toasts = s.toasts;
      if (toasts.length >= TOAST_MAX_VISIBLE) {
        // 满员：优先淘汰最旧的非 error；全是 error 才动最旧的
        const victim = toasts.findIndex((x) => x.kind !== "error");
        const idx = victim >= 0 ? victim : 0;
        toasts = toasts.filter((_, i) => i !== idx);
      }
      return { toasts: [...toasts, t] };
    }),
  dismissToast: (id) =>
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
  setConfirm: (confirm) => set({ confirm }),
  setConfirmBusy: (confirmBusy) => set({ confirmBusy }),
  setReviewWidth: (v) =>
    set((s) => ({ reviewWidth: apply(s.reviewWidth, v) })),
}));
