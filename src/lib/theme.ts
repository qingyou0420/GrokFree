/** 主题应用：data-theme 属性 + localStorage 首帧防闪变（供 index.html 启动脚本读取） */
export function applyTheme(theme: string) {
  // Default light; only explicit "dark" switches
  const t = theme === "dark" ? "dark" : "light";
  document.documentElement.setAttribute("data-theme", t);
  try {
    localStorage.setItem("grok-theme", t);
  } catch {
    /* ignore */
  }
}
