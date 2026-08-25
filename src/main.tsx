import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles/app.css";

// 主题已由 index.html 的首帧前脚本设定（localStorage 缓存 / 跟随系统），
// 这里不再强制覆盖，否则深色用户每次启动都会白闪一下。

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
