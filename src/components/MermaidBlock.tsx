import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { mermaidKind, mermaidKindLabel } from "../lib/mermaidFence";
import { renderMermaidChart } from "../lib/mermaidRender";

type View = "diagram" | "source";

export function MermaidBlock({ chart }: { chart: string }) {
  const reactId = useId();
  const bodyRef = useRef<HTMLDivElement>(null);
  const [svg, setSvg] = useState<string | null>(null);
  const [error, setError] = useState(false);
  const [busy, setBusy] = useState(true);
  const [view, setView] = useState<View>("diagram");
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const [theme, setTheme] = useState(
    () => document.documentElement.getAttribute("data-theme") || "light"
  );

  const kind = mermaidKind(chart);
  const label = mermaidKindLabel(kind);

  useEffect(() => {
    const el = document.documentElement;
    const obs = new MutationObserver(() => {
      setTheme(el.getAttribute("data-theme") || "light");
    });
    obs.observe(el, { attributes: true, attributeFilter: ["data-theme"] });
    return () => obs.disconnect();
  }, []);

  useEffect(() => {
    setSvg(null);
    setError(false);
    setBusy(true);
  }, [chart]);

  useEffect(() => {
    let cancelled = false;
    setBusy(true);
    setError(false);
    const t = window.setTimeout(() => {
      void (async () => {
        const result = await renderMermaidChart(chart);
        if (cancelled) return;
        if ("error" in result) {
          setError(true);
          setBusy(false);
          return;
        }
        setSvg(result.svg);
        setError(false);
        setBusy(false);
        requestAnimationFrame(() => {
          if (!cancelled && result.bindFunctions && bodyRef.current) {
            result.bindFunctions(bodyRef.current);
          }
        });
      })();
    }, 280);
    return () => {
      cancelled = true;
      window.clearTimeout(t);
    };
  }, [chart, theme]);

  useEffect(() => {
    if (!expanded) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setExpanded(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [expanded]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(chart);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      /* ignore */
    }
  };

  const showSvg = view === "diagram" && svg && !error;
  const status = error
    ? "还不能画（语法未完成或无效）"
    : busy && !svg
      ? "绘制中…"
      : null;

  const diagram = showSvg ? (
    <div
      ref={bodyRef}
      className="mermaid-diagram"
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  ) : (
    <pre className="mermaid-source">{chart || " "}</pre>
  );

  return (
    <div className="mermaid-card" data-kind={kind}>
      <div className="mermaid-toolbar">
        <span className="mermaid-label">{label}</span>
        {status && <span className="mermaid-status">{status}</span>}
        <span className="spacer" />
        <button
          type="button"
          className="btn sm ghost"
          onClick={() => setView(view === "source" ? "diagram" : "source")}
        >
          {view === "source" ? "图示" : "源码"}
        </button>
        <button type="button" className="btn sm ghost" onClick={() => void copy()}>
          {copied ? "已复制" : "复制"}
        </button>
        <button
          type="button"
          className="btn sm ghost"
          onClick={() => setExpanded(true)}
          disabled={!svg}
        >
          放大
        </button>
      </div>
      {diagram}
      {expanded &&
        svg &&
        createPortal(
          <div
            className="modal-backdrop mermaid-lightbox"
            onClick={() => setExpanded(false)}
            role="presentation"
          >
            <div
              className="modal mermaid-lightbox-modal"
              onClick={(e) => e.stopPropagation()}
              role="dialog"
              aria-modal="true"
              aria-labelledby={`${reactId}-title`}
            >
              <header>
                <span id={`${reactId}-title`}>{label}</span>
                <button
                  type="button"
                  className="icon-btn"
                  onClick={() => setExpanded(false)}
                  title="关闭"
                >
                  ✕
                </button>
              </header>
              <div className="body mermaid-lightbox-body">
                <div
                  className="mermaid-diagram mermaid-diagram-lg"
                  dangerouslySetInnerHTML={{ __html: svg }}
                />
              </div>
            </div>
          </div>,
          document.body
        )}
    </div>
  );
}
