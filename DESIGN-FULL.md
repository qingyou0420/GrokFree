# Grok Build Desktop — Windows 原生桌面客户端设计文档

| 字段 | 值 |
|------|-----|
| **产品名（建议）** | **Grok Build Desktop**（简称 *Grok Desktop*；安装目录 / 进程名：`GrokBuild.exe`） |
| **文档版本** | 1.0 |
| **状态** | Implementation-ready draft |
| **目标平台** | Windows 10 21H2+ / Windows 11（x64 优先；arm64 跟进） |
| **对标** | OpenAI Codex / ChatGPT Windows 桌面端（体验对标，非抄袭） |
| **核心原则** | Thin client + thick agent；复用 `grok agent` + ACP；与 `~/.grok` 兼容 |

---

## 1. 问题陈述

### 1.1 现状

Grok Build（亦写作 Grok / SpaceXAI CLI）已是功能完备的 **终端 TUI AI 编程助手**：

- 理解代码库、执行 shell、编辑文件、搜索 web、管理任务
- 入口：交互 TUI（`grok`）、headless（`grok -p`）、**Agent Client Protocol (ACP)**
- 安装：`irm https://x.ai/cli/install.ps1 | iex` → `%USERPROFILE%\.grok\bin`
- 认证：浏览器登录 grok.com → `~/.grok/auth.json`，或 `XAI_API_KEY`
- 配置：`~/.grok/config.toml`（MCP、skills、plugins、hooks、themes）
- 会话：磁盘 session 历史，可 resume
- Agent：`grok agent stdio` / `grok agent serve --bind 127.0.0.1:2419`（JSON-RPC ACP）
- 权限：ask / auto / acceptEdits / always-approve 等
- 功能面：Dashboard 多 agent、subagents、plan mode、workflows、monitors、diff/edits 等

**缺口**：Windows 开发者缺少一个 **原生桌面指挥中心**——可并行管理多项目 / 多 agent、可视化 tool 调用与 diff review、系统托盘与通知、权限批准流，同时不牺牲现有 CLI/TUI 路径。

### 1.2 对标产品已证明的需求

Codex Windows 桌面端定位为 **agent 指挥中心**，而非纯聊天窗：

- 多 agent 并行，按 project 组织 threads
- 长任务管理、统一 diff/review
- 原生 PowerShell agent + Windows sandbox（restricted tokens、FS ACL、sandbox user）；可选 WSL2
- UI：project sidebar + active chat + review pane
- 与 CLI 共享 home 目录配置
- winget / Microsoft Store 分发

Grok Build Desktop 应在同等体验维度上竞争，同时 **底层复用 Grok agent 内核**，避免重写 tool loop。

### 1.3 成功标准（产品层）

| ID | 标准 |
|----|------|
| S1 | 用户可在 5 分钟内完成安装 → 登录 → 打开已有项目 → 发出第一条 prompt 并看到流式回复 |
| S2 | 同时运行 ≥3 个 project/session agent，切换不丢上下文 |
| S3 | Tool call、permission prompt、diff 在 UI 中可审阅与批准 |
| S4 | 与 `~/.grok` 的 config / auth / sessions / skills / MCP **共享**，CLI 与 Desktop 可互通 |
| S5 | 默认安全：ask + workspace sandbox；always-approve 需显式开启 |
| S6 | 安装包 < 50MB（不含 CLI 二进制）；冷启动 < 2s（目标机：现代 x64 笔记本） |

---

## 2. 目标与非目标

### 2.1 目标（Goals）

1. **多项目 / 多 agent 指挥中心**：Projects → Sessions/Agents 层级；并行运行、切换、挂起。
2. **富 UI**：对话流、tool 可视化、diff review、权限批准、Dashboard 总览。
3. **Thin client**：UI 通过 ACP（stdio 或 localhost serve）驱动 `grok agent`，不在 UI 进程内实现 agent 内核。
4. **Windows 一等公民**：PowerShell、Win 路径、Toast 通知、系统托盘、MSIX/winget 安装、Windows 沙箱策略。
5. **配置兼容**：优先直接读写 `~/.grok`；必要时提供明确迁移与 schema 扩展（`desktop.*` 节）。
6. **可演进**：MVP 可交付，架构预留 native sandbox、内嵌 terminal、git worktree、browser preview。

### 2.2 非目标（Non-Goals）

| 非目标 | 说明 |
|--------|------|
| 重写 agent 内核 | Tool loop、模型调用、skills 执行保留在 `grok` 二进制 |
| 替代 IDE | 不做成 VS Code 分叉；编辑器集成通过「打开外部编辑器」+ 后续 ACP 扩展 |
| 首版 macOS/Linux Desktop | 本文档以 Windows 为硬目标；跨平台仅作为技术栈选择的副作用 |
| 完整 Cloud 多租户后台 | Desktop 以本地 agent 为主；云端 session 同步非 MVP |
| 完整 Browser 自动化套件 | 内嵌 browser preview 为 v1.x；不在 MVP 重做 Playwright 产品 |
| 强制用户放弃 CLI | Desktop 是增强入口，不是唯一入口 |

---

## 3. 用户与用例

### 3.1 角色

| 角色 | 描述 | 典型环境 |
|------|------|----------|
| **Windows 全栈/后端开发者** | 日常 PowerShell、VS/VS Code、本地 repo | Win11 + Git + Node/Python/.NET |
| **多仓库维护者** | 同时改 monorepo + 多个服务 | 多 project 并行 agent |
| **CLI 深度用户** | 已用 `grok` TUI，希望可视化 review | 已有 `~/.grok` 配置 |
| **安全敏感团队开发者** | 需可审计的权限与沙箱 | 企业策略、受限网络 |
| **偶尔使用者** | 安装后点选项目即可用 | 需要 onboarding 向导 |

### 3.2 核心用例（MVP）

| ID | 用例 | 优先级 |
|----|------|--------|
| UC1 | 安装 Desktop，检测/安装 CLI，完成 auth | P0 |
| UC2 | 添加/打开 project（文件夹），创建 session | P0 |
| UC3 | 发送 prompt，流式展示 agent 文本 / reasoning | P0 |
| UC4 | 展示 tool calls（shell、edit、search…），支持 expand 详情 | P0 |
| UC5 | Permission prompt：允许一次 / 会话内 / 拒绝 | P0 |
| UC6 | File edit / unified diff review：Accept / Reject / Open in editor | P0 |
| UC7 | 多 session 并行，侧边栏切换 | P0 |
| UC8 | Resume 历史 session（读磁盘 session 文件） | P0 |
| UC9 | Settings：权限模式、sandbox、主题、默认 shell、MCP 列表（只读+跳转编辑） | P0 |
| UC10 | 系统托盘：显示运行中 agent 数；任务完成 Toast | P1 |
| UC11 | Dashboard：多 agent 状态卡片（running / waiting / error） | P1 |
| UC12 | 从 CLI 已登录状态直接进入（共享 auth.json） | P0 |

### 3.3 后续用例（v1.x）

- Git worktree 隔离并行任务
- 内嵌终端（PowerShell / CMD / Git Bash / WSL）
- 内嵌 browser / file preview
- Scheduled tasks / workflows 可视化
- WSL2 agent runtime 切换
- Native Windows sandbox（restricted token + ACL）
- Plugins / skills 图形管理

### 3.4 关键用户旅程（Happy Path）

```
[安装 winget/MSIX]
    → 首次启动 Onboarding
    → 检测 ~/.grok/bin/grok.exe；缺失则引导安装 CLI
    → 检测 auth.json；缺失则打开浏览器 OAuth / 粘贴 API Key
    → 主界面：侧边栏「添加项目」
    → 选择工作区目录 → session/new (cwd=project)
    → 用户输入 prompt → ACP session/prompt
    → 流式 UI；遇 permission → 模态/内联批准
    → 遇 file edit → Review pane 展示 diff → Accept
    → 任务完成 → Toast + 侧边栏状态 idle
```

---

## 4. 竞品 / 参考分析（Codex Desktop 模式）

### 4.1 Codex 信息架构（抽象复用）

| 区域 | 职责 | Grok Desktop 映射 |
|------|------|-------------------|
| Project sidebar | 项目列表、线程/agent 列表 | **Projects + Sessions** |
| Active chat | 对话、tool 流、permission | **Session Transcript** |
| Review pane | Diff、文件变更、Git | **Review / Changes** |
| Settings | editor、shell、sandbox | **Settings + ~/.grok 联动** |
| Tray / background | 长任务不挡工作 | **System tray + notifications** |

### 4.2 Codex Windows 技术特征（设计启发）

| 特征 | Codex 做法 | Grok Desktop 策略 |
|------|------------|-------------------|
| Agent 运行时 | 原生 PowerShell + 可选 WSL2 | MVP：宿主 PowerShell 环境跑 `grok agent`；v1：WSL 选项 |
| Sandbox | restricted token、FS ACL、sandbox user、command-runner 分层 | MVP：策略层 + CLI 现有 sandbox 模式；v1：原生 sandbox 组件 |
| 配置共享 | `%USERPROFILE%\.codex` | `%USERPROFILE%\.grok` 为唯一真相源 |
| 分发 | winget / Store | MSIX + winget 主路径；可选 NSIS/bootstrap |
| 并行 | project threads + worktrees | Sessions 并行；worktrees 为 v1.x |

### 4.3 差异化（Grok 应强调）

1. **ACP 一等公民**：Desktop 是标准 ACP Client，与其他编辑器集成路径一致。
2. **已有 TUI Dashboard / workflows / monitors 能力** 映射到图形 Dashboard，而不是从零造编排。
3. **xAI / Grok 模型与生态**（skills、MCP、plugins）在 UI 中可发现。
4. **更轻的安装面**：Thin UI + 已装或按需下载的 `grok` CLI，避免「再打包一整套 agent」。

### 4.4 不应照搬

- 不绑定 ChatGPT 账号体系；保留 Grok / API Key 路径。
- 不强制 Store-only 分发。
- 不在首版强推 worktree 隔离（复杂度高，依赖 Git 工作流成熟度）。

---

## 5. 关键决策（Key Decisions）

> 本节汇总架构选型结论；详细论证见后续各章。

| # | 决策 | 选择 | 简要理由 |
|---|------|------|----------|
| D1 | **产品名** | **Grok Build Desktop**（进程 `GrokBuild.exe`，品牌短名 Grok Desktop） | 与 CLI「Grok Build」一致；避免与 grok.com 网页混淆 |
| D2 | **技术栈** | **Tauri 2** + React + TypeScript + Rust core | 包体小、内存低、权限模型清晰；Rust 便于 spawn/管道/沙箱后续；Web UI 适合复杂 transcript/diff |
| D3 | **进程模型** | **UI 进程 + Agent Supervisor（Rust）+ 每 Session 一个 `grok agent stdio` 子进程** | 崩溃隔离；ACP stdio 最简单可靠；Supervisor 统一生命周期 |
| D4 | **通信** | UI ↔ Rust：Tauri commands + events；Rust ↔ agent：**ACP JSON-RPC over stdio** | 不强制首版 `agent serve`；多 session 用多 stdio 进程 |
| D5 | **配置真相源** | **`%USERPROFILE%\.grok`**（config.toml、auth.json、sessions、skills） | 与 CLI 零迁移互通；Desktop 仅增 `desktop.json` 或 config 中 `[desktop]` |
| D6 | **Session 同步** | Desktop 通过 ACP + 直接读 session 索引；写路径以 agent 为准 | 避免双写；UI 缓存可失效 |
| D7 | **权限默认** | **ask** + sandbox **workspace**；always-approve 需二次确认 | 安全默认；对标 Codex 的 least-privilege |
| D8 | **Windows Sandbox MVP** | **策略门控 + 现有 grok sandbox 模式 + 工作区边界提示**；不做完整 restricted-token | 交付速度；原生 sandbox 单列后续 PR 轨 |
| D9 | **分发** | **MSIX 主推 + winget**；自动更新用 Tauri updater 或 MSIX 更新 | Store/winget 友好；企业可侧载 |
| D10 | **内嵌 Terminal / Browser** | **MVP 不做**；v1.x 可选 | 减 scope；外部 Terminal / 浏览器 deep link 足够 |
| D11 | **Git Review** | MVP：**文件 diff 列表 + unified diff**；不内嵌完整 Git GUI | 依赖 agent 产出的 edits；Git 面板 v1.x |
| D12 | **CLI 依赖** | Desktop **不捆绑**完整 agent 重实现；**依赖或引导安装**同版本 `grok` CLI | 单一内核；版本协商在 initialize |
| D13 | **UI 布局** | **三栏**：Sidebar (Projects/Sessions) \| Transcript \| Review（可折叠） | 对标 Codex 指挥中心；Review 可拖拽宽度 |
| D14 | **Leader 进程** | **单 UI 实例**（second-instance 聚焦已有窗口）；多 agent 不共享一个 grok 进程 | 避免全局状态缠死；托盘挂在 UI 实例 |
| D15 | **认证** | 复用 CLI：读 `auth.json`；未登录则 `grok auth` 或内嵌系统浏览器 OAuth | 不自建 token 存储格式 |

---

## 6. 技术栈推荐与权衡

### 6.1 候选对比

| 维度 | **Tauri 2** | Electron | WinUI 3 / .NET |
|------|-------------|----------|----------------|
| 包体 / 内存 | 极小（数 MB 级 UI + 系统 WebView2） | 大（80MB+ Chromium） | 中等，原生 |
| UI 复杂度（chat/diff） | Web 技术成熟 | 同左 | XAML 复杂列表/虚拟滚动成本高 |
| 进程/管道/安全 | Rust 一等 | Node 可行 | C# 可行 |
| 跨平台后续 | 好 | 好 | 差（Windows-only） |
| 招聘 / 生态 | 需少量 Rust | 前端最广 | .NET 桌面人才 |
| Windows 集成 | 插件 + Rust FFI 足够 | 成熟 | **最强原生** |
| 自动更新 / 打包 | Tauri bundler + MSIX | electron-builder | MSIX 优秀 |
| 与 grok（若 Rust/Go）亲和 | 高 | 中 | 中 |

### 6.2 选型结论：**Tauri 2 + React + TypeScript + Rust**

**理由：**

1. Desktop 是 **thin client**，不需要 Electron 级别的 Node 主进程能力。
2. 大量 transcript、markdown、diff 渲染用 React 生态（virtualized list、Shiki/Monaco 只读、streaming）最快。
3. Agent Supervisor、stdio、job object、后续 sandbox 适合放在 **Rust 侧**。
4. 内存与启动时间直接影响「常驻托盘指挥中心」体验。
5. WebView2 在 Win10/11 目标机覆盖良好（可 bootstrap Evergreen）。

**放弃 Electron 的原因**：包体与常驻内存对「多 agent 指挥中心」不友好；安全面更大。  
**放弃纯 WinUI 的原因**：transcript/tool 可视化迭代速度慢；跨平台归零；与 web 组件生态脱节。  
**混合方案（不采用）**：WinUI shell + WebView 内嵌 — 双栈成本高，Tauri 已覆盖该模式。

### 6.3 前端栈（建议锁定）

| 层 | 选择 |
|----|------|
| 框架 | React 19 + TypeScript |
| 构建 | Vite |
| 状态 | Zustand 或 Jotai（session 流式更新密集） |
| 路由 | 轻量：视图状态机，不必重型 router |
| 样式 | Tailwind CSS + 设计 token（暗色默认） |
| Markdown | markdown-it / react-markdown + GFM |
| Diff | `@git-diff-view/react` 或 Monaco diff（只读优先） |
| 列表虚拟化 | `@tanstack/react-virtual` |
| 图标 | Lucide |
| i18n | 首版中英（`i18next`）；默认跟随系统 |

### 6.4 Rust / Tauri 侧 crate 边界（逻辑）

```
grok-desktop/
  src-tauri/
    src/
      main.rs              # app entry, single-instance
      lib.rs
      commands/            # Tauri invoke handlers
      supervisor/          # agent process lifecycle
      acp/                 # JSON-RPC client over stdio
      config/              # ~/.grok read/watch
      auth/
      notifications/
      paths.rs             # Windows path helpers
      update.rs
  ui/                      # React app
```

---

## 7. 架构

### 7.1 逻辑组件图

```
┌─────────────────────────────────────────────────────────────────┐
│                     Grok Build Desktop (UI Process)             │
│  ┌──────────────┐  ┌─────────────────┐  ┌────────────────────┐  │
│  │ React UI     │  │ Tauri IPC Bridge│  │ Tray / Toast / OS  │  │
│  │ Transcript   │◄─┤ commands/events │─►│ deep links         │  │
│  │ Review/Diff  │  └────────┬────────┘  └────────────────────┘  │
│  │ Dashboard    │           │                                   │
│  └──────────────┘           ▼                                   │
│                    ┌────────────────────┐                       │
│                    │  Agent Supervisor  │  (Rust, in-process)   │
│                    │  - SessionRegistry │                       │
│                    │  - Spawn/Kill/Health                       │
│                    │  - Permission broker                       │
│                    │  - Config watcher  │                       │
│                    └─────────┬──────────┘                       │
└──────────────────────────────┼──────────────────────────────────┘
                               │ ACP JSON-RPC (stdio pipes)
          ┌────────────────────┼────────────────────┐
          ▼                    ▼                    ▼
   ┌─────────────┐      ┌─────────────┐      ┌─────────────┐
   │ grok agent  │      │ grok agent  │      │ grok agent  │
   │ stdio #1    │      │ stdio #2    │      │ stdio #N    │
   │ cwd=proj A  │      │ cwd=proj B  │      │ resume ...  │
   └──────┬──────┘      └──────┬──────┘      └──────┬──────┘
          │                    │                    │
          ▼                    ▼                    ▼
   ~/.grok (shared): config.toml, auth.json, sessions/, skills/, ...
```

### 7.2 进程模型（详细）

#### 7.2.1 进程角色

| 进程 | 数量 | 职责 | 崩溃影响 |
|------|------|------|----------|
| **UI + Supervisor** | 1 | 窗口、托盘、配置监视、子进程管理 | 全部 UI 丢失；agent 子进程可被 Job Object 级联终止或策略选择 detach |
| **grok agent stdio** | 0..N（每活跃 session 1） | ACP agent：模型、tools、sandbox 策略执行 | 单 session 失败；Supervisor 标记 error 并提供 Restart |
| **（可选）grok agent serve** | 0..1 | 高级：多 client 或外部 IDE 附着 | 非 MVP 默认 |

#### 7.2.2 为什么「每 Session 一进程」而不是「单 Leader 多 session」

| 方案 | 优点 | 缺点 | 结论 |
|------|------|------|------|
| 单 `grok agent serve` + 多 session | 进程少、可能共享缓存 | 单点故障；ACP serve 并发成熟度依赖 CLI；权限上下文纠缠 | **v1 可选优化** |
| **每 session `grok agent stdio`** | 隔离好、与 ACP 本地子进程模型一致、易杀易起 | 内存随 session 线性增 | **MVP 采用** |
| 每 project 一进程多 session | 折中 | CLI 需支持多 session/stdio 复用 | 待 CLI 能力确认 |

**MVP 决策（D3）**：每 **活跃** session 一个 `grok agent stdio`。空闲 session 可 **hibernate**（杀进程，保留磁盘 session id，resume 时再 spawn）。

#### 7.2.3 Spawn 契约

```text
Command:
  %USERPROFILE%\.grok\bin\grok.exe agent stdio
  # 或 PATH 中的 grok；允许 Settings 覆盖 executable path

Env:
  继承用户环境
  可选：GROK_DESKTOP=1
  可选：XAI_API_KEY（仅当用户选择 env 认证且未用 auth.json）
  NO_COLOR=1 或 FORCE 由 agent 决定 JSON 输出

Working directory:
  Project root（session 绑定）

Stdio:
  stdin/stdout: ACP JSON-RPC（Content-Length 或 newline-delimited — 以 grok agent 实际实现为准，客户端需兼容探测）
  stderr: 日志 tee 到 Desktop 日志目录 + 可选 UI「Agent Log」

Windows Job Object:
  每个 agent 进程加入 Job；UI 退出时 TerminateJobObject（设置可改为「后台继续」——见开放问题）
```

#### 7.2.4 ACP 会话生命周期

```
Supervisor.spawn(session)
  → Child::spawn(grok agent stdio)
  → ACP initialize {
        protocolVersion,
        clientInfo: { name: "grok-build-desktop", version },
        capabilities: { ... }
     }
  → initialize result: agent capabilities, auth status
  → session/new { cwd, mcpServers?, ... }  或 session/load { sessionId }
  → 等待用户 prompt
  → session/prompt { prompt, ... }
  → 流式 notifications: message chunks, tool_call, tool_result, permission_request, reasoning, ...
  → session 结束 / 取消 / 空闲超时 hibernate
```

#### 7.2.5 崩溃恢复

| 故障 | 检测 | 恢复 |
|------|------|------|
| agent 进程 exit ≠ 0 | WaitOn process | UI 标红；展示 stderr tail；按钮 Restart / Resume session |
| agent 无响应 | ping/heartbeat 或 read 超时 | 提示；可 kill + restart |
| UI 崩溃 | 无 | 下次启动读 session 索引；不自动 resume 运行中任务（除非实现 detach 模式） |
| 半截 tool call | agent 侧负责 | UI 显示 interrupted |

**Session 持久化**：以 `~/.grok` sessions 为准；Supervisor 维护 `desktop-session-map.json`（见 §12）仅存 UI 元数据（标题、projectId、pinned、lastViewport）。

### 7.3 IPC 边界

#### 7.3.1 UI → Rust（Tauri commands，示例）

| Command | 说明 |
|---------|------|
| `get_app_state` | 初始化 hydrate |
| `list_projects` / `add_project` / `remove_project` | 项目管理 |
| `list_sessions` / `create_session` / `open_session` / `close_session` | 会话 |
| `send_prompt` | 转发 ACP session/prompt |
| `cancel_prompt` | 取消当前生成 |
| `respond_permission` | 允许/拒绝 tool |
| `respond_diff` | accept/reject file change（若走 client 侧应用） |
| `get_config` / `open_config_file` | 配置 |
| `get_auth_status` / `start_login` | 认证 |
| `set_desktop_prefs` | 仅 Desktop UI 偏好 |
| `reveal_in_explorer` / `open_in_editor` | OS 集成 |
| `hibernate_session` / `restart_agent` | 生命周期 |

#### 7.3.2 Rust → UI（events）

| Event | Payload 概要 |
|-------|----------------|
| `agent://stream` | sessionId + ACP notification 原样或规范化 envelope |
| `agent://state` | sessionId + Running/Idle/WaitingPermission/Error |
| `agent://permission` | 结构化 permission request |
| `agent://diff` | 文件路径 + patch + toolCallId |
| `config://changed` | 文件路径 |
| `auth://changed` | status |
| `notify://toast` | title/body（也可直接 OS toast） |

**规范化 envelope（建议）**：

```json
{
  "sessionId": "uuid",
  "ts": "2026-07-30T12:00:00Z",
  "kind": "tool_call" | "message_delta" | "permission" | "diff" | "reasoning" | "error" | "raw",
  "acp": { }
}
```

UI 优先理解 `kind`；未知时降级 `raw` JSON 折叠展示——保证 CLI 超前 Desktop 时仍可用。

### 7.4 与现有 Grok 能力的映射

| Grok CLI / Agent 能力 | Desktop 呈现 |
|----------------------|--------------|
| session/prompt 流式 | Transcript bubbles + typing |
| tool calls | Tool cards（可展开命令、输出、exit code） |
| permission prompts | 内联 PermissionBar / 模态 |
| reasoning | 可折叠「思考」区块（设置可隐藏） |
| file edits | Review pane diff |
| Dashboard 多 agent | Dashboard 视图 + 侧边栏状态点 |
| subagents | 线程内嵌子卡片 / 次级 session 链接 |
| plan mode | Plan banner + 确认后执行 |
| workflows / monitors | v1.x 列表页；MVP 可链到 CLI 文档 |
| MCP / skills | Settings 只读列表 + 「在配置中编辑」 |
| themes | Desktop 自有主题 + 可选跟随 CLI theme 名 |

---

## 8. ACP 集成规范

### 8.1 角色

- **Client**：Grok Build Desktop（Supervisor）
- **Agent**：`grok agent stdio`（或后续 serve）

### 8.2 必备能力协商（initialize）

Client 声明：

- `clientInfo.name = "grok-build-desktop"`
- 支持的 protocolVersion（取交叉最大）
- filesystem / terminal 由 **agent** 执行；client 负责 **permission UI** 与 **diff presentation**
- 若 ACP 支持 client 端 `fs/read` 等反向调用，Desktop 应实现只读预览路径（可选增强）

Agent 返回：

- auth 需求
- 支持的 session 方法
- 权限模式列表

**版本不匹配**：阻塞主流程，UI 引导更新 CLI 或 Desktop。

### 8.3 方法使用表（MVP）

| 方向 | 方法 / 通知 | Desktop 行为 |
|------|-------------|--------------|
| C→A | `initialize` | 每子进程一次 |
| C→A | `session/new` | 新任务 |
| C→A | `session/load` 或等价 resume | 打开历史 |
| C→A | `session/prompt` | 用户发送 |
| C→A | `session/cancel` | 停止 |
| C→A | permission response | 用户点击 |
| A→C | message/streaming | 追加 transcript |
| A→C | tool_call / tool_result | Tool card |
| A→C | permission request | 阻塞该 session 的输入区，显示批准 UI |
| A→C | 文件变更相关 | 推入 Review store |

> 具体 JSON 字段名以 `grok agent` 当前 ACP 实现与官方 ACP schema 为准；实现时建立 **`acp_adapter` 层** 隔离字段漂移。

### 8.4 Permission 模型（UI 语义）

映射 CLI 权限模式：

| 模式 | UI 默认 | 行为 |
|------|---------|------|
| `ask` | **默认** | 每个敏感 tool 询问 |
| `acceptEdits` | 可选 | 自动接受文件编辑，仍问 shell/网络 |
| `auto` | 可选 | 按 agent 策略自动 |
| `always-approve` | **高级，二次确认** | 全放行；Settings 红字警告 |

单次 permission 响应选项（MVP）：

1. **Allow once**
2. **Allow for session**
3. **Deny**
4. （P1）**Always allow this tool pattern** → 写回 config 或 session policy

### 8.5 扩展点（非破坏）

为 Desktop 预留 **ACP extensions**（自定义 method 前缀 `grok/` 或 `_grok/`）：

| 扩展 | 用途 | 阶段 |
|------|------|------|
| `grok/dashboard_state` | 聚合多 agent 状态 | v1 |
| `grok/diff_bundle` | 批量文件变更 | MVP 可用 tool 事件拼 |
| `grok/open_editor` | 请求 client 打开编辑器 | v1 |
| `grok/notify` | 任务完成通知 | P1 |

扩展必须 **能力协商**；agent 无扩展时 Desktop 降级。

### 8.6 `agent serve` 模式（可选架构）

```
grok agent serve --bind 127.0.0.1:2419
```

- **用途**：外部 IDE 与 Desktop 附着同一 agent；减少进程数
- **安全**：仅 localhost；可选 token 文件 `%USERPROFILE%\.grok\agent-serve.token`
- **MVP**：不默认启用；Settings 高级开关
- **多 session**：依赖 serve 是否支持多 session 并发——需 CLI 文档确认后再切换默认

---

## 9. UI 信息架构与 UX

### 9.1 导航结构

```
App Shell
├── Title bar（自定义：项目名 / session 状态 / 窗口控件）
├── Sidebar（可折叠）
│   ├── Search projects
│   ├── Projects[]
│   │    └── Sessions[]（状态点：● running / ◐ waiting / ○ idle / ! error）
│   ├── Dashboard
│   └── Settings / Account
├── Main: Transcript
│   ├── Plan/Mode banner（plan mode 等）
│   ├── Message list（virtualized）
│   ├── Tool cards / Reasoning
│   ├── Permission inline bar
│   └── Composer（输入框 + 模式 + 附件预留）
├── Right: Review pane（可关闭）
│   ├── Changed files list
│   ├── Diff viewer
│   └── Accept / Reject / Open
├── Status bar
│   ├── CLI version / ACP ok
│   ├── Permission mode / Sandbox mode
│   └── Network / model（若暴露）
└── System tray
    ├── Open Grok Desktop
    ├── Running agents (n)
    └── Quit
```

### 9.2 关键屏幕

#### 9.2.1 Onboarding

1. 欢迎  
2. 检测 CLI（路径、版本 ≥ 最小版本）  
3. 安装引导（运行官方 `install.ps1` 或打开文档）  
4. 登录（浏览器 / API Key）  
5. 可选：导入已有 projects（最近 session 的 cwd 扫描）  

#### 9.2.2 Home / Dashboard

- 卡片：Running agents、Waiting for permission、Recent projects  
- 快捷：「新任务」、打开最近 session  

#### 9.2.3 Project + Session（主工作区）

- 三栏布局（D13）  
- Composer 支持：  
  - 多行输入  
  - 快捷键 `Ctrl+Enter` 发送  
  - 权限模式快速切换（写入 session 或全局 prefs）  
  - `@file` 提及（P1，基于工作区索引）  

#### 9.2.4 Review

- 文件树 + 状态（pending / accepted / rejected）  
- Unified diff；语法高亮  
- **Accept** 行为：若 agent 已写入磁盘，Accept = 确认已阅；若 client 侧暂存，Accept = 应用 patch（**以 agent 实际 edit 语义为准**，适配层统一）  
- Open in preferred editor（`desktop.preferredEditor`：code / devenv / cursor 等）  

#### 9.2.5 Settings

| 分组 | 项 |
|------|-----|
| Account | 登录状态、登出、API Key |
| Agent | grok 可执行路径、默认 permission、默认 sandbox |
| Runtime | PowerShell 路径、可选 WSL（v1）、环境变量透传 |
| Appearance | 主题、字体、是否显示 reasoning |
| Notifications | Toast、声音、仅 permission 时通知 |
| Integrations | MCP 列表（只读）、Open config.toml、Skills 目录 |
| Advanced | always-approve 解锁、agent serve、日志级别、更新通道 |
| About | 版本、CLI 版本、开源许可、诊断导出 |

### 9.3 交互原则

1. **Session 级阻塞**：仅 WaitingPermission 的 session 禁用发送；其他 session 仍可操作。  
2. **长任务**：切换 session 不取消；托盘显示计数。  
3. **危险操作**：删除 project 引用不删磁盘；Reject diff 若文件已改需 agent 回滚或 git checkout（明确文案，避免静默丢数据）。  
4. **可访问性**：键盘可达、对比度、减少动画选项。  
5. **密度**：默认舒适；提供 compact（类 IDE）。

### 9.4 关键快捷键（建议）

| 快捷键 | 动作 |
|--------|------|
| `Ctrl+N` | 当前项目新 session |
| `Ctrl+Shift+N` | 添加 project |
| `Ctrl+Enter` | 发送 |
| `Ctrl+Shift+C` | 取消生成 |
| `Ctrl+B` | 切换 sidebar |
| `Ctrl+Shift+B` | 切换 review |
| `Ctrl+,` | Settings |
| `Ctrl+1..9` | 切换最近 session |

---

## 10. 功能规格：MVP vs 后续

### 10.1 MVP（v0.1 — 可对外 Tech Preview）

| 模块 | 包含 |
|------|------|
| 安装 / 更新 | MSIX 或 bootstrap installer；检查 CLI |
| Auth | 共享 auth.json；登录引导 |
| Projects | 添加文件夹、列表、最近 |
| Sessions | 新建、resume、并行 ≥3、hibernate |
| Transcript | 流式文本、markdown、tool cards、reasoning 折叠 |
| Permissions | Allow once / session / Deny；默认 ask |
| Review | 变更文件列表 + unified diff + open editor |
| Settings | 核心 agent/UI 项；打开 config.toml |
| Tray | 显示/隐藏；退出 |
| 日志 | `%LOCALAPPDATA%\GrokBuild\logs` |
| 兼容 | 读 config.toml、skills 路径、MCP 配置（运行时仍由 agent 加载） |

### 10.2 v0.2（打磨）

- Toast 通知（permission、任务完成）  
- Dashboard 聚合视图  
- Session 搜索与重命名  
- Diff Accept/Reject 与 git 状态提示  
- 更好的错误诊断（一键复制 debug bundle）  
- 深色/浅色完整 token  

### 10.3 v1.0

- Windows **native sandbox** 集成（与 CLI 同步）  
- Preferred editor / 多 shell 设置落地  
- Subagent 可视化  
- Plan mode 一等 UI  
- winget 正式发布  
- 自动更新稳定通道  

### 10.4 v1.x 路线图

| 特性 | 说明 |
|------|------|
| 内嵌 Terminal | 复用 Windows Terminal 组件或 xterm.js + conpty |
| Browser preview | 简单内嵌 WebView 或系统浏览器 |
| Git panel | status / commit message 建议 / worktree |
| Worktrees | 并行任务目录隔离 |
| WSL2 agent | 切换 runtime |
| Workflows / Monitors UI | 对接 CLI 能力 |
| Skills/MCP 图形安装 | 不只是打开 toml |
| Microsoft Store 上架 | 签名与沙箱策略合规 |

### 10.5 明确延后

- 移动端  
- 完整多用户远程团队协作  
- 自研模型路由 UI（除非 CLI 暴露）  

---

## 11. Windows 专项设计

### 11.1 PowerShell 与 Shell

- Agent 默认在 **用户 Windows 环境** 执行；`grok` 自身决定调用 PowerShell 还是其他 shell。  
- Desktop Settings 暴露：  
  - `desktop.defaultShell`: `powershell` | `pwsh` | `cmd` | `gitbash`（**写入 config 或环境，供 agent 读取**——需 CLI 支持配置键；否则文档说明仅影响「Open external terminal」）  
- 路径：全程使用绝对路径；UI 显示可用 `\\?\` 长路径感知；Rust 侧规范化。  
- 编码：UTF-8；与 PowerShell 5.1 交互时注意代码页问题，日志中记录。

### 11.2 安装路径与目录

| 用途 | 路径 |
|------|------|
| CLI 二进制 | `%USERPROFILE%\.grok\bin\grok.exe` |
| 用户配置 | `%USERPROFILE%\.grok\config.toml` |
| Auth | `%USERPROFILE%\.grok\auth.json` |
| Sessions | `%USERPROFILE%\.grok\sessions\`（以实际 CLI 为准） |
| Desktop 安装 | `MSIX` 包目录 或 `%LOCALAPPDATA%\Programs\GrokBuild\` |
| Desktop 状态 | `%LOCALAPPDATA%\GrokBuild\` |
| Desktop 日志 | `%LOCALAPPDATA%\GrokBuild\logs\` |
| Desktop 缓存 | `%LOCALAPPDATA%\GrokBuild\cache\` |

**原则**：密钥与 agent 真相源在 `~/.grok`；UI 状态在 LocalAppData。

### 11.3 通知与托盘

- 托盘图标：空闲 / 运行中 / 需注意（permission）三态。  
- Windows Toast（Windows App SDK / Tauri notification plugin）：  
  - 需用户批准 permission 且窗口非前台  
  - 长任务完成  
- 点击 Toast → 聚焦对应 session。  

### 11.4 单实例与协议

- **单实例**：second launch 通过 named pipe / mutex 激活首实例。  
- **Deep link**（P1）：`grokbuild://open?project=...` 或 `grokbuild://session/{id}`  
- 可选：注册 `grok` 自定义协议与文件关联。  

### 11.5 WSL

- MVP：不依赖 WSL。  
- v1：Settings「Agent Runtime: Windows | WSL2」，spawn `wsl -d <distro> -- grok agent stdio`（路径与 auth 映射复杂，单独立项）。  

### 11.6 外部编辑器与终端

```toml
# 建议写入 config.toml
[desktop]
preferred_editor = "code"  # code | cursor | notepad | custom
preferred_editor_path = ""
preferred_terminal = "wt"  # wt | powershell | cmd
open_diff_external = false
```

`open_in_editor(path, line?)` 实现：  
- VS Code: `code -g path:line`  
- 失败则 `ShellExecute` 默认关联。  

### 11.7 安装器与权限

- 默认 **per-user** 安装，不需 Admin。  
- Native sandbox 的 **setup helper**（若未来需要创建 sandbox 用户 / 驱动能力）可单独 **elevated one-shot**，与主 UI 分离（对标 Codex 的 setup.exe 分层思想）。  

---

## 12. 配置 / 状态 / 存储

### 12.1 共享：`%USERPROFILE%\.grok\config.toml`

Desktop **尊重**现有键（MCP、hooks、themes、permission、sandbox 等）。  
新增建议节（与 CLI 团队协调）：

```toml
[desktop]
# 由 Desktop 读写；CLI 可忽略未知键
window = { width = 1440, height = 900, sidebar_width = 280, review_width = 420 }
theme = "system"          # system | dark | light
show_reasoning = true
hibernate_idle_minutes = 30
start_minimized_to_tray = false
preferred_editor = "code"
preferred_terminal = "wt"
notify_on_permission = true
notify_on_complete = true
cli_path = ""             # empty = default ~/.grok/bin/grok.exe
```

若 CLI 暂不接受未知表：改为  

`%LOCALAPPDATA%\GrokBuild\desktop.toml`  

并在文档中说明 **双文件**策略（**优先合并进 config.toml**）。

### 12.2 Auth

- 只读/触发刷新 `~/.grok/auth.json`  
- 不复制 token 到 LocalAppData  
- 登录：调用 `grok auth login`（子进程）或官方 URL + 回调（若 CLI 支持 device flow）  

### 12.3 Projects 模型（Desktop）

```json
// %LOCALAPPDATA%\GrokBuild\projects.json
{
  "projects": [
    {
      "id": "proj_...",
      "name": "my-app",
      "path": "C:\\src\\my-app",
      "createdAt": "...",
      "lastOpenedAt": "...",
      "pinned": true
    }
  ]
}
```

### 12.4 Session 映射

```json
// %LOCALAPPDATA%\GrokBuild\session-index.json
{
  "sessions": [
    {
      "desktopId": "d_...",
      "grokSessionId": "s_...",
      "projectId": "proj_...",
      "title": "Fix flaky tests",
      "status": "idle",
      "createdAt": "...",
      "updatedAt": "...",
      "agentPid": null
    }
  ]
}
```

- **grokSessionId** 指向磁盘 session；resume 时交给 agent。  
- 标题：用户可编辑；默认可取首条 prompt 截断。  

### 12.5 同步模型（CLI TUI ↔ Desktop）

| 数据 | 写入者 | 同步方式 |
|------|--------|----------|
| config.toml | CLI 或 Desktop 或用户编辑器 | `notify` 文件监视（debounce 300ms） |
| auth.json | CLI auth 流程 | 监视 + 焦点时 refresh |
| sessions 内容 | **仅 agent** | Desktop 不写 transcript 真相；UI 缓存可丢 |
| session 列表 | agent 创建 + Desktop 索引 | 启动时扫描 sessions 目录 reconcile |
| skills/MCP | 用户/CLI | agent 启动时加载；改配置后需 restart agent |
| desktop prefs | Desktop | 本地文件 |

**冲突**：config 外部修改时，若 Settings 脏写，提示「配置已在外部更改，重新加载？」。

### 12.6 与 TUI Dashboard

- TUI Dashboard 与 Desktop Dashboard **不共享运行时进程**。  
- 共享的是 **磁盘 session 与配置**。  
- 若用户在 TUI 与 Desktop 同时跑同一 session：  
  - **MVP 策略**：不支持双写；Desktop 打开 session 前检测 lock 文件（若 CLI 提供）；否则文档警告「避免同时 resume 同一 session」。  
  - **v1**：session lock + 友好错误。  

---

## 13. Windows Sandbox 策略

### 13.1 目标层级（对标 Codex 思想，自主实现）

| 层 | 能力 | 阶段 |
|----|------|------|
| L0 | 无 OS 隔离；依赖 ask 权限 + 用户信任 | 仅 debug |
| **L1 MVP** | **Workspace 边界策略** + CLI `sandbox` 模式（workspace / read-only / strict 的 Windows 尽力实现） + 权限 UI | **MVP** |
| L2 | 网络策略提示 / 代理控制（若 CLI 支持） | v0.2–v1 |
| **L3** | **Restricted token + 文件系统 ACL + 专用 sandbox 用户 + command-runner 子进程** | **v1 原生沙箱** |
| L4 | AppContainer / 更强网络隔离 | 研究 |

### 13.2 MVP（L1）具体行为

1. Desktop 默认设置：`permission = ask`，`sandbox = workspace`（键名对齐 config）。  
2. UI 在 Status bar 显示当前 sandbox；切换需确认。  
3. Agent 执行 shell/edit 时：  
   - 工作区外写操作 → permission 必问（即使 auto 也建议降级）。  
4. 不宣称「OS 级隔离」——文案诚实：「策略隔离 / 权限门控；OS sandbox 开发中」。  
5. 提供「打开工作区只读」会话模板（read-only sandbox）。  

### 13.3 长期原生沙箱（L3）架构草案

对标业界 Windows agent sandbox 分层（概念对齐，独立实现）：

```
grok.exe (agent)
  → (optional) grok-windows-sandbox-setup.exe  [one-time elevated]
  → grok-command-runner.exe  [runs under restricted token]
       principal: sandbox user OR restricted user token
       write_restricted token SIDs
       ACL: workspace RW; others deny write
       network: block by default / allowlist
```

**Desktop 职责**：

- 检测 sandbox 组件是否安装/可用  
- 引导 setup  
- 在 Settings 展示 sandbox health  
- **不在 UI 进程内执行用户命令**  

**实现归属**：**优先放在 CLI/agent 仓库**，Desktop 只消费能力位。避免 UI 与 CLI 两套沙箱。

### 13.4 与 always-approve 的关系

- always-approve **不关闭** OS sandbox（L3 之后）。  
- always-approve 只跳过 **交互批准**。  
- UI 文案必须区分「跳过询问」vs「关闭沙箱」。  

---

## 14. 安全与权限模型

### 14.1 威胁模型（摘要）

| 威胁 | 缓解 |
|------|------|
| 恶意 prompt 诱导删库/泄密 | 默认 ask；sandbox；diff review |
| 供应链：假冒更新 | 签名 MSIX；HTTPS 更新清单；证书钉扎可选 |
| 本地恶意程序读 token | auth.json ACL 保持用户 only；不写世界可读日志 |
| ACP 中间人 | stdio 无网络面；serve 仅 127.0.0.1 + token |
| UI XSS（markdown） | 渲染消毒；禁用危险 HTML；CSP on WebView |
| 命令注入（路径） | Rust 侧 Arg 数组 spawn，禁止拼接 | 

### 14.2 权限 UX 安全细节

- 展示 **可执行命令全文**、cwd、env 增量。  
- 批量「Allow all pending」需额外 confirm。  
- 自动批准规则展示在 Settings 可删除。  

### 14.3 Secret 处理

- Transcript 日志默认 **脱敏** API key 形态。  
- 导出诊断包前扫描警告。  

### 14.4 代码完整性

- `grok.exe` 路径可配置，但显示 **签名/哈希状态**（若有 Authenticode）。  
- 首次自定义 cli_path 黄条警告。  

---

## 15. 可观测性、更新、打包

### 15.1 日志

| 通道 | 内容 | 位置 |
|------|------|------|
| ui.log | 前端错误、IPC | logs/ui-*.log |
| supervisor.log | spawn、ACP 摘要 | logs/supervisor-*.log |
| agent-*.log | 子进程 stderr | logs/agent-{session}.log |
| redact | tokens | 默认开启 |

日志级别：Settings + 环境变量 `GROK_DESKTOP_LOG=debug`。

### 15.2 指标（可选、隐私优先）

- 本地计数：crash、permission 次数、session 数  
- **默认不上报**；opt-in 诊断  

### 15.3 更新

| 通道 | 机制 |
|------|------|
| Desktop | Tauri updater **或** MSIX 自动更新 / winget upgrade |
| CLI | 检测 `grok --version`；提示运行 install.ps1 / `grok update`（若存在） |

版本策略：Desktop 声明 `minCliVersion`；不满足则阻断 agent 功能并引导升级。

### 15.4 打包与分发（D9）

**推荐路径：**

1. **CI 产出**：  
   - `GrokBuild_{version}_x64.msix`  
   - 可选 `.exe` bootstrap（安装 WebView2 + 放权启动）  
2. **winget 清单**：`xAI.GrokBuild` 或 `Grok.GrokBuildDesktop`  
3. **签名**：Authenticode + MSIX 证书  
4. **CLI**：Desktop 检测缺失时：  
   - 按钮「安装 Grok CLI」→ 提升/非提升运行官方脚本  
   - 或文档链接  

**不**在 MVP 强依赖 Microsoft Store 审核；Store 作 v1 目标。

### 15.5 依赖：WebView2

- 安装器检测 Evergreen Runtime；缺失则引导安装。  

---

## 16. 模块边界（给工程师的仓库建议）

### 16.1 建议仓库布局

**方案 A（推荐）**：单仓 monorepo 目录  

```
grok/                        # 现有 CLI 仓（示例）
  desktop/                   # 新
    package.json
    src/                     # React
    src-tauri/
  ...
```

**方案 B**：`grok-desktop` 独立仓，依赖已发布 `grok` 二进制与 ACP schema crate。

### 16.2 内部 crate / 包

| 包 | 职责 |
|----|------|
| `grok-desktop-ui` | React UI |
| `grok-desktop-app` | Tauri/Rust 壳 |
| `grok-acp-client` | 可单测的 ACP 客户端（可抽到共享给其他 IDE） |
| `grok-desktop-config` | 路径、toml/json 读写 |

**共享优先**：`grok-acp-client` 与 CLI 使用同一 schema 定义（Rust crate 或 JSON schema 生成 TS）。

---

## 17. 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| ACP 协议仍快速演进 | UI 频繁破 | adapter 层 + 兼容测试矩阵 |
| CLI 无稳定 stdio 帧格式 | 集成失败 | 早期 spike PR0；冻结一版契约测试 |
| 多 agent 内存涨 | 体验差 | hibernate；限制同时活跃数（默认 5） |
| WebView2 差异 | UI bug | 锁定最低 Runtime；CI 真机烟测 |
| 双端同时写 session | 损坏 | lock / 文档警告 |
| 用户期望 OS sandbox 但 MVP 没有 | 信任危机 | 诚实文案；路线图公开 |
| 签名与企业策略 | 无法安装 | 提供 per-user MSIX；IT 文档 |
| React 大 transcript 卡顿 | 不可用 | 虚拟列表；消息分页；工具输出截断+展开 |
| always-approve 被误开 | 安全事故 | 二次确认 + 启动时 banner |

---

## Open Questions

## 18. 开放问题

| ID | 问题 | 建议默认 | 需谁决策 |
|----|------|----------|----------|
| Q1 | CLI 是否已稳定支持 `session/load` 与磁盘 session 格式文档？ | 以现有 resume 行为为准做适配 | CLI 团队 |
| Q2 | File edit 是 agent 直接写盘还是 client 应用 patch？ | 假设 agent 写盘；Review=审计 | CLI 确认 |
| Q3 | 退出 UI 时是否终止所有 agent？ | 默认终止（Job Object）；设置可「后台继续」 | 产品 |
| Q4 | `[desktop]` 是否并入 config.toml？ | 是 | CLI 配置 owner |
| Q5 | 最小支持 Windows 版本？ | Win10 21H2+ | 产品 |
| Q6 | 品牌最终名 Grok Build Desktop vs Grok Desktop？ | Grok Build Desktop | 市场 |
| Q7 | 是否需要官方 ACP 扩展命名空间注册？ | `_grok/` 前缀 | 协议 owner |
| Q8 | arm64 是否与 x64 同发 MVP？ | MVP x64；arm64 紧随 | 工程 |
| Q9 | 企业代理 / 强制更新通道？ | v1 再做 | 产品 |
| Q10 | 与 JetBrains/VS Code ACP 插件的会话互通？ | 非 MVP；同 serve 模式预留 | 生态 |

---

## 19. 测试策略（实现约束）

| 层 | 内容 |
|----|------|
| 单元 | ACP 帧解析、permission 状态机、session-index reconcile |
| 集成 | mock `grok agent`（固定脚本响应 JSON-RPC） |
| E2E | Playwright 对 WebView 或 tauri driver；烟测 spawn 真 CLI |
| 安全 | markdown XSS 用例；路径穿越 |
| 性能 | 10k 行 transcript 滚动；5 session 并行 |

**Mock agent**：`grok-desktop-fixtures/fake-agent` 可执行文件，用于 CI 无网络、无 API key。

---

## 20. MVP 里程碑（工程视角）

| 里程碑 | 退出标准 |
|--------|----------|
| M0 Spike | stdio ACP initialize + 一轮 prompt 流式显示 |
| M1 Shell | 三栏 UI 骨架 + projects.json |
| M2 Agent UX | tool cards + permission + cancel |
| M3 Review | diff pane + open editor |
| M4 Polish | tray、onboarding、日志、安装包 |
| M5 Preview | 外测；minCliVersion 门禁 |

---

## 21. 附录 A — 状态机（Session）

```
                 create/open
    ┌────┐      spawn ok      ┌─────────┐
    │Init├──────────────────►│  Idle   │
    └────┘                   └────┬────┘
     │ fail                       │ send_prompt
     ▼                            ▼
  ┌──────┐   stream end    ┌───────────┐
  │Error │◄────────────────┤ Running  │
  └──┬───┘   crash         └─────┬─────┘
     │ restart                   │ permission_request
     ▼                           ▼
  ┌──────┐                 ┌─────────────────┐
  │Idle  │◄── respond ─────┤ WaitingPermission│
  └──────┘                 └─────────────────┘
     │ idle timeout
     ▼
  ┌──────────┐
  │Hibernated│  (process killed, id kept)
  └──────────┘
```

---

## 22. 附录 B — 技术栈决策记录（ADR 摘要）

### ADR-001：Tauri 2 而非 Electron

- **Context**：需要常驻、多窗格、低内存指挥中心。  
- **Decision**：Tauri 2。  
- **Consequences**：团队需维护少量 Rust；换取包体与安全模型。  

### ADR-002：每 Session 一 agent 进程

- **Context**：隔离与 ACP stdio 自然模型。  
- **Decision**：每活跃 session 一进程 + hibernate。  
- **Consequences**：内存线性；实现简单。  

### ADR-003：~/.grok 为真相源

- **Context**：CLI 用户已有配置。  
- **Decision**：共享 home；Desktop 状态分开放 LocalAppData。  
- **Consequences**：需文件监视与锁策略。  

### ADR-004：Sandbox 分阶段

- **Context**：完整 Windows sandbox 工程量大。  
- **Decision**：MVP 策略层；原生沙箱随 CLI 交付。  
- **Consequences**：安全文案必须准确。  

---

## Key Decisions

| ID | 决策 | 理由 |
|----|------|------|
| KD1 | 产品名 **Grok Build Desktop** | 与 CLI 品牌一致，安装/进程可识别 |
| KD2 | **Tauri 2 + React + TS + Rust** | 轻量、适合 thin client、Rust 管进程与后续沙箱 |
| KD3 | **Thin client + ACP stdio**，不重写 agent | 单一内核、与生态一致 |
| KD4 | **每活跃 Session 独立 `grok agent stdio` + Supervisor** | 崩溃隔离、生命周期清晰 |
| KD5 | UI 三栏 **Projects/Sessions \| Transcript \| Review** | 对标指挥中心体验，工程可实现 |
| KD6 | **`%USERPROFILE%\.grok` 共享**；UI 状态在 LocalAppData | CLI/Desktop 互通、零迁移 |
| KD7 | 默认 **ask + workspace sandbox**；always-approve 显式 | 安全默认 |
| KD8 | Sandbox **分阶段**：MVP 策略门控，v1 原生 restricted-token 栈（归 CLI） | 可交付 vs 深度安全解耦 |
| KD9 | 分发 **MSIX + winget**，CLI 按需检测安装 | Windows 主流分发；避免双内核打包 |
| KD10 | MVP **不内嵌 Terminal/Browser**；做 diff review + 外开编辑器 | 控制范围，先打通 agent 指挥闭环 |
| KD11 | 单 UI 实例 + 托盘 | 符合指挥中心心智 |
| KD12 | ACP **adapter 层** + 可选 `_grok/` 扩展 | 抗协议漂移 |

---

## PR Plan

> 原则：每个 PR 可独立审查与合并；先契约与骨架，后体验，再打包与安全深化。  
> 假设代码落在 monorepo `desktop/` 下；若独立仓则路径同构。

### PR-00 — Spike: ACP stdio 客户端与假 agent

| 项 | 内容 |
|----|------|
| **标题** | `desktop: ACP stdio client spike + fake-agent fixture` |
| **组件/文件** | `desktop/src-tauri/src/acp/*`, `desktop/fixtures/fake-agent/*`, 最小集成测试 |
| **依赖** | 无 |
| **描述** | 实现 JSON-RPC 读写、initialize、session/new、session/prompt、流式 notification 解析；`fake-agent` 可在 CI 无网运行。冻结一版帧格式测试向量。 |

### PR-01 — 仓库骨架与 Tauri 2 应用壳

| 项 | 内容 |
|----|------|
| **标题** | `desktop: scaffold Tauri 2 + React + TS app shell` |
| **组件/文件** | `desktop/package.json`, `desktop/src/*`, `desktop/src-tauri/*`, CI workflow 草稿 |
| **依赖** | 无（可与 PR-00 并行，合并后接入） |
| **描述** | 创建窗口、暗色空布局、单实例 mutex、基本 logging 到 `%LOCALAPPDATA%\GrokBuild\logs`。 |

### PR-02 — 路径、配置与 auth 探测

| 项 | 内容 |
|----|------|
| **标题** | `desktop: resolve ~/.grok paths, config.toml read, auth status` |
| **组件/文件** | `src-tauri/src/config/*`, `src-tauri/src/auth/*`, `src-tauri/src/paths.rs`, Settings 只读页 |
| **依赖** | PR-01 |
| **描述** | 定位 `grok.exe`、解析/监视 `config.toml` 与 `auth.json`；暴露 Tauri commands；未登录/未安装 CLI 的状态模型。 |

### PR-03 — Agent Supervisor 生命周期

| 项 | 内容 |
|----|------|
| **标题** | `desktop: agent supervisor spawn/kill/hibernate with Job Objects` |
| **组件/文件** | `src-tauri/src/supervisor/*`, 接入 PR-00 `acp` |
| **依赖** | PR-00, PR-02 |
| **描述** | 每 session 子进程、环境与 cwd、stderr 捕获、崩溃检测、hibernate 策略、退出时 Job 终止。 |

### PR-04 — Projects / Sessions 数据层与侧边栏

| 项 | 内容 |
|----|------|
| **标题** | `desktop: projects.json + session-index + sidebar navigation` |
| **组件/文件** | `src/state/*`, `src/components/Sidebar/*`, `src-tauri` project/session commands |
| **依赖** | PR-02 |
| **描述** | 添加/移除 project、创建 session 元数据、状态点 UI；与 grokSessionId 映射。 |

### PR-05 — Transcript 流式 UI

| 项 | 内容 |
|----|------|
| **标题** | `desktop: streaming transcript, markdown, tool cards` |
| **组件/文件** | `src/components/Transcript/*`, event bridge `agent://stream` |
| **依赖** | PR-03, PR-04 |
| **描述** | 虚拟列表、message delta、tool call/result 卡片、reasoning 折叠、取消生成。 |

### PR-06 — Permission 批准流

| 项 | 内容 |
|----|------|
| **标题** | `desktop: permission prompts (once / session / deny)` |
| **组件/文件** | `src/components/Permission/*`, supervisor permission broker, Settings 默认模式 |
| **依赖** | PR-05 |
| **描述** | WaitingPermission 状态机、内联批准条、模式与 CLI 配置对齐；危险模式二次确认。 |

### PR-07 — Review pane 与 diff

| 项 | 内容 |
|----|------|
| **标题** | `desktop: review pane with unified diff and open-in-editor` |
| **组件/文件** | `src/components/Review/*`, diff 依赖, `open_in_editor` command |
| **依赖** | PR-05 |
| **描述** | 从 tool 事件聚合文件变更；展示 diff；Accept/Reject 语义对接 agent；外部编辑器打开。 |

### PR-08 — Onboarding 与 CLI 安装引导

| 项 | 内容 |
|----|------|
| **标题** | `desktop: onboarding wizard (CLI detect/install + login)` |
| **组件/文件** | `src/screens/Onboarding/*`, 调用 install 文档/脚本入口 |
| **依赖** | PR-02, PR-04 |
| **描述** | 首次运行向导；minCliVersion 检查；登录成功后进入主界面。 |

### PR-09 — Dashboard 与多 session 状态

| 项 | 内容 |
|----|------|
| **标题** | `desktop: dashboard for parallel agents overview` |
| **组件/文件** | `src/screens/Dashboard/*`, supervisor 聚合状态 API |
| **依赖** | PR-03, PR-05, PR-06 |
| **描述** | 运行中/等待权限/错误卡片；点击跳转 session；活跃数上限提示。 |

### PR-10 — 托盘与 Windows Toast

| 项 | 内容 |
|----|------|
| **标题** | `desktop: system tray + toast notifications` |
| **组件/文件** | tray 插件配置, `notifications/*`, 图标资源 |
| **依赖** | PR-06, PR-09 |
| **描述** | 托盘三态、显示/退出、permission 与完成 Toast、点击聚焦 session。 |

### PR-11 — Settings 完整页与 desktop 偏好持久化

| 项 | 内容 |
|----|------|
| **标题** | `desktop: settings UI + desktop prefs persistence` |
| **组件/文件** | `src/screens/Settings/*`, `[desktop]` 或 `desktop.toml` 读写 |
| **依赖** | PR-02, PR-06 |
| **描述** | 权限/沙箱/主题/编辑器/通知/cli_path；打开 config.toml；高级 always-approve。 |

### PR-12 — Session resume 与磁盘 reconcile

| 项 | 内容 |
|----|------|
| **标题** | `desktop: resume sessions from ~/.grok + index reconcile` |
| **组件/文件** | supervisor load/resume, session-index reconcile, UI 历史列表 |
| **依赖** | PR-03, PR-04, PR-05 |
| **描述** | 扫描历史 session、resume 流程、损坏/锁冲突错误处理。 |

### PR-13 — 打包 MSIX / 安装器与版本门禁

| 项 | 内容 |
|----|------|
| **标题** | `desktop: MSIX packaging, WebView2 detect, version gates` |
| **组件/文件** | `src-tauri/tauri.conf.json` bundle, 签名文档, `minCliVersion` |
| **依赖** | PR-01 及主功能 PR（建议 PR-08 后） |
| **描述** | 产出可安装包；检测 WebView2；About 页版本信息；更新通道雏形。 |

### PR-14 — winget 清单与自动更新

| 项 | 内容 |
|----|------|
| **标题** | `desktop: winget manifest + auto-update pipeline` |
| **组件/文件** | `dist/winget/*`, updater 配置, CI release |
| **依赖** | PR-13 |
| **描述** | winget 提交清单；Desktop 自动更新；与 CLI 更新提示协同。 |

### PR-15 — 可观测性与诊断导出

| 项 | 内容 |
|----|------|
| **标题** | `desktop: structured logging + diagnostic bundle export` |
| **组件/文件** | logging middleware, `export_diagnostics` command, 脱敏 |
| **依赖** | PR-03, PR-11 |
| **描述** | 统一日志格式；一键导出（日志+版本+配置红acted）便于支持。 |

### PR-16 — E2E 与 fake-agent CI 门禁

| 项 | 内容 |
|----|------|
| **标题** | `desktop: e2e smoke tests with fake-agent in CI` |
| **组件/文件** | `desktop/e2e/*`, CI job |
| **依赖** | PR-00, PR-05, PR-06, PR-08 |
| **描述** | 关键路径自动化：启动→假登录态→prompt→permission→diff 展示。 |

### PR-17 —（v1）Windows native sandbox 集成

| 项 | 内容 |
|----|------|
| **标题** | `desktop: surface CLI native Windows sandbox health & setup` |
| **组件/文件** | Settings sandbox 面板, setup 引导, 能力探测 |
| **依赖** | CLI 侧 sandbox PR；Desktop PR-11 |
| **描述** | 不实现内核沙箱；检测 CLI sandbox 组件、引导 elevated setup、状态展示与文案。 |

### PR-18 —（v1.x）内嵌终端（可选）

| 项 | 内容 |
|----|------|
| **标题** | `desktop: optional embedded terminal (conpty)` |
| **组件/文件** | Terminal pane, conpty 绑定 |
| **依赖** | PR-04 主壳稳定 |
| **描述** | 可选面板；默认关闭；与 agent cwd 对齐。 |

### PR-19 —（v1.x）Git worktree 与增强 Review

| 项 | 内容 |
|----|------|
| **标题** | `desktop: git status hints + worktree-backed sessions` |
| **组件/文件** | Review/Git 组件, project 服务 |
| **依赖** | PR-07；CLI worktree 支持更佳 |
| **描述** | 并行任务目录隔离与 Git 状态提示。 |

### PR 依赖关系（简图）

```
PR-00 ──────────────┐
PR-01 ──► PR-02 ────┼──► PR-03 ──► PR-05 ──► PR-06 ──► PR-09 ──► PR-10
              │     │         │        │
              │     │         └────────┴──► PR-07
              ├──► PR-04 ──────────────────► PR-08
              │         └──► PR-12
              └──► PR-11
PR-05/06/08 ──► PR-16
主功能 + PR-08 ──► PR-13 ──► PR-14
PR-03/11 ──► PR-15
CLI sandbox ──► PR-17
（可选）PR-18, PR-19
```

### 建议合并节奏

1. **Week 1–2**：PR-00, PR-01, PR-02  
2. **Week 3–4**：PR-03, PR-04, PR-05  
3. **Week 5–6**：PR-06, PR-07, PR-08  
4. **Week 7**：PR-09, PR-10, PR-11, PR-12  
5. **Week 8**：PR-13, PR-15, PR-16 → **Tech Preview**  
6. **之后**：PR-14, PR-17+  

---

## 文档修订记录

| 版本 | 日期 | 说明 |
|------|------|------|
| 1.0 | 2026-07-30 | 初版：Windows Grok Build Desktop 实现向设计 |

---

*本文档面向工程师与技术产品：可按 PR Plan 直接拆分开工。协议字段以实现时 `grok agent` / ACP schema 为准，经 `acp_adapter` 隔离。*
