/** 从围栏代码块 className 识别 mermaid，并读出图种。 */

export function isMermaidLang(className: string | undefined | null): boolean {
  return /(?:^|\s)language-mermaid(?:\s|$)/.test(className ?? "");
}

const KIND_LABEL: Record<string, string> = {
  mindmap: "脑图",
  flowchart: "流程图",
  graph: "流程图",
  sequenceDiagram: "时序图",
  classDiagram: "类图",
  erDiagram: "ER 图",
  stateDiagram: "状态图",
  "stateDiagram-v2": "状态图",
  gantt: "甘特图",
  timeline: "时间线",
  gitGraph: "Git 图",
  pie: "饼图",
  journey: "用户旅程",
  quadrantChart: "象限图",
  C4Context: "C4",
  C4Container: "C4",
  C4Component: "C4",
  requirementDiagram: "需求图",
  packet: "包图",
  block: "块图",
  sankey: "桑基图",
  xychart: "XY 图",
  kanban: "看板",
  architecture: "架构图",
};

export function mermaidKind(chart: string): string {
  const lines = chart.split("\n");
  let i = 0;
  while (i < lines.length && !lines[i].trim()) i++;
  if (lines[i]?.trim() === "---") {
    i++;
    while (i < lines.length && lines[i].trim() !== "---") i++;
    i++;
  }
  for (; i < lines.length; i++) {
    const t = lines[i].trim();
    if (!t || t.startsWith("%%")) continue;
    return t.split(/\s+/)[0] || "mermaid";
  }
  return "mermaid";
}

export function mermaidKindLabel(kind: string): string {
  return KIND_LABEL[kind] ?? "图示";
}

export type MarkdownPart =
  | { type: "md"; body: string }
  | { type: "mermaid"; body: string };

/**
 * 自己切开 ```mermaid 围栏，不依赖 react-markdown 的 pre/code 结构。
 * 未闭合的围栏（流式输出中）把余下全文当作图源。
 */
export function splitMarkdownMermaid(text: string): MarkdownPart[] {
  const parts: MarkdownPart[] = [];
  const openRe = /```mermaid[ \t]*\r?\n/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = openRe.exec(text))) {
    if (m.index > last) {
      parts.push({ type: "md", body: text.slice(last, m.index) });
    }
    const start = m.index + m[0].length;
    const close = text.indexOf("```", start);
    if (close === -1) {
      parts.push({ type: "mermaid", body: text.slice(start).replace(/\n$/, "") });
      return parts;
    }
    parts.push({
      type: "mermaid",
      body: text.slice(start, close).replace(/\n$/, ""),
    });
    last = close + 3;
    openRe.lastIndex = last;
  }
  if (last < text.length) parts.push({ type: "md", body: text.slice(last) });
  return parts.length ? parts : [{ type: "md", body: text }];
}
