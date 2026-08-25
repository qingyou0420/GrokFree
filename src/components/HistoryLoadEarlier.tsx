import type { RefObject } from "react";
import { useUiStore } from "../state";
import {
  HISTORY_EXPAND_CAP,
  HISTORY_LOAD_STEP,
} from "../hooks/useSessionActions";

/**
 * 「加载更早 / 全部展开」。
 * 展开后补偿 scrollTop（保持视口内容不跳）：记录展开前后 scrollHeight 差值。
 */
export function HistoryLoadEarlier(props: {
  sessionId: string;
  total: number;
  shown: number;
  earlierCount: number;
  historyInitial: number;
  scrollRef: RefObject<HTMLDivElement | null>;
}) {
  const { sessionId, total, shown, earlierCount, historyInitial, scrollRef } =
    props;
  const tail = useUiStore((s) => s.historyTail[sessionId] ?? historyInitial);
  const setHistoryTail = useUiStore((s) => s.setHistoryTail);

  const expandBy = (nextLimit: number) => {
    const el = scrollRef.current;
    const prevHeight = el?.scrollHeight ?? 0;
    const prevTop = el?.scrollTop ?? 0;
    setHistoryTail((prev) => ({
      ...prev,
      [sessionId]: Math.min(total, nextLimit),
    }));
    requestAnimationFrame(() => {
      const node = scrollRef.current;
      if (!node) return;
      const delta = node.scrollHeight - prevHeight;
      node.scrollTop = prevTop + delta;
    });
  };

  return (
    <div className="history-load-earlier">
      <span className="help">
        已隐藏更早 {earlierCount} 条 · 当前显示 {shown}/{total}
      </span>
      <button
        type="button"
        className="btn sm"
        onClick={() => expandBy(tail + HISTORY_LOAD_STEP)}
      >
        加载更早（+{HISTORY_LOAD_STEP}）
      </button>
      <button
        type="button"
        className="btn sm ghost"
        onClick={() => expandBy(HISTORY_EXPAND_CAP)}
        title={
          total > HISTORY_EXPAND_CAP
            ? `条数超过 ${HISTORY_EXPAND_CAP}，仅展开最近 ${HISTORY_EXPAND_CAP} 条`
            : undefined
        }
      >
        全部展开
      </button>
    </div>
  );
}
