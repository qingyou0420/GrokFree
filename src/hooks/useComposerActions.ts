import { useCallback, useState } from "react";
import { api } from "../lib/api";
import { useSessionStore } from "../state";
import { SEND_QUEUE_MAX } from "../state/sessionStore";

type Flash = (
  text: string,
  kind?: "info" | "success" | "error",
  sessionId?: string | null
) => void;

function uid(p: string) {
  return `${p}_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`;
}

/**
 * 输入框状态 + 发送（乐观上屏 user 块，失败补 system 错误块）。
 *
 * 发送**不再持有全局 busy**：一轮对话可能跑几分钟，全局 busy 会把
 * 侧栏「新建/恢复」、项目切换全部锁死（看起来就是"切项目卡死"）。
 * 忙碌状态改为按会话（status running / waiting_permission）门禁。
 */
export function useComposerActions(opts: {
  stickToBottom: () => void;
  scrollToBottom: (force?: boolean) => void;
  flash: Flash;
}) {
  const { stickToBottom, scrollToBottom, flash } = opts;
  const [input, setInput] = useState("");
  const setTranscripts = useSessionStore((s) => s.setTranscripts);

  const sendPrompt = useCallback(async () => {
    const text = input.trim();
    const { activeSessionId, live, enqueueSend } = useSessionStore.getState();
    if (!text || !activeSessionId) return;
    // 忙时排队（借鉴 grok-app send queue）：本会话在忙不再拦下消息，
    // 入队并在本轮结束后自动按序发送。不引入全局 busy。
    const cur = live.find((s) => s.id === activeSessionId);
    if (
      cur &&
      (cur.status === "running" || cur.status === "waiting_permission")
    ) {
      const n = enqueueSend(activeSessionId, text);
      if (n < 0) {
        flash(
          `排队已满（${SEND_QUEUE_MAX} 条），请等待本轮结束`,
          "error",
          activeSessionId
        );
        return;
      }
      setInput("");
      flash(`已排队（${n} 条待发送），本轮结束后自动发送`, "info", activeSessionId);
      return;
    }
    setInput("");
    stickToBottom();
    setTranscripts((prev) => ({
      ...prev,
      [activeSessionId]: [
        ...(prev[activeSessionId] ?? []),
        { kind: "user", id: uid("u"), text },
      ],
    }));
    scrollToBottom(true);
    try {
      await api.sendPrompt(activeSessionId, text);
    } catch (e) {
      flash(`发送失败：${e}`, "error", activeSessionId);
      setTranscripts((prev) => ({
        ...prev,
        [activeSessionId]: [
          ...(prev[activeSessionId] ?? []),
          { kind: "system", id: uid("sys"), text: `错误：${e}` },
        ],
      }));
    }
  }, [input, stickToBottom, setTranscripts, scrollToBottom, flash]);

  return {
    input,
    setInput,
    sendPrompt,
  };
}
