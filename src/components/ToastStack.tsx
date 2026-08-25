import { useUiStore, type ToastItem } from "../state";

/** 右下角堆叠 toast：带会话的可点击跳转，error 带手动关闭 */
export function ToastStack({
  onFocusSession,
}: {
  onFocusSession: (id: string) => void;
}) {
  const toasts = useUiStore((s) => s.toasts);
  const dismiss = useUiStore((s) => s.dismissToast);
  if (toasts.length === 0) return null;
  return (
    <div className="toast-stack">
      {toasts.map((t) => (
        <ToastRow key={t.id} toast={t} onDismiss={dismiss} onFocusSession={onFocusSession} />
      ))}
    </div>
  );
}

function ToastRow({
  toast,
  onDismiss,
  onFocusSession,
}: {
  toast: ToastItem;
  onDismiss: (id: number) => void;
  onFocusSession: (id: string) => void;
}) {
  return (
    <div
      className={`toast ${toast.kind} ${toast.sessionId ? "clickable" : ""}`}
      onClick={() => {
        if (toast.sessionId) {
          onFocusSession(toast.sessionId);
          onDismiss(toast.id);
        }
      }}
      title={toast.sessionId ? "点击跳转到会话" : undefined}
    >
      <span className="toast-text">{toast.text}</span>
      <button
        type="button"
        className="toast-close"
        title="关闭"
        aria-label="关闭提示"
        onClick={(e) => {
          e.stopPropagation();
          onDismiss(toast.id);
        }}
      >
        ✕
      </button>
    </div>
  );
}
