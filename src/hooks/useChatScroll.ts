import { useCallback, useEffect, useMemo, useRef, useState, type MutableRefObject } from "react";
import { useSessionStore } from "../state";
import type { ChatBlock } from "../lib/types";

/** 距底部小于该距离时跟随新消息自动贴底 */
const STICK_BOTTOM_PX = 140;

/**
 * 会话区滚动状态机（自 App.tsx 原样抽取，行为保持快照）：
 * - stickBottom：用户上滑阅读历史时关闭自动贴底，回到底部附近恢复
 * - 切换会话：遮罩 → 两帧布局 → 恢复滚动位置或贴底 → 揭开；2.5s 兜底强揭
 * - 流式跟随：末尾块内容指纹触发，程序化滚动期间忽略 onScroll
 * 改这里之前先读 docs/REVIEW-2026-08-16.md 第五节。
 */
export function useChatScroll(opts: {
  blocks: ChatBlock[];
  activeSessionId: string | null;
  /** activeLive?.status —— 运行中更积极地跟随 */
  activeStatus?: string;
  /** 与 useResumeSession 共用的恢复进行中标记 */
  resumeInFlightRef: MutableRefObject<boolean>;
}) {
  const { blocks, activeSessionId, activeStatus, resumeInFlightRef } = opts;
  const scrollRef = useRef<HTMLDivElement>(null);
  /** 是否贴着底部（用户上滑阅读历史时关闭自动滚底） */
  const stickBottomRef = useRef(true);
  /** 程序化贴底中：忽略 onScroll，避免误关 stickBottom */
  const autoScrollingRef = useRef(false);
  /** 各会话滚动位置（切换时恢复；恢复/新会话仍贴底） */
  const scrollPosBySession = useRef<Record<string, number>>({});
  /** null = 贴底；数字 = 恢复该 scrollTop */
  const pendingScrollRestore = useRef<number | null>(null);
  /** 下次揭开是否强制贴底（resume / create） */
  const forceStickBottomRef = useRef(false);
  /**
   * 会话区是否已完成「贴底 + 布局」，再显示内容，避免打开时闪一下。
   * false = 遮罩遮住中间渲染过程
   */
  const [chatPaintReady, setChatPaintReady] = useState(true);
  const paintTokenRef = useRef(0);

  const scrollToBottom = useCallback((force = false) => {
    const el = scrollRef.current;
    if (!el) return;
    if (!force && !stickBottomRef.current) return;
    autoScrollingRef.current = true;
    stickBottomRef.current = true;
    // 同步贴底 + 再跟两帧，覆盖 markdown/工具块晚到的增高
    el.scrollTop = el.scrollHeight;
    requestAnimationFrame(() => {
      const n1 = scrollRef.current;
      if (n1 && (force || stickBottomRef.current)) {
        n1.scrollTop = n1.scrollHeight;
      }
      requestAnimationFrame(() => {
        const n2 = scrollRef.current;
        if (n2 && (force || stickBottomRef.current)) {
          n2.scrollTop = n2.scrollHeight;
        }
        // 再等一拍再放开，避免浏览器合成滚动事件误判
        window.setTimeout(() => {
          autoScrollingRef.current = false;
        }, 50);
      });
    });
  }, []);

  /** 布局两帧后恢复滚动或贴底，再揭开遮罩 */
  const revealChatAfterPaint = useCallback((sessionId: string | null) => {
    const token = ++paintTokenRef.current;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (token !== paintTokenRef.current) return;
        const node = scrollRef.current;
        if (node) {
          const forceBottom = forceStickBottomRef.current;
          forceStickBottomRef.current = false;
          if (
            !forceBottom &&
            pendingScrollRestore.current != null &&
            pendingScrollRestore.current >= 0
          ) {
            autoScrollingRef.current = true;
            node.scrollTop = pendingScrollRestore.current;
            pendingScrollRestore.current = null;
            const dist =
              node.scrollHeight - node.scrollTop - node.clientHeight;
            stickBottomRef.current = dist <= STICK_BOTTOM_PX;
            window.setTimeout(() => {
              autoScrollingRef.current = false;
            }, 50);
          } else {
            autoScrollingRef.current = true;
            node.scrollTop = node.scrollHeight;
            stickBottomRef.current = true;
            pendingScrollRestore.current = null;
            window.setTimeout(() => {
              autoScrollingRef.current = false;
            }, 50);
          }
        }
        const cur = useSessionStore.getState().activeSessionId;
        if (sessionId && cur !== sessionId) return;
        setChatPaintReady(true);
      });
    });
  }, []);

  const onChatScroll = useCallback(() => {
    if (!chatPaintReady) return;
    // 程序化贴底产生的 scroll 事件不得关掉 stick
    if (autoScrollingRef.current) return;
    const el = scrollRef.current;
    if (!el) return;
    const dist = el.scrollHeight - el.scrollTop - el.clientHeight;
    stickBottomRef.current = dist <= STICK_BOTTOM_PX;
    const sid = useSessionStore.getState().activeSessionId;
    if (sid) {
      scrollPosBySession.current[sid] = el.scrollTop;
    }
  }, [chatPaintReady]);

  /** 新会话 / 恢复会话前调用：下次揭开强制贴底，不恢复旧位置 */
  const beginAtBottom = useCallback(() => {
    forceStickBottomRef.current = true;
    pendingScrollRestore.current = null;
  }, []);

  /** 发送消息后进入本轮自动跟随 */
  const stickToBottom = useCallback(() => {
    stickBottomRef.current = true;
  }, []);

  /**
   * 流式输出多数是「同一条消息原地变长」，blocks.length 不变。
   * 用末尾块内容指纹 + 条数 + 会话状态触发跟随滚动。
   */
  const streamFollowKey = useMemo(() => {
    if (!activeSessionId) return "none";
    const list = blocks;
    const n = list.length;
    const last = n > 0 ? list[n - 1] : null;
    let tail = "empty";
    if (last) {
      switch (last.kind) {
        case "assistant":
        case "user":
        case "system":
        case "thought":
          tail = `${last.kind}:${last.id}:${last.text?.length ?? 0}`;
          break;
        case "tool":
          tail = `tool:${last.id}:${last.status}:${
            typeof last.output === "string"
              ? last.output.length
              : last.output
                ? JSON.stringify(last.output).length
                : 0
          }`;
          break;
        case "diff":
          tail = `diff:${last.id}:${last.patch?.length ?? 0}`;
          break;
        case "plan":
          tail = `plan:${last.id}:${last.text?.length ?? 0}:${last.entries?.length ?? 0}`;
          break;
        default:
          tail = `${(last as { kind: string }).kind}:${(last as { id?: string }).id ?? n}`;
      }
    }
    // 倒数第二块也纳入（助手+工具并行更新时）
    const prev = n > 1 ? list[n - 2] : null;
    let prevKey = "";
    if (prev) {
      if (
        prev.kind === "assistant" ||
        prev.kind === "user" ||
        prev.kind === "system" ||
        prev.kind === "thought"
      ) {
        prevKey = `${prev.kind}:${prev.text?.length ?? 0}`;
      } else if (prev.kind === "tool") {
        prevKey = `tool:${prev.status}:${
          typeof prev.output === "string" ? prev.output.length : 0
        }`;
      } else {
        prevKey = prev.kind;
      }
    }
    return `${activeSessionId}|${n}|${tail}|${prevKey}|${activeStatus ?? ""}`;
  }, [activeSessionId, blocks, activeStatus]);

  // 新内容 / 流式更新：用户贴底时跟随；运行中更积极（仍尊重上滑阅读）
  useEffect(() => {
    if (!chatPaintReady || !activeSessionId) return;
    const running =
      activeStatus === "running" ||
      activeStatus === "starting" ||
      activeStatus === "waiting_permission";
    if (running && stickBottomRef.current) {
      scrollToBottom(true);
    } else {
      scrollToBottom(false);
    }
  }, [streamFollowKey, chatPaintReady, activeSessionId, activeStatus, scrollToBottom]);

  // 切换会话：保存旧滚动位置，遮罩，再揭开（恢复中由 loadTranscript 揭开）
  const prevActiveForScroll = useRef<string | null>(null);
  useEffect(() => {
    const prev = prevActiveForScroll.current;
    if (prev && scrollRef.current && chatPaintReady) {
      scrollPosBySession.current[prev] = scrollRef.current.scrollTop;
    }
    prevActiveForScroll.current = activeSessionId;

    if (!activeSessionId) {
      setChatPaintReady(true);
      return;
    }
    setChatPaintReady(false);
    if (resumeInFlightRef.current || forceStickBottomRef.current) {
      pendingScrollRestore.current = null;
      return;
    }
    const saved = scrollPosBySession.current[activeSessionId];
    pendingScrollRestore.current =
      typeof saved === "number" ? saved : null;
    // 首次打开无记录 → 贴底
    if (pendingScrollRestore.current == null) {
      forceStickBottomRef.current = true;
    }
    revealChatAfterPaint(activeSessionId);
    // chatPaintReady 故意不入依赖：只在「保存旧位置」时读取，
    // 入依赖会在揭开(true)后重跑本 effect 造成遮罩-揭开循环
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeSessionId, revealChatAfterPaint]);

  // 兜底：若遮罩超过 2.5s 仍未揭开（异常路径），强制显示，避免一直挡着
  useEffect(() => {
    if (chatPaintReady || !activeSessionId) return;
    const t = window.setTimeout(() => {
      setChatPaintReady(true);
    }, 2500);
    return () => window.clearTimeout(t);
  }, [chatPaintReady, activeSessionId]);

  return {
    scrollRef,
    onChatScroll,
    chatPaintReady,
    scrollToBottom,
    revealChatAfterPaint,
    beginAtBottom,
    stickToBottom,
  };
}
