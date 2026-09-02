import { describe, expect, it } from "vitest";
import {
  isMermaidLang,
  mermaidKind,
  mermaidKindLabel,
  splitMarkdownMermaid,
} from "./mermaidFence";

describe("isMermaidLang", () => {
  it("matches fenced mermaid class names", () => {
    expect(isMermaidLang("language-mermaid")).toBe(true);
    expect(isMermaidLang("language-mermaid extra")).toBe(true);
    expect(isMermaidLang("language-js")).toBe(false);
    expect(isMermaidLang("mermaid")).toBe(false);
    expect(isMermaidLang(undefined)).toBe(false);
  });
});

describe("mermaidKind", () => {
  it("reads the first directive", () => {
    expect(mermaidKind("mindmap\n  root((x))")).toBe("mindmap");
    expect(mermaidKind("flowchart TD\n  A --> B")).toBe("flowchart");
    expect(mermaidKind("sequenceDiagram\n  A->>B: hi")).toBe("sequenceDiagram");
  });

  it("skips blanks, comments, and frontmatter", () => {
    expect(
      mermaidKind("%% comment\n\nmindmap\n  root((x))")
    ).toBe("mindmap");
    expect(
      mermaidKind("---\ntitle: Demo\n---\nflowchart LR\n  A --> B")
    ).toBe("flowchart");
  });

  it("maps kind to a Chinese label", () => {
    expect(mermaidKindLabel("mindmap")).toBe("脑图");
    expect(mermaidKindLabel("flowchart")).toBe("流程图");
    expect(mermaidKindLabel("unknownType")).toBe("图示");
  });
});

describe("splitMarkdownMermaid", () => {
  it("leaves plain markdown as a single part", () => {
    expect(splitMarkdownMermaid("hello **x**")).toEqual([
      { type: "md", body: "hello **x**" },
    ]);
  });

  it("extracts a closed mermaid fence", () => {
    const text = "见下图\n\n```mermaid\nmindmap\n  root((x))\n```\n完";
    expect(splitMarkdownMermaid(text)).toEqual([
      { type: "md", body: "见下图\n\n" },
      { type: "mermaid", body: "mindmap\n  root((x))" },
      { type: "md", body: "\n完" },
    ]);
  });

  it("treats an unclosed fence as mermaid (streaming)", () => {
    const text = "```mermaid\nflowchart TD\n  A --> B";
    expect(splitMarkdownMermaid(text)).toEqual([
      { type: "mermaid", body: "flowchart TD\n  A --> B" },
    ]);
  });
});
