import type { GrokEnvironment } from "../lib/types";

export function Onboarding({
  env,
  onRefresh,
  onContinue,
}: {
  env: GrokEnvironment | null;
  onRefresh: () => void;
  onContinue: () => void;
}) {
  const okCli = !!env?.grokExists;
  const okAuth = !!env?.authLoggedIn;
  const ready = okCli && okAuth;

  return (
    <div className="empty-state" style={{ padding: 48 }}>
      <div
        className="logo-mark"
        style={{ width: 48, height: 48, margin: "0 auto 16px", fontSize: 22 }}
      >
        G
      </div>
      <h2>欢迎使用 GrokFree</h2>
      <p>
        轻量界面通过 ACP 驱动本机 <code>grok agent</code>。配置与登录状态与 CLI 共享，位于{" "}
        <code>~/.grok</code>。
      </p>

      <div
        style={{
          textAlign: "left",
          maxWidth: 480,
          margin: "0 auto 20px",
          background: "var(--bg-panel)",
          border: "1px solid var(--border)",
          borderRadius: 12,
          padding: 16,
        }}
      >
        <div style={{ marginBottom: 12 }}>
          <strong>{okCli ? "✓" : "○"} Grok 命令行工具</strong>
          <div className="help" style={{ color: "var(--text-muted)", fontSize: 12, marginTop: 4 }}>
            {env?.grokPath || "—"}
            {env?.grokVersion ? ` · ${env.grokVersion}` : ""}
            {!okCli && (
              <div style={{ marginTop: 6 }}>
                请在 PowerShell 中安装：
                <pre style={{ marginTop: 6, fontSize: 11 }}>
                  irm https://x.ai/cli/install.ps1 | iex
                </pre>
              </div>
            )}
          </div>
        </div>
        <div>
          <strong>{okAuth ? "✓" : "○"} 身份验证</strong>
          <div className="help" style={{ color: "var(--text-muted)", fontSize: 12, marginTop: 4 }}>
            {okAuth
              ? `已登录：${env?.authPath}`
              : "请在终端运行一次 grok 完成登录，或设置环境变量 XAI_API_KEY。"}
          </div>
        </div>
      </div>

      <div style={{ display: "flex", gap: 8, justifyContent: "center" }}>
        <button className="btn" onClick={onRefresh}>
          重新检测
        </button>
        <button className="btn primary" disabled={!ready} onClick={onContinue}>
          {ready ? "进入 GrokFree" : "请先完成上述准备"}
        </button>
      </div>
    </div>
  );
}
