import { useCallback, useEffect, useRef } from "react";
import { useUiStore, type ToastItem } from "../state";

/** info/success 自动消失时长 */
const TOAST_AUTO_MS = 4000;
/** error 停留更久，且带手动关闭按钮 */
const TOAST_ERROR_MS = 8000;

/**
 * 堆叠式 toast 队列（替代旧单例：后到不再覆盖先到）。
 * info/success 4s 自动消失；error 8s 且可手动关——漏看「需要授权」是真损失。
 * 同屏上限由 uiStore.pushToast 淘汰（TOAST_MAX_VISIBLE）。
 */
export function useToast() {
  const pushToast = useUiStore((s) => s.pushToast);
  const dismissToast = useUiStore((s) => s.dismissToast);
  const seq = useRef(0);
  const timers = useRef(new Map<number, number>());

  useEffect(() => {
    const map = timers.current;
    return () => {
      for (const t of map.values()) window.clearTimeout(t);
      map.clear();
    };
  }, []);

  const dismiss = useCallback(
    (id: number) => {
      const t = timers.current.get(id);
      if (t !== undefined) {
        window.clearTimeout(t);
        timers.current.delete(id);
      }
      dismissToast(id);
    },
    [dismissToast]
  );

  const flash = useCallback(
    (
      text: string,
      kind: ToastItem["kind"] = "info",
      sessionId?: string | null
    ) => {
      const id = ++seq.current;
      pushToast({ id, text, kind, sessionId: sessionId ?? null });
      const ttl = kind === "error" ? TOAST_ERROR_MS : TOAST_AUTO_MS;
      const t = window.setTimeout(() => {
        timers.current.delete(id);
        dismissToast(id);
      }, ttl);
      timers.current.set(id, t);
    },
    [pushToast, dismissToast]
  );

  return { flash, dismiss };
}
