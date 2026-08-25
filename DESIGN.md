# Implementation status vs DESIGN-FULL.md

Rebuilt **v0.1.0** as Tauri 2 (replacing the Electron prototype). Current: **v0.9.0** (GrokFree; GitHub Releases 一键云端更新).

| Decision | Design | Implemented |
|----------|--------|-------------|
| D1 Product name | GrokFree | Yes (`grokfree.exe`) |
| D2 Stack | Tauri 2 + React + TS + Rust | Yes |
| D3 Process model | 1 agent stdio / session | Yes (Supervisor) |
| D4 ACP over stdio | Yes | Yes |
| D5 `~/.grok` truth | Yes | CLI/auth/config probed; agent uses it |
| D7 Permission default ask | Yes | Yes + UI + always-approve confirm |
| D8 Sandbox L1 | Strategy + CLI flag | Honest: agent flag **disabled** (CLI rejects); note in Settings |
| D9 MSIX + winget | Primary | **NSIS tech-preview** (MSIX deferred to signed release) |
| D10 No terminal/browser | Yes | Yes (external terminal/editor only) |
| D11 Diff review | List + unified | Yes; Accept/Reject + batch + copy patch |
| D12 No bundled agent | Yes | Yes |
| D13 3-column UI | Yes | Yes + resizable Review |
| D14 Single instance | Yes | `tauri-plugin-single-instance` |
| D15 Auth | Shared auth.json | Onboarding checks auth |

## Paths

- Desktop state: `%LOCALAPPDATA%\GrokBuild\desktop-state.json`
- Logs: `%LOCALAPPDATA%\GrokBuild\logs`
- Installer: `src-tauri/target/release/bundle/nsis/`
- One-click local update: scans project `nsis/` + `%LOCALAPPDATA%\GrokBuild\installers`, topbar / 设置→关于

## Fixes (historical)

| Issue | Fix |
|-------|-----|
| Shell tools fail:「无法创建终端」 | ACP `TerminalHost` implemented |
| Window shows「localhost 拒绝连接」 | `custom-protocol` default feature |
| cmd vs PowerShell mismatch | Default interpreter PowerShell (v0.2.1) |
| Blank transcript on resume | Load `chat_history.jsonl`; no silent `session/new` on load fail; sidebar resume resolves disk path |
| Garbled disk titles | Nested UUID + percent-decode cwd |

## v0.2 shipped

- Disk session browser + resume
- Diff Accept / Reject
- Git status strip
- Dashboard multi-session overview
- Toast + OS notification
- Diagnostics export
- Session search, rename, remove meta
- Shortcuts: Ctrl+N / Ctrl+B / Ctrl+D / Ctrl+,

## v0.3 shipped (this iteration — 「可用主力」)

### 包 A — 设置与 CLI 对齐
- Capability probe (`cli_caps`): agent sandbox flag = false（诚实灰显）
- CLI **version gate** on session spawn (`minCliVersion`)
- always-approve **二次确认**
- 浅色 / 深色主题 token
- Settings 分 tab：常规 · Skills/MCP · 关于

### 包 B — 会话与指挥体验
- Toast **点击跳转 session** + 聚焦主窗口
- 托盘 tooltip 三态：空闲 / 运行中 / 需授权（`update_tray_status`）
- Ctrl+1..9 切换最近会话
- 错误态操作条：重启会话 / 日志 / 诊断
- 顶栏「编辑器」打开项目根

### 包 C — Review / Git
- 批量 Accept / Reject
- Git ahead/behind 解析
- 复制补丁
- Review 宽度拖拽 + localStorage 记忆

### 包 D — Agent 能力面（只读）
- Plan banner + plan entries 解析
- Subagent tool 卡片高亮
- 未知 ACP → raw 折叠块
- Skills / MCP 只读列表（扫 `~/.grok` + config.toml）

### 包 E — 版本
- **0.3.0**（package / Cargo / tauri.conf）
- About 页：Desktop + CLI 版本 + 能力标志

### 包 F — 质量
- Rust 单测：semver gate、MCP/skills 解析、git ahead/behind
- 诊断包含 capabilities / cliVersionOk

## v0.4.1 shipped — UI 迭代（浅色默认）

### 体验（0.4.0 + 0.4.1）
- **浅色主题默认**；深色为可选；去掉粉红 danger，统一红/蓝/青绿 token
- **审查栏默认关闭**；顶栏精简（审查 + 新建 + ⋯）
- 侧栏：运行中 / 休眠分段；线性图标；项目/会话 ⋯ 菜单
- 统一 ConfirmDialog；权限弹窗摘要 + 可折叠 JSON
- 空工作区引导；Composer 随输入增高
- 对话：角色条、tool 完成态折叠、筛选工具条
- 审查：变更 / Git 分 tab；路径 basename；grid 对齐拖拽
- 指挥中心：待授权 / 错误优先排序

## v0.3.1 shipped — 会话治理与噪声收敛

### 会话治理
- 侧栏 / 顶栏 / Dashboard **删除会话**（停进程 + 移 meta，不默认删磁盘）
- 磁盘历史：**过滤 ghost 空会话**（num_messages=0、无真实对话）
- 磁盘历史：**仅当前项目**筛选、搜索、**永久删除磁盘目录**
- 侧栏 **清理空/占位会话**（uuid 标题 / 「新会话」）

### Transcript 噪声
- 扩展 silent ACP / xAI 生命周期事件表
- 默认 **隐藏未识别事件**；设置中可开「显示折叠块（调试）」
- raw 块上限，避免刷屏

### 焦点 / 托盘
- 托盘左键 / 「处理待办会话」→ 聚焦主窗口并跳转待授权/错误会话
- Toast 点击、二次启动同样走 `app://focus-session`
- git / version probe：`CREATE_NO_WINDOW` 防侧栏闪窗

## Not yet (v1.0 / v1.x)

- MSIX / winget / auto-update
- Plan / Subagent **一等**创建与控制（非只读）
- Skills / MCP 图形安装
- Windows Job Objects / native L3 sandbox
- Embedded terminal / browser preview
- Git worktree 隔离会话
