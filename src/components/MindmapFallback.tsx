import type { MindmapNode } from "../lib/mermaidFence";

type Laid = {
  label: string;
  x: number;
  y: number;
  w: number;
  h: number;
  children: Laid[];
};

const NODE_H = 30;
const GAP_X = 36;
const GAP_Y = 10;
const PAD_X = 12;

function textWidth(s: string): number {
  let w = 0;
  for (const ch of s) w += ch.charCodeAt(0) > 127 ? 14 : 8;
  return w;
}

function nodeW(label: string): number {
  return Math.max(40, textWidth(label) + PAD_X * 2);
}

function layout(node: MindmapNode, x: number, y: number): { laid: Laid; height: number } {
  const w = nodeW(node.label);
  if (node.children.length === 0) {
    return {
      laid: { label: node.label, x, y, w, h: NODE_H, children: [] },
      height: NODE_H,
    };
  }
  let cy = y;
  const kids: Laid[] = [];
  let totalH = 0;
  for (let i = 0; i < node.children.length; i++) {
    if (i) cy += GAP_Y;
    const sub = layout(node.children[i], x + w + GAP_X, cy);
    kids.push(sub.laid);
    cy += sub.height;
    totalH += sub.height + (i ? GAP_Y : 0);
  }
  const h = Math.max(NODE_H, totalH);
  return {
    laid: {
      label: node.label,
      x,
      y: y + (h - NODE_H) / 2,
      w,
      h: NODE_H,
      children: kids,
    },
    height: h,
  };
}

function bounds(n: Laid, acc = { w: 0, h: 0 }): { w: number; h: number } {
  acc.w = Math.max(acc.w, n.x + n.w);
  acc.h = Math.max(acc.h, n.y + n.h);
  for (const c of n.children) bounds(c, acc);
  return acc;
}

function Links({ node }: { node: Laid }) {
  const x1 = node.x + node.w;
  const y1 = node.y + node.h / 2;
  return (
    <>
      {node.children.map((c, i) => {
        const x2 = c.x;
        const y2 = c.y + c.h / 2;
        const mx = (x1 + x2) / 2;
        return (
          <g key={`l-${c.label}-${i}`}>
            <path
              d={`M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`}
              fill="none"
              stroke="currentColor"
              strokeWidth="1.25"
              className="mm-link"
            />
            <Links node={c} />
          </g>
        );
      })}
    </>
  );
}

function Nodes({ node, root }: { node: Laid; root?: boolean }) {
  return (
    <>
      <g>
        <rect
          x={node.x}
          y={node.y}
          width={node.w}
          height={node.h}
          rx={root ? node.h / 2 : 8}
          className={root ? "mm-svg-root" : "mm-svg-node"}
        />
        <text
          x={node.x + node.w / 2}
          y={node.y + node.h / 2}
          textAnchor="middle"
          dominantBaseline="middle"
          className="mm-svg-text"
        >
          {node.label}
        </text>
      </g>
      {node.children.map((c, i) => (
        <Nodes key={`n-${c.label}-${i}`} node={c} />
      ))}
    </>
  );
}

/** 本地 SVG 脑图，不走 mermaid，避免 WebView 里引擎挂死。 */
export function MindmapFallback({ tree }: { tree: MindmapNode }) {
  const { laid, height } = layout(tree, 8, 8);
  const { w, h } = bounds(laid);
  const width = w + 16;
  const vh = Math.max(height, h) + 16;
  return (
    <svg
      className="mindmap-svg"
      viewBox={`0 0 ${width} ${vh}`}
      width={width}
      height={vh}
      role="img"
      aria-label={tree.label}
    >
      <Links node={laid} />
      <Nodes node={laid} root />
    </svg>
  );
}
