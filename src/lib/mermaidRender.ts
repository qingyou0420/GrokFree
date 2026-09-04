import type { Mermaid, RenderResult } from "mermaid";
import { mermaidKind } from "./mermaidFence";

let mermaidMod: Mermaid | null = null;
let initTheme: string | null = null;
let seq = 0;

function themeName(): "dark" | "default" {
  return document.documentElement.getAttribute("data-theme") === "dark"
    ? "dark"
    : "default";
}

function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/**
 * 用 mermaid.min.js 整包，避免 mermaid.core 的动态 import()
 * 在 Tauri 自定义协议下加载 mindmap 等分包失败。
 */
async function getMermaid(): Promise<Mermaid> {
  if (!mermaidMod) {
    const mod = (await import("mermaid/dist/mermaid.min.js")) as {
      default?: Mermaid;
    } & Mermaid;
    mermaidMod = mod.default ?? mod;
  }
  const theme = themeName();
  if (initTheme !== theme) {
    const font = cssVar("--font") || "Segoe UI, system-ui, sans-serif";
    mermaidMod.initialize({
      startOnLoad: false,
      securityLevel: "loose",
      suppressErrorRendering: true,
      theme,
      fontFamily: font,
      flowchart: { htmlLabels: false, curve: "basis", useMaxWidth: true },
      sequence: { useMaxWidth: true },
      mindmap: { useMaxWidth: true, padding: 12 },
      themeVariables: {
        fontFamily: font,
        background: "transparent",
      },
    });
    initTheme = theme;
  }
  return mermaidMod;
}

export type MermaidOk = RenderResult & { kind: string };
export type MermaidFail = { error: string; kind: string };

export async function renderMermaidChart(
  chart: string
): Promise<MermaidOk | MermaidFail> {
  const kind = mermaidKind(chart);
  const trimmed = chart.trim();
  if (!trimmed) return { error: "empty", kind };

  const mermaid = await getMermaid();
  const id = `gf-mmd-${++seq}`;
  try {
    // 不要先 parse 再 render：mindmap 在部分 WebView 里 parse 会误报失败。
    const result = await mermaid.render(id, trimmed);
    if (!result?.svg) return { error: "empty-svg", kind };
    return { ...result, kind };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return { error: message || "parse", kind };
  } finally {
    document.getElementById(id)?.remove();
    document.getElementById(`d${id}`)?.remove();
  }
}
