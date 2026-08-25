/** Bottom prompt composer: input, send, stop */

type Props = {
  activeSessionId: string | null;
  /** 当前会话的智能体显示名（占位符跟随，多智能体下不再写死 Grok） */
  agentName?: string | null;
  input: string;
  setInput: (v: string) => void;
  busy: boolean;
  statusHint: string;
  showStop: boolean;
  onStop: () => void;
  onSend: () => void;
};

export function Composer({
  activeSessionId,
  agentName,
  input,
  setInput,
  busy,
  statusHint,
  showStop,
  onStop,
  onSend,
}: Props) {
  return (
    <div className="composer-wrap">
      <div className="composer">
        <textarea
          placeholder={
            activeSessionId
              ? `向 ${agentName || "小精灵"} 发送消息… Enter 发送 · Shift+Enter 换行`
              : "请先新建或恢复一个会话"
          }
          value={input}
          disabled={!activeSessionId || busy}
          rows={2}
          onChange={(e) => {
            setInput(e.target.value);
            const ta = e.target;
            ta.style.height = "auto";
            ta.style.height = `${Math.min(ta.scrollHeight, 200)}px`;
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              onSend();
            }
          }}
        />
        <div className="composer-bar">
          <span className="hint">{statusHint}</span>
          <span className="spacer" />
          {showStop && (
            <button
              type="button"
              className="btn sm danger"
              onClick={onStop}
            >
              停止
            </button>
          )}
          <button
            type="button"
            className="btn sm primary"
            disabled={!activeSessionId || !input.trim() || busy}
            onClick={() => void onSend()}
          >
            发送
          </button>
        </div>
      </div>
    </div>
  );
}
