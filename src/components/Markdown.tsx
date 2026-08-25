import { memo } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

/**
 * memo：流式更新时只有最后一条消息在变，历史消息 text 引用不变，
 * 跳过 react-markdown + remark-gfm 的整段重解析（热路径）。
 */
export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="md">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
    </div>
  );
});
