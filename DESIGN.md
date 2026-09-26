# GrokFree 现状（0.9.7）

个人 Windows 工作台。Tauri 2 + React + Rust，通过 ACP 驱动本机 Grok CLI。每个会话一个 Agent 进程。登录与 CLI 配置在 `%USERPROFILE%\.grok`，桌面状态在 `%LOCALAPPDATA%\GrokFree`。

霜月陪伴已在 0.9.0 删除。GLM / DeepSeek 只是默认关闭的档案，磁盘恢复只保证 Grok。

## 已经在用的行为

- 发送不锁全局界面。创建只锁当前项目，恢复只锁当前会话。
- 进程上限 8，空闲 30 分钟回收，运行中和等待授权不回收。
- 流静默只提示，不自动取消。长任务没有 600 秒墙钟超时。
- 本会话内允许、忙时排队、轮次租约、自有会话日志（CLI 历史只是兜底）。
- 进程意外退出后自动恢复并发送「继续」，最多 2 次。设置里可以关掉。关掉或续跑失败时才丢弃排队消息。
- GitHub Releases 一键更新。安装包是 NSIS，不是 MSIX。
- 本地控制口只绑 `127.0.0.1`，token 在 `control.json`。本机任意进程读到该文件就能驱动 Agent。

## 控制口

| 方法 | 路径 | 作用 |
|---|---|---|
| GET | `/health` `/projects` `/sessions` `/sessions/:id` | 健康、项目、会话。`:id` 含是否在跑、错误、待授权 |
| POST | `/prompt` | `text` 必填。有 `sessionId` 就发给该会话；否则 `project` 必填，只复用同项目的 idle。忙则 409 |
| POST | `/permission` | `{sessionId, allow, remember?}` 应答当前挂起的授权 |
| POST | `/cancel` | `{sessionId}` |

派发成功后界面会收到用户气泡。失败会补一条系统错误，而不是只写日志。

## 明确不做

不恢复霜月。不把其他模型做成和 Grok 对等的磁盘恢复。不写 CLI 的权限配置。不做 MSIX、winget、内嵌终端、Git worktree、会话 fork、虚拟列表、原生沙箱。

`docs/迭代方案.md` 和 2026-08 的三份 UI 审查描述的是更早的代码（含已删除的霜月），不能当路线图。
