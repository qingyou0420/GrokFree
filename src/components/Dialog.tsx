import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";

export type ConfirmDialogProps = {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
};

/** App-styled confirm dialog (replaces window.confirm). */
export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel = "确定",
  cancelLabel = "取消",
  danger = false,
  busy = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  if (!open) return null;
  return (
    <div className="modal-backdrop" onClick={onCancel} role="presentation">
      <div
        className="modal dialog-sm"
        onClick={(e) => e.stopPropagation()}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-dialog-title"
      >
        <header>
          <span id="confirm-dialog-title">{title}</span>
          <button
            type="button"
            className="icon-btn"
            onClick={onCancel}
            title="关闭"
          >
            ✕
          </button>
        </header>
        <div className="body">
          <p className="dialog-message">{message}</p>
        </div>
        <div className="footer">
          <button type="button" className="btn" onClick={onCancel} disabled={busy}>
            {cancelLabel}
          </button>
          <button
            type="button"
            className={`btn ${danger ? "danger" : "primary"}`}
            onClick={onConfirm}
            disabled={busy}
          >
            {busy ? "处理中…" : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

export type InputDialogProps = {
  open: boolean;
  title: string;
  /** 提交回调；返回 false 表示校验失败保持弹窗 */
  onSubmit: (value: string) => void | Promise<void>;
  onCancel: () => void;
  initialValue?: string;
  placeholder?: string;
  confirmLabel?: string;
  busy?: boolean;
};

/** App-styled single-line input dialog (replaces window.prompt — WebView 支持不可靠)。Enter 提交，Esc 取消。 */
export function InputDialog({
  open,
  title,
  onSubmit,
  onCancel,
  initialValue = "",
  placeholder,
  confirmLabel = "确定",
  busy = false,
}: InputDialogProps) {
  const [value, setValue] = useState(initialValue);
  const inputRef = useRef<HTMLInputElement>(null);

  // 打开时同步初值并聚焦全选（busy 提交失败保持弹窗时不清空）
  useEffect(() => {
    if (!open) return;
    setValue(initialValue);
    const t = window.setTimeout(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    }, 0);
    return () => window.clearTimeout(t);
  }, [open, initialValue]);

  if (!open) return null;
  return (
    <div className="modal-backdrop" onClick={onCancel} role="presentation">
      <div
        className="modal dialog-sm"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="input-dialog-title"
      >
        <header>
          <span id="input-dialog-title">{title}</span>
          <button
            type="button"
            className="icon-btn"
            onClick={onCancel}
            title="关闭"
          >
            ✕
          </button>
        </header>
        <div className="body">
          <input
            ref={inputRef}
            className="dialog-input"
            value={value}
            placeholder={placeholder}
            disabled={busy}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !busy) {
                e.preventDefault();
                void onSubmit(value);
              } else if (e.key === "Escape") {
                e.preventDefault();
                onCancel();
              }
            }}
          />
        </div>
        <div className="footer">
          <button type="button" className="btn" onClick={onCancel} disabled={busy}>
            取消
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={busy || !value.trim()}
            onClick={() => void onSubmit(value)}
          >
            {busy ? "处理中…" : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

export type MenuItem = {
  id: string;
  label: string;
  danger?: boolean;
  disabled?: boolean;
  onSelect: () => void;
};

type MenuCoords = {
  top: number;
  left: number;
  right: number;
};

/**
 * Anchored dropdown. Portaled to document.body with position:fixed so it is not
 * trapped under .chat-scroll / topbar backdrop-filter stacking contexts.
 * Place as a child of .menu-shell next to the trigger button.
 */
export function OverflowMenu({
  open,
  onClose,
  items,
  align = "right",
}: {
  open: boolean;
  onClose: () => void;
  items: MenuItem[];
  align?: "left" | "right";
}) {
  const markerRef = useRef<HTMLSpanElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const [coords, setCoords] = useState<MenuCoords | null>(null);

  const focusItem = (dir: 1 | -1 | "first" | "last") => {
    const menu = listRef.current;
    if (!menu) return;
    const items = Array.from(
      menu.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')
    ).filter((b) => !b.disabled);
    if (items.length === 0) return;
    const idx = items.indexOf(document.activeElement as HTMLButtonElement);
    let next: number;
    if (dir === "first") next = 0;
    else if (dir === "last") next = items.length - 1;
    else if (idx < 0) next = dir === 1 ? 0 : items.length - 1;
    else next = (idx + dir + items.length) % items.length;
    items[next]?.focus();
  };

  // 打开即聚焦首项：键盘用户不需要先 Tab 定位菜单
  useLayoutEffect(() => {
    if (!open) return;
    const t = window.setTimeout(() => focusItem("first"), 0);
    return () => window.clearTimeout(t);
  }, [open, items]);

  useLayoutEffect(() => {
    if (!open) {
      setCoords(null);
      return;
    }
    const measure = () => {
      const shell = markerRef.current?.closest(".menu-shell") as HTMLElement | null;
      const el = shell ?? markerRef.current?.parentElement;
      if (!el) return;
      const r = el.getBoundingClientRect();
      setCoords({
        top: r.bottom + 4,
        left: r.left,
        right: window.innerWidth - r.right,
      });
    };
    measure();
    window.addEventListener("resize", measure);
    window.addEventListener("scroll", measure, true);
    return () => {
      window.removeEventListener("resize", measure);
      window.removeEventListener("scroll", measure, true);
    };
  }, [open]);

  // Escape to close
  useLayoutEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  const menuStyle: CSSProperties = coords
    ? {
        position: "fixed",
        top: coords.top,
        ...(align === "right"
          ? { right: coords.right, left: "auto" }
          : { left: coords.left, right: "auto" }),
      }
    : {
        // avoid flash at 0,0 before measure
        position: "fixed",
        top: -9999,
        left: -9999,
        visibility: "hidden",
      };

  const layer = (
    <>
      <div className="menu-backdrop" onClick={onClose} aria-hidden />
      <div
        ref={listRef}
        className={`overflow-menu overflow-menu-portal align-${align}`}
        style={menuStyle}
        role="menu"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" || e.key === "Tab") {
            e.preventDefault();
            focusItem(1);
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            focusItem(-1);
          } else if (e.key === "Home") {
            e.preventDefault();
            focusItem("first");
          } else if (e.key === "End") {
            e.preventDefault();
            focusItem("last");
          }
        }}
      >
        {items.map((item) => (
          <button
            key={item.id}
            type="button"
            role="menuitem"
            className={`overflow-menu-item ${item.danger ? "danger" : ""}`}
            disabled={item.disabled}
            onClick={() => {
              if (item.disabled) return;
              item.onSelect();
              onClose();
            }}
          >
            {item.label}
          </button>
        ))}
      </div>
    </>
  );

  return (
    <>
      {/* Anchor marker stays in .menu-shell for getBoundingClientRect */}
      <span ref={markerRef} className="menu-anchor-marker" aria-hidden />
      {createPortal(layer, document.body)}
    </>
  );
}

export function MenuShell({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  return <div className={`menu-shell ${className}`}>{children}</div>;
}
