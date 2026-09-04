declare module "mermaid/dist/mermaid.min.js" {
  import type mermaid from "mermaid";
  const bundled: typeof mermaid;
  export default bundled;
}
