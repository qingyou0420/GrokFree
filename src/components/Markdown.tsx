import { memo } from "react";
import ReactMarkdown, { defaultUrlTransform } from "react-markdown";
import remarkGfm from "remark-gfm";
import { splitMarkdownMermaid } from "../lib/mermaidFence";
import { MermaidBlock } from "./MermaidBlock";

function urlTransform(url: string): string {
  if (url.startsWith("data:image/")) return url;
  return defaultUrlTransform(url);
}

/**
 * memo：流式更新时只有最后一条消息在变，历史消息 text 引用不变，
 * 跳过 react-markdown + remark-gfm 的整段重解析（热路径）。
 *
 * mermaid 围栏在进 react-markdown 之前切开，避免 pre/code 结构差异导致图画不出来。
 */
export const Markdown = memo(function Markdown({ text }: { text: string }) {
  const parts = splitMarkdownMermaid(text);
  return (
    <div className="md">
      {parts.map((p, i) =>
        p.type === "mermaid" ? (
          <MermaidBlock key={`m-${i}`} chart={p.body} />
        ) : p.body.trim() ? (
          <ReactMarkdown
            key={`t-${i}`}
            remarkPlugins={[remarkGfm]}
            urlTransform={urlTransform}
          >
            {p.body}
          </ReactMarkdown>
        ) : null
      )}
    </div>
  );
});
