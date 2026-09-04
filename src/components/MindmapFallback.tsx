import type { MindmapNode } from "../lib/mermaidFence";

function Branch({ node }: { node: MindmapNode }) {
  return (
    <div className="mm-branch">
      <div className="mm-node">{node.label}</div>
      {node.children.length > 0 && (
        <div className="mm-kids">
          {node.children.map((c, i) => (
            <Branch key={`${c.label}-${i}`} node={c} />
          ))}
        </div>
      )}
    </div>
  );
}

/** mermaid 引擎失败时的脑图：按缩进大纲画树，不依赖动态分包。 */
export function MindmapFallback({ tree }: { tree: MindmapNode }) {
  return (
    <div className="mindmap-fallback" role="img" aria-label={tree.label}>
      <div className="mm-root">{tree.label}</div>
      {tree.children.length > 0 && (
        <div className="mm-kids mm-kids-root">
          {tree.children.map((c, i) => (
            <Branch key={`${c.label}-${i}`} node={c} />
          ))}
        </div>
      )}
    </div>
  );
}
