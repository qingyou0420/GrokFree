import { useCallback, useState, type Dispatch, type SetStateAction } from "react";
import { api } from "../lib/api";
import { useSessionStore } from "../state";

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
 */
export function useComposerActions(opts: {
  setBusy: Dispatch<SetStateAction<boolean>>;
  stickToBottom: () => void;
  scrollToBottom: (force?: boolean) => void;
  flash: Flash;
}) {
  const { setBusy, stickToBottom, scrollToBottom, flash } = opts;
  const [input, setInput] = useState("");
  const setTranscripts = useSessionStore((s) => s.setTranscripts);

  const sendPrompt = useCallback(async () => {
    const text = input.trim();
    const activeSessionId = useSessionStore.getState().activeSessionId;
    if (!text || !activeSessionId) return;
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
    setBusy(true);
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
    } finally {
      setBusy(false);
    }
  }, [input, stickToBottom, setTranscripts, scrollToBottom, setBusy, flash]);

  return {
    input,
    setInput,
    sendPrompt,
  };
}
