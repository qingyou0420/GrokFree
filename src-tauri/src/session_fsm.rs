//! 会话状态机：纯函数 + 单测（借鉴 grok-app session_fsm 的思路）。
//!
//! 状态仍以字符串存在 `LiveSession.status` / `SessionMeta.status`（与前端及
//! 磁盘格式兼容），但**迁移规则和忙碌/可回收判定收拢到这里**——闲置回收、
//! 进程上限、项目切换清扫、静默看门狗共用同一份真值，避免各处散落的
//! `matches!(status, "idle" | …)` 漂移。前端只投影快照，不做状态推导。

/// 会话状态常量（`LiveSession.status` 的合法取值）。
pub mod status {
    pub const IDLE: &str = "idle";
    pub const RUNNING: &str = "running";
    pub const WAITING_PERMISSION: &str = "waiting_permission";
    pub const ERROR: &str = "error";
    pub const HIBERNATED: &str = "hibernated";
    /// 宿主进程在本轮进行中死亡（turn lease 修复标记，仅存在于 meta）。
    pub const INTERRUPTED: &str = "interrupted";
}

/// 会话状态迁移事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// `session/prompt` 已发出
    PromptStart,
    /// prompt RPC 正常返回
    PromptOk,
    /// prompt RPC 报错 / 超时
    PromptErr,
    /// agent 发来 `session/request_permission`
    PermissionRequest,
    /// 用户批准（含「本会话内允许」自动批准）
    PermissionAllow,
    /// 用户拒绝
    PermissionDeny,
    /// agent 进程意外退出
    AgentExited,
    /// 主动休眠（手动 / 闲置回收 / 项目切换清扫 / 容量回收）
    Hibernate,
    /// 静默看门狗自愈：RPC 已结束但状态残留 running
    StallHeal,
}

/// 纯迁移函数：`(当前状态, 事件) -> 新状态`。
/// 未列出的组合保持原状态（迁移无效时不撒谎、不猜）。
pub fn transition(current: &str, ev: Event) -> &'static str {
    use status::*;
    match (current, ev) {
        // 发起新一轮
        (IDLE | ERROR | INTERRUPTED, Event::PromptStart) => RUNNING,
        // 轮次结束
        (RUNNING | WAITING_PERMISSION, Event::PromptOk) => IDLE,
        (RUNNING | WAITING_PERMISSION, Event::PromptErr) => ERROR,
        // 权限流（轮内）
        (RUNNING, Event::PermissionRequest) => WAITING_PERMISSION,
        (WAITING_PERMISSION, Event::PermissionAllow) => RUNNING,
        (WAITING_PERMISSION, Event::PermissionDeny) => IDLE,
        // 进程消亡
        (_, Event::AgentExited) => ERROR,
        (_, Event::Hibernate) => HIBERNATED,
        // 自愈：仅 running 需要拨回
        (RUNNING, Event::StallHeal) => IDLE,
        // 其余组合：保持原状态
        (cur, _) => leak_static(cur),
    }
}

/// 可被回收（闲置休眠 / 容量腾位 / 项目切换清扫）的**活跃**会话状态。
pub fn is_reclaimable(status: &str) -> bool {
    matches!(status, status::IDLE | status::ERROR)
}

/// 启动时 meta 里可能残留的「看起来还活着」状态（宿主重启后进程必然已死）。
pub fn is_stale_live_status(status: &str) -> bool {
    matches!(
        status,
        status::RUNNING | status::WAITING_PERMISSION | "starting"
    )
}

/// 把已知状态字符串映射回 'static（transition 返回值需要）。
/// 未知字符串按 idle 处理——上游只会传入本模块常量。
fn leak_static(s: &str) -> &'static str {
    use status::*;
    match s {
        RUNNING => RUNNING,
        WAITING_PERMISSION => WAITING_PERMISSION,
        ERROR => ERROR,
        HIBERNATED => HIBERNATED,
        INTERRUPTED => INTERRUPTED,
        _ => IDLE,
    }
}

#[cfg(test)]
mod tests {
    use super::status::*;
    use super::*;

    #[test]
    fn prompt_lifecycle() {
        assert_eq!(transition(IDLE, Event::PromptStart), RUNNING);
        assert_eq!(transition(RUNNING, Event::PromptOk), IDLE);
        assert_eq!(transition(RUNNING, Event::PromptErr), ERROR);
        // 出错后可直接重试
        assert_eq!(transition(ERROR, Event::PromptStart), RUNNING);
        // 中断修复后的会话可以直接再发起
        assert_eq!(transition(INTERRUPTED, Event::PromptStart), RUNNING);
    }

    #[test]
    fn permission_flow() {
        assert_eq!(transition(RUNNING, Event::PermissionRequest), WAITING_PERMISSION);
        assert_eq!(transition(WAITING_PERMISSION, Event::PermissionAllow), RUNNING);
        assert_eq!(transition(WAITING_PERMISSION, Event::PermissionDeny), IDLE);
        // 等待授权中 RPC 结束（拒绝导致轮次收尾）也能落回
        assert_eq!(transition(WAITING_PERMISSION, Event::PromptOk), IDLE);
    }

    #[test]
    fn invalid_transitions_keep_state() {
        // 空闲会话不会因为迟到的权限应答而变 running
        assert_eq!(transition(IDLE, Event::PermissionAllow), IDLE);
        // 已休眠的会话不受 heal 影响
        assert_eq!(transition(HIBERNATED, Event::StallHeal), HIBERNATED);
        // running 中重复 PromptStart 不变
        assert_eq!(transition(RUNNING, Event::PromptStart), RUNNING);
    }

    #[test]
    fn exit_and_hibernate_from_anywhere() {
        for s in [IDLE, RUNNING, WAITING_PERMISSION, ERROR] {
            assert_eq!(transition(s, Event::AgentExited), ERROR);
            assert_eq!(transition(s, Event::Hibernate), HIBERNATED);
        }
    }

    #[test]
    fn heal_only_fixes_running() {
        assert_eq!(transition(RUNNING, Event::StallHeal), IDLE);
        assert_eq!(transition(IDLE, Event::StallHeal), IDLE);
        assert_eq!(transition(WAITING_PERMISSION, Event::StallHeal), WAITING_PERMISSION);
    }

    #[test]
    fn predicates() {
        assert!(is_reclaimable(IDLE) && is_reclaimable(ERROR));
        assert!(!is_reclaimable(RUNNING) && !is_reclaimable(WAITING_PERMISSION));
        assert!(is_stale_live_status(RUNNING) && is_stale_live_status("starting"));
        assert!(!is_stale_live_status(HIBERNATED) && !is_stale_live_status(IDLE));
    }
}
