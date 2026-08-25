# GrokFree

Windows 桌面客户端，通过 ACP 驱动本机 **Grok CLI**（`grok agent`）。

- **Tauri 2 + React + TypeScript + Rust**
- 每个会话一个 `grok agent` 进程（Supervisor）
- 与 CLI 共享 `%USERPROFILE%\.grok` 登录与配置
- 桌面状态在 `%LOCALAPPDATA%\GrokFree`
- 三栏：项目/会话 · 对话 · 审查
- 引导、权限、设置、托盘、单实例
- **一键云端更新**（从 GitHub Releases 下载安装包并启动）

## 安装

从 [Releases](https://github.com/qingyou0420/GrokFree/releases) 下载 `GrokFree_*_x64-setup.exe`。

需要已安装 [Grok CLI](https://x.ai/grok) 并完成登录：

```powershell
irm https://x.ai/cli/install.ps1 | iex
```

## 更新

侧栏出现「更新」按钮，或 **设置 → 关于 → 检查更新 / 一键更新**。

应用会查询 GitHub Releases 的最新版，下载 NSIS 安装包并启动向导。若安装目录需要管理员权限，请在 UAC 提示时选择「是」。

## 开发

前置：Rust（MSVC）、Node.js 20+、WebView2、Grok CLI。

```powershell
npm install
npm run tauri:dev
```

打包：

```powershell
npm run tauri:build
```

产物在 `src-tauri\target\release\bundle\nsis\`。

发版（触发 GitHub Actions 构建并上传安装包）：

```powershell
git tag v0.9.0
git push origin v0.9.0
```

## 快捷键

| 键 | 操作 |
|----|------|
| Ctrl+N | 新建会话 |
| Ctrl+B | 切换审查面板 |
| Ctrl+D | 总览 |
| Ctrl+, | 设置 |
| Ctrl+1..9 | 切换最近会话 |
| Enter | 发送（Shift+Enter 换行） |

## 测试

```powershell
npm run typecheck
npm test
cd src-tauri
cargo test --lib
```
