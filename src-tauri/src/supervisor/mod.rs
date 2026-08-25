//! Agent Supervisor: one `grok agent stdio` process per active session (design D3)

use crate::acp::{AcpClient, AcpEvent};
use crate::config::{DesktopPrefs, DesktopState, SessionMeta};
use crate::paths;
use crate::session_fsm::{self as fsm, Event as FsmEvent};
use crate::terminal::TerminalHost;
use crate::turn_lease;
use anyhow::{anyhow, Result};
use chrono::Utc;
use parking_lot::Mutex as StdMutex;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSession {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub cwd: String,
    pub grok_session_id: Option<String>,
    /// 该会话使用的小精灵档案 id（agents.json）
    pub agent_id: String,
    pub status: String, // idle | running | waiting_permission | error | hibernated | starting
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegated_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

struct SessionRuntime {
    meta: LiveSession,
    client: Option<Arc<AcpClient>>,
    /// Instance id of `client`; guards against stale `Exited` events from a
    /// replaced client overwriting the state of the new one.
    client_gen: u64,
    /// 最近一次生命周期活动（发送/应答/流式输出）。闲置回收依据。
    last_activity: std::time::Instant,
    /// 最近一次流式进展（token / 工具事件）。静默看门狗依据。
    last_stream_at: std::time::Instant,
    /// `session/prompt` 是否在途。RPC 已结束但状态仍 running → 无声自愈。
    prompt_inflight: bool,
    /// 本轮是否产生过任何 token / 工具事件（无输出轮次用更短静默阈值）。
    turn_had_activity: bool,
    /// 本轮已弹过「继续等待 / 结束本轮」，避免重复提示（继续等待后重置）。
    stall_notified: bool,
    /// 「本会话内允许」的权限范围缓存（scope key，见 permission_scope_key）。
    allow_scopes: std::collections::HashSet<String>,
}

impl SessionRuntime {
    fn new(meta: LiveSession, client: Arc<AcpClient>) -> Self {
        let now = std::time::Instant::now();
        let client_gen = client.instance_id();
        Self {
            meta,
            client: Some(client),
            client_gen,
            last_activity: now,
            last_stream_at: now,
            prompt_inflight: false,
            turn_had_activity: false,
            stall_notified: false,
            allow_scopes: std::collections::HashSet::new(),
        }
    }
}

/// 借鉴 grok-app 的进程治理默认值（process_limits）：
/// 活跃 agent 进程上限；超限时先回收最旧的空闲会话，全忙则拒绝新建。
const MAX_CONCURRENT_AGENTS: usize = 8;
/// 空闲（idle/error）超过该时长自动休眠（meta 保留，一键恢复）。
const IDLE_HIBERNATE_SECS: u64 = 30 * 60;
/// 流静默看门狗：本轮有过输出后允许的纯静默时长（工具常安静数分钟）。
const STALL_SILENCE_SECS: u64 = 300;
/// 本轮从未产生 token/工具事件时的静默阈值（挂死的 prompt 应更早提示）。
const STALL_SILENCE_NO_OUTPUT_SECS: u64 = 120;

pub struct Supervisor {
    sessions: Mutex<HashMap<String, SessionRuntime>>,
    state: Arc<StdMutex<DesktopState>>,
    /// ACP client-side terminal host (required when clientCapabilities.terminal=true)
    terminals: TerminalHost,
    /// Serializes the probe→spawn→initialize→load pipeline. Concurrent
    /// create/resume clicks used to each spawn their own `grok agent` +
    /// `grok --version` — the "many grok processes" storm.
    spawn_gate: Mutex<()>,
    /// In-flight start guards: `proj:<id>` for create, `sess:<id>` for resume.
    /// Rejects duplicate starts (double-click / impatient re-click) instead of
    /// stacking processes.
    starting: StdMutex<std::collections::HashSet<String>>,
    /// Spawn 已成功但 initialize/session-load 还在途的 client（按桌面会话 id）。
    /// 「取消恢复」由此杀掉在途进程；成功/失败后由 RAII 守卫移除。
    pending_spawns: Arc<StdMutex<HashMap<String, Arc<AcpClient>>>>,
}

/// RAII: removes the start-guard key when the create/resume attempt ends.
struct StartGuard<'a> {
    set: &'a StdMutex<std::collections::HashSet<String>>,
    key: String,
}

impl Drop for StartGuard<'_> {
    fn drop(&mut self) {
        self.set.lock().remove(&self.key);
    }
}

/// RAII: exposes an in-flight spawn's client for「取消恢复」, removed on drop.
struct PendingSpawnGuard {
    map: Arc<StdMutex<HashMap<String, Arc<AcpClient>>>>,
    key: String,
}

impl PendingSpawnGuard {
    fn register(
        map: Arc<StdMutex<HashMap<String, Arc<AcpClient>>>>,
        key: String,
        client: Arc<AcpClient>,
    ) -> Self {
        map.lock().insert(key.clone(), client);
        Self { map, key }
    }
}

impl Drop for PendingSpawnGuard {
    fn drop(&mut self) {
        self.map.lock().remove(&self.key);
    }
}

impl Supervisor {
    pub fn new(state: Arc<StdMutex<DesktopState>>) -> Self {
        let terminals = TerminalHost::new();
        // Honor Desktop prefs so ACP shell matches agent expectation (PowerShell).
        {
            let shell = state.lock().prefs.default_shell.clone();
            terminals.set_default_shell(&shell);
        }
        Self {
            sessions: Mutex::new(HashMap::new()),
            state,
            terminals,
            spawn_gate: Mutex::new(()),
            starting: StdMutex::new(std::collections::HashSet::new()),
            pending_spawns: Arc::new(StdMutex::new(HashMap::new())),
        }
    }

    /// Claim an exclusive start slot for `key`; `None` when already starting.
    fn begin_start(&self, key: String) -> Option<StartGuard<'_>> {
        let mut set = self.starting.lock();
        if !set.insert(key.clone()) {
            return None;
        }
        Some(StartGuard {
            set: &self.starting,
            key,
        })
    }

    pub fn terminals(&self) -> &TerminalHost {
        &self.terminals
    }

    /// Keep terminal host shell in sync after prefs save.
    pub fn sync_shell_from_prefs(&self) {
        let shell = self.state.lock().prefs.default_shell.clone();
        self.terminals.set_default_shell(&shell);
    }

    pub async fn list_live(&self) -> Vec<LiveSession> {
        let map = self.sessions.lock().await;
        map.values().map(|s| s.meta.clone()).collect()
    }

    pub async fn create_session(
        &self,
        app: AppHandle,
        project_id: String,
        cwd: String,
        title: Option<String>,
        agent_id: Option<String>,
        delegated_by: Option<String>,
        job_id: Option<String>,
    ) -> Result<LiveSession> {
        // 同项目并发新建守卫：双击/连点曾各自 spawn 一个 grok agent（进程风暴）
        let _start = self
            .begin_start(format!("proj:{project_id}"))
            .ok_or_else(|| anyhow!("该项目正在启动会话，请稍候…"))?;

        let prefs = self.state.lock().prefs.clone();
        let id = Uuid::new_v4().to_string();
        let title = title.unwrap_or_else(|| {
            PathBuf::from(&cwd)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "session".into())
        });
        let agent_id = agent_id.unwrap_or_else(|| crate::agents::DEFAULT_AGENT_ID.into());

        // cwd 先于一切检查：目录不存在时 spawn 只会报「无法启动 exe」，
        // 把真实原因（项目目录失效）埋掉。双击项目新建会话踩的就是这个。
        // spawn_blocking：失效网络盘上的 is_dir 可能阻塞几十秒，别占 worker。
        if !dir_exists(&cwd).await {
            return Err(anyhow!(
                "项目工作目录不存在或不可访问：{cwd}。项目可能被移动/重命名/删除。请在左侧项目菜单中移除后重新添加。"
            ));
        }

        self.ensure_capacity(&app).await?;

        let (client, grok_sid) = self
            .spawn_agent(&app, &id, &cwd, &prefs, None, &agent_id)
            .await?;

        let live = LiveSession {
            id: id.clone(),
            project_id: project_id.clone(),
            title: title.clone(),
            cwd: cwd.clone(),
            grok_session_id: Some(grok_sid.clone()),
            agent_id: agent_id.clone(),
            status: "idle".into(),
            error: None,
            delegated_by: delegated_by.clone(),
            job_id: job_id.clone(),
        };

        {
            let mut map = self.sessions.lock().await;
            map.insert(id.clone(), SessionRuntime::new(live.clone(), client.clone()));
        }

        {
            let mut st = self.state.lock();
            st.sessions.insert(
                0,
                SessionMeta {
                    id: id.clone(),
                    project_id,
                    title,
                    cwd,
                    grok_session_id: Some(grok_sid),
                    agent_id,
                    status: "idle".into(),
                    created_at: Utc::now().to_rfc3339(),
                    last_active_at: Utc::now().to_rfc3339(),
                    delegated_by,
                    job_id,
                },
            );
            if st.sessions.len() > 100 {
                st.sessions.truncate(100);
            }
            let _ = st.save();
        }

        let _ = app.emit("agent://state", &live);
        Ok(live)
    }

    /// Resume: spawn agent and `session/load` for an existing grok session id.
    /// Does **not** fall back to `session/new` on load failure (that would orphan
    /// the real history under a new empty session id).
    pub async fn resume_session(
        &self,
        app: AppHandle,
        desktop_session_id: String,
        grok_session_id: String,
        project_id: String,
        cwd: String,
        title: String,
        agent_id: Option<String>,
    ) -> Result<LiveSession> {
        // 并发恢复守卫：同一会话的重复点击不再各起一个进程
        let _start = self
            .begin_start(format!("sess:{desktop_session_id}"))
            .ok_or_else(|| anyhow!("该会话正在恢复中，请稍候…"))?;

        // Kill existing live process for this id if any
        self.hibernate(app.clone(), &desktop_session_id).await.ok();

        // 同一 grok 会话可能已被别的桌面会话占着（磁盘历史每次恢复都发新
        // 桌面 id）：先停掉旧进程，否则每次恢复都多留一个 grok agent。
        let dup_ids: Vec<String> = {
            let map = self.sessions.lock().await;
            map.values()
                .filter(|rt| {
                    rt.meta.id != desktop_session_id
                        && rt.meta.grok_session_id.as_deref() == Some(grok_session_id.as_str())
                })
                .map(|rt| rt.meta.id.clone())
                .collect()
        };
        for dup in dup_ids {
            tracing::info!("恢复去重：休眠占用同一 grok 会话的旧桌面会话 {dup}");
            let _ = self.hibernate(app.clone(), &dup).await;
        }

        // 同 create_session：cwd 失效时先给出真实原因，而不是误导性的「无法启动 exe」
        if !dir_exists(&cwd).await {
            return Err(anyhow!(
                "项目工作目录不存在或不可访问：{cwd}。项目可能被移动/重命名/删除。请在左侧项目菜单中移除后重新添加。"
            ));
        }

        self.ensure_capacity(&app).await?;

        let prefs = self.state.lock().prefs.clone();
        let agent_id = agent_id.unwrap_or_else(|| crate::agents::DEFAULT_AGENT_ID.into());
        let (kept_delegated, kept_job) = {
            let st = self.state.lock();
            st.sessions
                .iter()
                .find(|s| s.id == desktop_session_id)
                .map(|s| (s.delegated_by.clone(), s.job_id.clone()))
                .unwrap_or((None, None))
        };
        let (client, sid) = self
            .spawn_agent(
                &app,
                &desktop_session_id,
                &cwd,
                &prefs,
                Some(grok_session_id.as_str()),
                &agent_id,
            )
            .await?;

        let live = LiveSession {
            id: desktop_session_id.clone(),
            project_id,
            title,
            cwd,
            grok_session_id: Some(sid.clone()),
            agent_id: agent_id.clone(),
            status: "idle".into(),
            error: None,
            delegated_by: kept_delegated.clone(),
            job_id: kept_job.clone(),
        };

        {
            let mut map = self.sessions.lock().await;
            map.insert(
                desktop_session_id.clone(),
                SessionRuntime::new(live.clone(), client.clone()),
            );
        }

        {
            let mut st = self.state.lock();
            if let Some(s) = st
                .sessions
                .iter_mut()
                .find(|s| s.id == desktop_session_id)
            {
                s.grok_session_id = Some(sid.clone());
                s.agent_id = agent_id;
                s.status = "idle".into();
                s.title = live.title.clone();
                s.cwd = live.cwd.clone();
                s.project_id = live.project_id.clone();
                s.last_active_at = Utc::now().to_rfc3339();
            } else {
                st.sessions.insert(
                    0,
                    SessionMeta {
                        id: desktop_session_id.clone(),
                        project_id: live.project_id.clone(),
                        title: live.title.clone(),
                        cwd: live.cwd.clone(),
                        grok_session_id: Some(sid),
                        agent_id: live.agent_id.clone(),
                        status: "idle".into(),
                        created_at: Utc::now().to_rfc3339(),
                        last_active_at: Utc::now().to_rfc3339(),
                        delegated_by: kept_delegated,
                        job_id: kept_job,
                    },
                );
                if st.sessions.len() > 100 {
                    st.sessions.truncate(100);
                }
            }
            let _ = st.save();
        }

        let _ = app.emit("agent://state", &live);
        Ok(live)
    }

    async fn spawn_agent(
        &self,
        app: &AppHandle,
        desktop_session_id: &str,
        cwd: &str,
        prefs: &DesktopPrefs,
        load_session_id: Option<&str>,
        agent_id: &str,
    ) -> Result<(Arc<AcpClient>, String)> {
        // 串行化整条 spawn 流水线（探测→spawn→initialize→load）。
        // 并发 spawn 是进程风暴的放大器；启动本身只有几秒，排队可接受。
        let _spawn_permit = self.spawn_gate.lock().await;

        // 按档案解析启动形态（agents.json；grok 之外纯配置驱动）
        let profiles = crate::agents::load();
        let profile = crate::agents::find(&profiles, agent_id).ok_or_else(|| {
            anyhow!("没有可用的小精灵档案（agents.json 为空），请在「设置 → 小精灵」配置")
        })?;
        if !profile.enabled {
            return Err(anyhow!(
                "小精灵「{}」未启用。请在「设置 → 小精灵」中启用并填好密钥。",
                profile.name
            ));
        }
        if load_session_id.is_some() && !profile.supports_resume {
            return Err(anyhow!(
                "小精灵「{}」暂不支持会话恢复（仅实时会话）。",
                profile.name
            ));
        }

        // grok 原生集成：路径解析 + 存在性检查 + 版本门禁。
        // 非 grok 档案（npx 适配器等）没有统一版本号可查，启动失败即反馈。
        let grok_exe = if profile.is_grok {
            let grok_path = paths::resolve_grok_executable(if prefs.grok_path.is_empty() {
                None
            } else {
                Some(prefs.grok_path.as_str())
            });
            if grok_path.is_absolute() && !grok_path.exists() {
                return Err(anyhow!(
                    "未找到 Grok CLI：{}。请先安装 CLI，或在设置中指定路径。",
                    grok_path.display()
                ));
            }
            let min = if prefs.min_cli_version.is_empty() {
                "0.2.0".into()
            } else {
                prefs.min_cli_version.clone()
            };
            // Cached + hard-deadline version probe (5s timeout, 60s TTL):
            // per-spawn `grok --version` with no deadline both multiplied
            // processes and hung the spawn path when the CLI misbehaved.
            let installed = {
                let exe = grok_path.clone();
                tokio::task::spawn_blocking(move || {
                    crate::config::probe_grok_version_cached(&exe)
                })
                .await
                .map_err(|e| anyhow!("CLI 探测失败：{e}"))?
            };
            if !crate::cli_caps::version_meets_min(installed.as_deref(), &min) {
                return Err(anyhow!(
                    "Grok CLI 版本过旧（当前：{}，需要 ≥ {}）。请运行 `irm https://x.ai/cli/install.ps1 | iex` 升级后再试。",
                    installed.as_deref().unwrap_or("未知"),
                    min
                ));
            }
            grok_path.to_string_lossy().to_string()
        } else {
            String::new()
        };

        let (event_tx, event_rx) = mpsc::unbounded_channel::<AcpEvent>();
        // --always-approve 是 grok CLI 专属标志；其他 agent 走 ACP 标准权限流。
        let always = profile.is_grok && prefs.permission_mode == "always-approve";
        let model = crate::agents::resolve_model(profile);
        // prefs.sandbox_mode remains a UI note; do not forward as a CLI flag.
        let _sandbox_pref = prefs.sandbox_mode.as_str();

        let spec = crate::agents::build_spawn(profile, &model, always, &grok_exe);
        let client = AcpClient::spawn(&spec, PathBuf::from(cwd).as_path(), event_tx).await?;

        // 注册在途 spawn：「取消恢复」可据此杀进程；RAII 守卫返回时移除
        let _pending = PendingSpawnGuard::register(
            self.pending_spawns.clone(),
            desktop_session_id.to_string(),
            client.clone(),
        );

        // 每个失败路径都显式 kill：kill_on_drop / Job Object 是兜底，
        // 显式杀死让「启动失败」立即回收进程树，而不是等 Arc 引用归零。
        if let Err(e) = client.initialize().await {
            let _ = client.kill().await;
            return Err(anyhow!("Agent initialize 失败：{e}"));
        }

        let result = if let Some(sid) = load_session_id {
            // Never fall back to session/new here: a silent new id leaves meta pointing
            // at an empty chat_history while the real transcript stays under the old id.
            match client
                .session_load(sid, PathBuf::from(cwd).as_path())
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("session/load failed for {sid}: {e}");
                    let _ = client.kill().await;
                    return Err(anyhow!(
                        "session/load 失败（会话可能已失效、cwd 不匹配或 CLI 不支持恢复）：{e}。请从「磁盘会话历史」重新选择该会话。"
                    ));
                }
            }
        } else {
            match client
                .session_new(PathBuf::from(cwd).as_path(), session_meta_json(prefs))
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    let _ = client.kill().await;
                    return Err(anyhow!("session/new 失败：{e}"));
                }
            }
        };

        // session/new returns top-level sessionId; session/load often only puts it in
        // result._meta.sessionId (or omits it entirely). Fall back to the id we asked to load.
        let grok_sid = match extract_session_id(&result)
            .or_else(|| load_session_id.map(|s| s.to_string()))
        {
            Some(sid) => sid,
            None => {
                let _ = client.kill().await;
                return Err(if load_session_id.is_some() {
                    anyhow!("session/load 未返回 sessionId：{result}")
                } else {
                    anyhow!("session/new 未返回 sessionId：{result}")
                });
            }
        };

        spawn_event_forwarder(
            app.clone(),
            desktop_session_id.to_string(),
            client.instance_id(),
            event_rx,
        );

        Ok((client, grok_sid))
    }

    pub async fn send_prompt(&self, app: AppHandle, session_id: &str, text: &str) -> Result<()> {
        // IMPORTANT: never hold the sessions mutex across ACP awaits.
        // Holding it during prompt() deadlocks permission / fs / cancel handlers
        // (they also need the mutex to respond), which freezes the whole turn.
        let (grok_sid, client) = {
            let mut map = self.sessions.lock().await;
            let rt = map
                .get_mut(session_id)
                .ok_or_else(|| anyhow!("会话不存在或已休眠：{session_id}"))?;
            // 后端真值门禁：全局 busy 取消后由这里防并发 prompt
            if rt.meta.status == fsm::status::RUNNING {
                return Err(anyhow!("会话正在执行中，请等待本轮完成或先「停止」"));
            }
            if rt.meta.status == fsm::status::WAITING_PERMISSION {
                return Err(anyhow!("会话正在等待授权，请先处理授权请求"));
            }
            rt.meta.status = fsm::transition(&rt.meta.status, FsmEvent::PromptStart).into();
            rt.meta.error = None;
            // 静默看门狗按轮武装
            let now = std::time::Instant::now();
            rt.prompt_inflight = true;
            rt.turn_had_activity = false;
            rt.stall_notified = false;
            rt.last_stream_at = now;
            rt.last_activity = now;
            let _ = app.emit("agent://state", &rt.meta);
            let sid = rt
                .meta
                .grok_session_id
                .clone()
                .ok_or_else(|| anyhow!("缺少 Grok 会话 ID"))?;
            let client = rt
                .client
                .as_ref()
                .ok_or_else(|| anyhow!("会话没有运行中的 Agent（可能已休眠）"))?
                .clone();
            (sid, client)
        };

        // 轮次磁盘租约：宿主中途死亡时，下次启动能把该会话标 interrupted
        // 而不是看起来"干净结束"。文件极小，仍走 spawn_blocking 不占 worker。
        {
            let (sid, gsid, title, cmd) = (
                session_id.to_string(),
                grok_sid.clone(),
                {
                    let map = self.sessions.lock().await;
                    map.get(session_id)
                        .map(|rt| rt.meta.title.clone())
                        .unwrap_or_default()
                },
                text.to_string(),
            );
            let _ = tokio::task::spawn_blocking(move || {
                turn_lease::write_lease(&sid, Some(&gsid), &title, &cmd);
            })
            .await;
        }

        let result = client.prompt(&grok_sid, text).await;

        // 轮次收尾（成败皆然）：清租约
        {
            let sid = session_id.to_string();
            let _ = tokio::task::spawn_blocking(move || turn_lease::clear_lease(&sid)).await;
        }

        let mut map = self.sessions.lock().await;
        if let Some(rt) = map.get_mut(session_id) {
            rt.prompt_inflight = false;
            rt.last_activity = std::time::Instant::now();
            match &result {
                Ok(_) => {
                    rt.meta.status = fsm::transition(&rt.meta.status, FsmEvent::PromptOk).into();
                }
                Err(e) => {
                    rt.meta.status =
                        fsm::transition(&rt.meta.status, FsmEvent::PromptErr).into();
                    rt.meta.error = Some(e.to_string());
                }
            }
            let _ = app.emit("agent://state", &rt.meta);
        }
        result.map(|_| ())
    }

    pub async fn cancel(&self, session_id: &str) -> Result<()> {
        let (grok_sid, client) = {
            let map = self.sessions.lock().await;
            let rt = map
                .get(session_id)
                .ok_or_else(|| anyhow!("会话不存在"))?;
            let sid = rt
                .meta
                .grok_session_id
                .clone()
                .ok_or_else(|| anyhow!("缺少 Grok 会话 ID"))?;
            let client = rt
                .client
                .as_ref()
                .ok_or_else(|| anyhow!("没有运行中的 Agent"))?
                .clone();
            (sid, client)
        };
        client.cancel(&grok_sid).await?;
        Ok(())
    }

    pub async fn respond_permission(
        &self,
        app: AppHandle,
        session_id: &str,
        request_id: Value,
        allow: bool,
        option_id: Option<String>,
        remember_scope: Option<String>,
    ) -> Result<()> {
        let client = {
            let map = self.sessions.lock().await;
            let rt = map
                .get(session_id)
                .ok_or_else(|| anyhow!("会话不存在"))?;
            rt.client
                .as_ref()
                .ok_or_else(|| anyhow!("没有运行中的 Agent"))?
                .clone()
        };
        let result = if allow {
            json!({
                "outcome": {
                    "outcome": "selected",
                    "optionId": option_id.unwrap_or_else(|| "allow-once".into())
                }
            })
        } else {
            json!({ "outcome": { "outcome": "cancelled" } })
        };
        client.respond(request_id, result).await?;

        let mut map = self.sessions.lock().await;
        if let Some(rt) = map.get_mut(session_id) {
            rt.meta.status = fsm::transition(
                &rt.meta.status,
                if allow {
                    FsmEvent::PermissionAllow
                } else {
                    FsmEvent::PermissionDeny
                },
            )
            .into();
            // 授权应答 = 轮次继续推进：重置静默计时
            let now = std::time::Instant::now();
            rt.last_stream_at = now;
            rt.last_activity = now;
            rt.stall_notified = false;
            // 「本会话内允许」：记住范围，后续同类请求自动批准
            if allow {
                if let Some(scope) = remember_scope.filter(|s| !s.trim().is_empty()) {
                    tracing::info!("会话 {session_id} 记住权限范围：{scope}");
                    rt.allow_scopes.insert(scope);
                }
            }
            let _ = app.emit("agent://state", &rt.meta);
        }
        Ok(())
    }

    pub async fn respond_server_request(
        &self,
        session_id: &str,
        request_id: Value,
        result: Option<Value>,
        error: Option<String>,
    ) -> Result<()> {
        let client = {
            let map = self.sessions.lock().await;
            let rt = map
                .get(session_id)
                .ok_or_else(|| anyhow!("会话不存在"))?;
            rt.client
                .as_ref()
                .ok_or_else(|| anyhow!("没有运行中的 Agent"))?
                .clone()
        };
        if let Some(err) = error {
            client.respond_error(request_id, -32000, &err).await?;
        } else {
            client
                .respond(request_id, result.unwrap_or(Value::Null))
                .await?;
        }
        Ok(())
    }

    /// Handle ACP client methods the Desktop is responsible for:
    /// - `fs/read_text_file` / `fs/write_text_file`
    /// - `terminal/*` (create, output, wait_for_exit, kill, release)
    ///
    /// Returns `true` if the method was handled (success or error response already sent).
    pub async fn handle_client_request(
        &self,
        session_id: &str,
        request_id: Value,
        method: &str,
        params: &Value,
    ) -> Result<bool> {
        // Prefer exact ACP method names; keep loose match for older agents.
        let m = method.trim();

        // --- Terminal (critical for shell tools) ---
        // Stamp desktop session id so hibernate can release the right terminals.
        // (ACP params.sessionId is the *grok* session id, not our desktop id.)
        let term_params = {
            let mut p = params.clone();
            if let Some(obj) = p.as_object_mut() {
                obj.insert(
                    "_desktopSessionId".into(),
                    Value::String(session_id.to_string()),
                );
            }
            p
        };
        match self.terminals.handle(m, &term_params).await {
            Ok(Some(result)) => {
                self.respond_server_request(session_id, request_id, Some(result), None)
                    .await?;
                return Ok(true);
            }
            Ok(None) => {}
            Err(e) => {
                // Method looked like terminal/* but failed
                if m.starts_with("terminal/") {
                    self.respond_server_request(
                        session_id,
                        request_id,
                        None,
                        Some(e.to_string()),
                    )
                    .await?;
                    return Ok(true);
                }
                return Err(e);
            }
        }

        // --- Filesystem ---
        let is_read = m == "fs/read_text_file"
            || m == "fs/readTextFile"
            || m.eq_ignore_ascii_case("fs/read_text_file")
            || (m.contains("fs/") && m.to_ascii_lowercase().contains("read"));
        let is_write = m == "fs/write_text_file"
            || m == "fs/writeTextFile"
            || m.eq_ignore_ascii_case("fs/write_text_file")
            || (m.contains("fs/") && m.to_ascii_lowercase().contains("write"));

        if !is_read && !is_write {
            return Ok(false);
        }

        let path = params
            .get("path")
            .or_else(|| params.get("uri"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("缺少文件路径"))?;

        let (cwd, fs_scope) = {
            let map = self.sessions.lock().await;
            let rt = map
                .get(session_id)
                .ok_or_else(|| anyhow!("会话不存在"))?;
            let cwd = rt.meta.cwd.clone();
            let scope = self.state.lock().prefs.fs_scope.clone();
            (cwd, scope)
        };
        let path = if fs_scope.trim().eq_ignore_ascii_case("unrestricted") {
            std::path::PathBuf::from(&path)
        } else {
            match crate::workspace::resolve_inside(std::path::Path::new(&cwd), &path) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("ACP fs 拒绝路径：{e}");
                    self.respond_server_request(session_id, request_id, None, Some(e))
                        .await?;
                    return Ok(true);
                }
            }
        };

        if is_read {
            // Optional line/limit (1-based line)
            let line = params.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
            let limit = params.get("limit").and_then(|v| v.as_u64());
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    let content = if line > 1 || limit.is_some() {
                        let lines: Vec<&str> = content.lines().collect();
                        let start = if line > 0 {
                            (line as usize).saturating_sub(1)
                        } else {
                            0
                        };
                        let end = match limit {
                            Some(n) => (start + n as usize).min(lines.len()),
                            None => lines.len(),
                        };
                        if start >= lines.len() {
                            String::new()
                        } else {
                            lines[start..end].join("\n")
                        }
                    } else {
                        content
                    };
                    self.respond_server_request(
                        session_id,
                        request_id,
                        Some(json!({ "content": content })),
                        None,
                    )
                    .await?;
                }
                Err(e) => {
                    self.respond_server_request(session_id, request_id, None, Some(e.to_string()))
                        .await?;
                }
            }
            return Ok(true);
        }

        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&path, content) {
            Ok(()) => {
                self.respond_server_request(session_id, request_id, Some(json!({})), None)
                    .await?;
            }
            Err(e) => {
                self.respond_server_request(session_id, request_id, None, Some(e.to_string()))
                    .await?;
            }
        }
        Ok(true)
    }

    /// Backward-compatible alias.
    pub async fn handle_fs_server_request(
        &self,
        session_id: &str,
        request_id: Value,
        method: &str,
        params: &Value,
    ) -> Result<bool> {
        self.handle_client_request(session_id, request_id, method, params)
            .await
    }

    pub async fn hibernate(&self, app: AppHandle, session_id: &str) -> Result<()> {
        self.terminals.release_session(session_id).await;
        // Collect under the lock; kill + persist outside it so concurrent
        // send_prompt / permission handlers never stall behind process death
        // and a full state-file write.
        let removed = {
            let mut map = self.sessions.lock().await;
            map.remove(session_id)
        };
        if let Some(mut rt) = removed {
            if let Some(client) = rt.client.take() {
                let _ = client.kill().await;
            }
            rt.meta.status = fsm::transition(&rt.meta.status, FsmEvent::Hibernate).into();
            let _ = app.emit("agent://state", &rt.meta);
            let mut st = self.state.lock();
            if let Some(s) = st.sessions.iter_mut().find(|s| s.id == session_id) {
                s.status = fsm::status::HIBERNATED.into();
                s.last_active_at = Utc::now().to_rfc3339();
            }
            let _ = st.save();
        }
        Ok(())
    }

    /// 进程上限（借鉴 grok-app I02）：满员时先回收最旧的空闲/出错会话；
    /// 全部在忙（running / waiting_permission）则拒绝——**绝不杀忙轮次**。
    async fn ensure_capacity(&self, app: &AppHandle) -> Result<()> {
        for _ in 0..MAX_CONCURRENT_AGENTS + 1 {
            let (count, oldest_idle) = {
                let map = self.sessions.lock().await;
                let count = map.len();
                let oldest = map
                    .values()
                    .filter(|rt| fsm::is_reclaimable(&rt.meta.status))
                    .min_by_key(|rt| rt.last_activity)
                    .map(|rt| rt.meta.id.clone());
                (count, oldest)
            };
            if count < MAX_CONCURRENT_AGENTS {
                return Ok(());
            }
            let Some(victim) = oldest_idle else {
                return Err(anyhow!(
                    "已达并发上限（{MAX_CONCURRENT_AGENTS} 个活跃小精灵，且全部在忙）。请等待任务完成或手动休眠。"
                ));
            };
            tracing::info!("进程上限：回收最旧空闲会话 {victim}");
            let _ = self.hibernate(app.clone(), &victim).await;
        }
        Ok(())
    }

    /// 事件转发器：该会话有流式进展（token / 工具事件）。
    /// 喂给闲置回收与静默看门狗；按合批粒度调用（≤25 次/秒/会话）。
    pub async fn note_stream_activity(&self, session_id: &str) {
        let mut map = self.sessions.lock().await;
        if let Some(rt) = map.get_mut(session_id) {
            let now = std::time::Instant::now();
            rt.last_stream_at = now;
            rt.last_activity = now;
            rt.turn_had_activity = true;
            rt.stall_notified = false;
        }
    }

    /// 静默看门狗（借鉴 grok-app stream_stall / tool_heartbeat）：
    /// 1. 无声自愈：状态 running 但 prompt RPC 已结束 → 拨回 idle；
    /// 2. 长工具心跳的等价物：会话还有存活的 ACP 终端 → 视为在干活，重新武装；
    /// 3. 纯静默超阈值 → 弹「继续等待 / 结束本轮」（agent://stall）。
    ///    **绝不自动取消**用户发起的轮次，只提示。
    pub async fn stall_check(&self, app: &AppHandle) {
        // 收集待办再逐个处理，锁内不做 IO / emit
        let mut heal: Vec<LiveSession> = Vec::new();
        let mut maybe_stalled: Vec<(String, String, u64)> = Vec::new();
        {
            let mut map = self.sessions.lock().await;
            for rt in map.values_mut() {
                if rt.meta.status != fsm::status::RUNNING {
                    continue;
                }
                if !rt.prompt_inflight {
                    rt.meta.status =
                        fsm::transition(&rt.meta.status, FsmEvent::StallHeal).into();
                    heal.push(rt.meta.clone());
                    continue;
                }
                if rt.stall_notified {
                    continue;
                }
                let threshold = if rt.turn_had_activity {
                    STALL_SILENCE_SECS
                } else {
                    STALL_SILENCE_NO_OUTPUT_SECS
                };
                let silent = rt.last_stream_at.elapsed().as_secs();
                if silent >= threshold {
                    maybe_stalled.push((rt.meta.id.clone(), rt.meta.title.clone(), silent));
                }
            }
        }
        for meta in heal {
            tracing::info!("无声自愈：会话 {} RPC 已结束但状态残留 running", meta.id);
            let _ = app.emit("agent://state", &meta);
        }
        for (id, title, silent) in maybe_stalled {
            // 有存活终端 = 长工具还在跑：重新武装，不算失速
            if self.terminals.session_has_running(&id).await {
                let mut map = self.sessions.lock().await;
                if let Some(rt) = map.get_mut(&id) {
                    rt.last_stream_at = std::time::Instant::now();
                }
                continue;
            }
            {
                let mut map = self.sessions.lock().await;
                match map.get_mut(&id) {
                    Some(rt) if rt.meta.status == "running" && !rt.stall_notified => {
                        rt.stall_notified = true;
                    }
                    _ => continue,
                }
            }
            tracing::warn!("会话 {id} 静默 {silent}s，提示用户（不自动取消）");
            let _ = app.emit(
                "agent://stall",
                json!({ "sessionId": id, "title": title, "silentSecs": silent }),
            );
        }
    }

    /// 「继续等待」：重置静默计时，允许下一轮提示。
    pub async fn stall_keep_waiting(&self, session_id: &str) {
        let mut map = self.sessions.lock().await;
        if let Some(rt) = map.get_mut(session_id) {
            rt.last_stream_at = std::time::Instant::now();
            rt.stall_notified = false;
        }
    }

    /// 闲置回收（借鉴 grok-app I03）：空闲/出错超 30 分钟自动休眠。
    /// meta 保留，一键恢复；running / waiting_permission 永不回收。
    pub async fn idle_reaper(&self, app: &AppHandle) {
        let victims: Vec<String> = {
            let map = self.sessions.lock().await;
            map.values()
                .filter(|rt| {
                    fsm::is_reclaimable(&rt.meta.status)
                        && rt.last_activity.elapsed().as_secs() >= IDLE_HIBERNATE_SECS
                })
                .map(|rt| rt.meta.id.clone())
                .collect()
        };
        for id in victims {
            tracing::info!("闲置回收：休眠会话 {id}（>30 分钟无活动）");
            let _ = self.hibernate(app.clone(), &id).await;
        }
    }

    /// 「取消恢复」：杀掉 spawn 已成功但 initialize/load 还在途的进程。
    /// fail_all_pending 会立刻打断在途请求，恢复流程随即报错返回。
    pub async fn cancel_start(&self, session_id: &str) -> Result<()> {
        let client = self.pending_spawns.lock().get(session_id).cloned();
        match client {
            Some(c) => {
                tracing::info!("用户取消启动：杀掉会话 {session_id} 的在途 Agent");
                c.kill().await
            }
            None => Err(anyhow!("该会话没有可取消的启动流程")),
        }
    }

    /// 权限自动应答：命中「本会话内允许」缓存时直接批准，不再弹窗。
    /// 命中时返回 true（已应答并广播状态）。
    pub async fn try_auto_allow(
        &self,
        app: &AppHandle,
        session_id: &str,
        request_id: &Value,
        params: &Value,
    ) -> bool {
        let scope = permission_scope_key(params);
        let client = {
            let mut map = self.sessions.lock().await;
            let Some(rt) = map.get_mut(session_id) else {
                return false;
            };
            if !rt.allow_scopes.contains(&scope) {
                return false;
            }
            rt.meta.status =
                fsm::transition(&rt.meta.status, FsmEvent::PermissionAllow).into();
            rt.last_stream_at = std::time::Instant::now();
            rt.stall_notified = false;
            let _ = app.emit("agent://state", &rt.meta);
            match rt.client.as_ref() {
                Some(c) => c.clone(),
                None => return false,
            }
        };
        let option_id = pick_allow_option(params);
        let outcome = json!({
            "outcome": { "outcome": "selected", "optionId": option_id }
        });
        if let Err(e) = client.respond(request_id.clone(), outcome).await {
            tracing::warn!("自动批准应答失败（{scope}）：{e}");
            return false;
        }
        let _ = app.emit(
            "agent://permissionAuto",
            json!({ "sessionId": session_id, "scope": scope }),
        );
        true
    }

    /// 「本会话内允许」：记住该 scope，本会话后续同类请求自动批准。
    pub async fn remember_allow_scope(&self, session_id: &str, scope: String) {
        let mut map = self.sessions.lock().await;
        if let Some(rt) = map.get_mut(session_id) {
            tracing::info!("会话 {session_id} 记住权限范围：{scope}");
            rt.allow_scopes.insert(scope);
        }
    }

    /// 项目切换的进程回收：休眠**其他项目**里空闲/出错的活跃会话。
    /// 运行中 / 等待授权的会话不动（切换永不打断在忙的轮次）。
    /// 返回回收数量。会话 meta 保留，随时可一键恢复。
    pub async fn hibernate_other_projects_idle(
        &self,
        app: AppHandle,
        active_project_id: &str,
    ) -> usize {
        let victims: Vec<String> = {
            let map = self.sessions.lock().await;
            map.values()
                .filter(|rt| {
                    rt.meta.project_id != active_project_id
                        && fsm::is_reclaimable(&rt.meta.status)
                })
                .map(|rt| rt.meta.id.clone())
                .collect()
        };
        let n = victims.len();
        for id in victims {
            let _ = self.hibernate(app.clone(), &id).await;
        }
        if n > 0 {
            tracing::info!("项目切换：回收其他项目的 {n} 个空闲 Agent 进程");
        }
        n
    }

    pub async fn kill_all(&self) {
        self.terminals.release_all().await;
        let clients: Vec<Arc<AcpClient>> = {
            let mut map = self.sessions.lock().await;
            map.drain()
                .filter_map(|(_, mut rt)| rt.client.take())
                .collect()
        };
        for c in clients {
            let _ = c.kill().await;
        }
    }

    /// Unexpected agent exit: drop client, mark error. Ignores the event when
    /// the session is gone (hibernated/removed) or was taken over by a newer
    /// client — a stale exit must not clobber the replacement's state.
    pub async fn mark_exited_if_current(
        &self,
        app: &AppHandle,
        session_id: &str,
        code: Option<i32>,
        client_instance: u64,
    ) {
        let mut map = self.sessions.lock().await;
        if let Some(rt) = map.get_mut(session_id) {
            if rt.client_gen != client_instance {
                tracing::info!(
                    "忽略陈旧的 Agent 退出事件（会话 {session_id} 已由新 Agent 接管）"
                );
                return;
            }
            rt.client = None;
            rt.meta.status = fsm::transition(&rt.meta.status, FsmEvent::AgentExited).into();
            rt.meta.error = Some(format!("Agent 进程已退出：{code:?}"));
            rt.prompt_inflight = false;
            let _ = app.emit("agent://state", &rt.meta);
        }
    }

    pub async fn set_status(&self, app: &AppHandle, session_id: &str, status: &str) {
        let mut map = self.sessions.lock().await;
        if let Some(rt) = map.get_mut(session_id) {
            rt.meta.status = status.into();
            let _ = app.emit("agent://state", &rt.meta);
        }
    }

    /// 收到 `session/request_permission`：状态机进入 waiting_permission。
    pub async fn mark_waiting_permission(&self, app: &AppHandle, session_id: &str) {
        let mut map = self.sessions.lock().await;
        if let Some(rt) = map.get_mut(session_id) {
            rt.meta.status =
                fsm::transition(&rt.meta.status, FsmEvent::PermissionRequest).into();
            rt.last_activity = std::time::Instant::now();
            let _ = app.emit("agent://state", &rt.meta);
        }
    }
}

/// Emit throttle for streaming events.
///
/// Every `app.emit` is dispatched onto the **main thread** (WebView2 must be
/// driven from its creating thread). Per-token notifications × per-line stderr
/// × N agent processes used to flood the Win32 message pump — the window could
/// not even process move events (the "frozen, unmovable window" symptom).
/// Notifications and stderr are therefore coalesced and flushed on a short
/// interval; permission / server requests / exit stay immediate (flushing the
/// buffer first to preserve ordering).
const STREAM_FLUSH_MS: u64 = 40;
/// Flush early when a batch grows this large (bounds payload size).
const STREAM_BATCH_MAX: usize = 200;
/// Cap buffered stderr lines per flush window (a crash-looping child can
/// produce megabytes; the UI only needs the head).
const STDERR_BATCH_MAX: usize = 200;

fn spawn_event_forwarder(
    app: AppHandle,
    desktop_session_id: String,
    client_instance: u64,
    mut event_rx: mpsc::UnboundedReceiver<AcpEvent>,
) {
    tokio::spawn(async move {
        let mut stream_buf: Vec<Value> = Vec::new();
        let mut stderr_buf: Vec<String> = Vec::new();
        let mut stderr_dropped: usize = 0;
        let mut flush_tick =
            tokio::time::interval(std::time::Duration::from_millis(STREAM_FLUSH_MS));
        flush_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        macro_rules! flush {
            () => {
                if !stream_buf.is_empty() {
                    let events = std::mem::take(&mut stream_buf);
                    // 喂静默看门狗 / 闲置回收（按合批粒度，非逐 token）
                    if let Some(state) = app.try_state::<crate::commands::AppState>() {
                        state
                            .supervisor
                            .note_stream_activity(&desktop_session_id)
                            .await;
                    }
                    let _ = app.emit(
                        "agent://streamBatch",
                        json!({
                            "sessionId": desktop_session_id,
                            "events": events
                        }),
                    );
                }
                if !stderr_buf.is_empty() || stderr_dropped > 0 {
                    let mut text = std::mem::take(&mut stderr_buf).join("\n");
                    let dropped = std::mem::take(&mut stderr_dropped);
                    if dropped > 0 {
                        text.push_str(&format!("\n…（另有 {dropped} 行 stderr 被省略）"));
                    }
                    let _ = app.emit(
                        "agent://stderr",
                        json!({ "sessionId": desktop_session_id, "text": text }),
                    );
                }
            };
        }

        loop {
            tokio::select! {
                ev = event_rx.recv() => {
                    let Some(ev) = ev else {
                        flush!();
                        break;
                    };
                    match ev {
                        AcpEvent::Notification { method, params } => {
                            stream_buf.push(json!({ "method": method, "params": params }));
                            if stream_buf.len() >= STREAM_BATCH_MAX {
                                flush!();
                            }
                        }
                        AcpEvent::Stderr { text } => {
                            if stderr_buf.len() < STDERR_BATCH_MAX {
                                stderr_buf.push(text);
                            } else {
                                stderr_dropped += 1;
                            }
                        }
                        AcpEvent::ParseError { line, error } => {
                            tracing::warn!("ACP parse error: {error} line={line}");
                        }
                        // 请求/退出必须即时处理；先冲刷缓冲保证事件顺序
                        other => {
                            flush!();
                            handle_immediate_event(
                                &app,
                                &desktop_session_id,
                                client_instance,
                                other,
                            )
                            .await;
                        }
                    }
                }
                _ = flush_tick.tick() => {
                    flush!();
                }
            }
        }
    });
}

/// ServerRequest / Exited handling (unchanged semantics; extracted so the
/// batching loop stays readable).
async fn handle_immediate_event(
    app: &AppHandle,
    desktop_session_id: &str,
    client_instance: u64,
    ev: AcpEvent,
) {
    match ev {
        AcpEvent::ServerRequest { id, method, params } => {
            let method_l = method.to_ascii_lowercase();
            if method_l.contains("permission") {
                // 「本会话内允许」缓存命中 → 直接批准，不打断用户
                if let Some(state) = app.try_state::<crate::commands::AppState>() {
                    if state
                        .supervisor
                        .try_auto_allow(app, desktop_session_id, &id, &params)
                        .await
                    {
                        return;
                    }
                }
                let _ = app.emit(
                    "agent://permission",
                    json!({
                        "sessionId": desktop_session_id,
                        "id": id,
                        "method": method,
                        "params": params,
                        // 供前端「本会话内允许」回传（respond_permission.rememberScope）
                        "scopeKey": permission_scope_key(&params)
                    }),
                );
                // Rust 侧也进入 waiting_permission：状态机真值在宿主，
                // 前端只投影快照（此前仅发事件，后端 meta 一直停在 running）
                if let Some(state) = app.try_state::<crate::commands::AppState>() {
                    state
                        .supervisor
                        .mark_waiting_permission(app, desktop_session_id)
                        .await;
                }
                return;
            }

            // Handle terminal/* and fs/* in Rust immediately.
            // Shell tools break if terminal/create waits on the UI hop.
            if method_l.starts_with("terminal/")
                || method_l.starts_with("fs/")
                || method_l.contains("read_text_file")
                || method_l.contains("write_text_file")
            {
                if let Some(state) = app.try_state::<crate::commands::AppState>() {
                    let supervisor = state.supervisor.clone();
                    let sid = desktop_session_id.to_string();
                    let method_owned = method.clone();
                    let params_owned = params.clone();
                    let req_id = id.clone();
                    // Detach so we can still process concurrent ACP messages
                    // (e.g. terminal/output while wait_for_exit is pending).
                    tokio::spawn(async move {
                        match supervisor
                            .handle_client_request(
                                &sid,
                                req_id.clone(),
                                &method_owned,
                                &params_owned,
                            )
                            .await
                        {
                            Ok(true) => {}
                            Ok(false) => {
                                let _ = supervisor
                                    .respond_server_request(
                                        &sid,
                                        req_id,
                                        None,
                                        Some(format!(
                                            "未处理的客户端方法：{method_owned}"
                                        )),
                                    )
                                    .await;
                            }
                            Err(e) => {
                                tracing::error!(
                                    "client request {method_owned} failed: {e}"
                                );
                                let _ = supervisor
                                    .respond_server_request(
                                        &sid,
                                        req_id,
                                        None,
                                        Some(e.to_string()),
                                    )
                                    .await;
                            }
                        }
                    });
                    return;
                }
            }

            // Fallback: forward unknown server requests to UI
            let _ = app.emit(
                "agent://serverRequest",
                json!({
                    "sessionId": desktop_session_id,
                    "id": id,
                    "method": method,
                    "params": params
                }),
            );
        }
        AcpEvent::Exited { code } => {
            // Route through mark_exited_if_current: emitting directly
            // here would let a stale exit (replaced/hibernated client)
            // clobber the new session state in the UI.
            if let Some(state) = app.try_state::<crate::commands::AppState>() {
                state
                    .supervisor
                    .mark_exited_if_current(app, desktop_session_id, code, client_instance)
                    .await;
            }
        }
        // Notification / Stderr / ParseError 由批量循环处理，不会走到这里
        _ => {}
    }
}

/// `Path::is_dir` off the async workers: a dead network drive can block for
/// tens of seconds and must not stall the runtime.
async fn dir_exists(path: &str) -> bool {
    let p = PathBuf::from(path);
    tokio::task::spawn_blocking(move || p.is_dir())
        .await
        .unwrap_or(false)
}

/// 后台看门狗：15s 一拍做静默检查，每 4 拍（60s）做一次闲置回收。
/// 从 lib.rs setup 启动一次。
pub fn spawn_watchdog(app: AppHandle, supervisor: Arc<Supervisor>) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(15));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut n: u64 = 0;
        loop {
            tick.tick().await;
            supervisor.stall_check(&app).await;
            n += 1;
            if n % 4 == 0 {
                supervisor.idle_reaper(&app).await;
            }
        }
    });
}

/// 权限请求的范围键（「本会话内允许」按此聚类）。
/// 优先 tool kind（execute / edit / read …，粒度与 CLI 的权限档一致），
/// 退化到工具标题的第一个词，再退化到 unknown。
pub fn permission_scope_key(params: &Value) -> String {
    let tool = params
        .get("toolCall")
        .or_else(|| params.get("tool_call"))
        .or_else(|| params.get("tool"));
    let kind = tool
        .and_then(|t| t.get("kind").or_else(|| t.get("type")))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(k) = kind {
        return format!("kind:{}", k.to_ascii_lowercase());
    }
    let title = tool
        .and_then(|t| t.get("title").or_else(|| t.get("name")))
        .or_else(|| params.get("title"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(t) = title {
        let first = t.split_whitespace().next().unwrap_or(t);
        return format!("tool:{}", first.to_ascii_lowercase());
    }
    "unknown".into()
}

/// 自动批准时选用的 optionId：优先请求自带 options 里 kind/id 含
/// allow_once 的项，避免猜一个 CLI 不认识的 id；找不到则退回 "allow-once"。
fn pick_allow_option(params: &Value) -> String {
    if let Some(options) = params.get("options").and_then(|v| v.as_array()) {
        // 先找 allow_once 类
        for opt in options {
            let kind = opt.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let oid = opt
                .get("optionId")
                .or_else(|| opt.get("option_id"))
                .or_else(|| opt.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let k = kind.to_ascii_lowercase().replace('-', "_");
            if !oid.is_empty() && (k == "allow_once" || oid.eq_ignore_ascii_case("allow-once")) {
                return oid.to_string();
            }
        }
        // 任意 allow 类（避开 always，本会话缓存不该升级为永久允许）
        for opt in options {
            let oid = opt
                .get("optionId")
                .or_else(|| opt.get("option_id"))
                .or_else(|| opt.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let l = oid.to_ascii_lowercase();
            if !oid.is_empty() && l.contains("allow") && !l.contains("always") {
                return oid.to_string();
            }
        }
    }
    "allow-once".into()
}

fn session_meta_json(prefs: &DesktopPrefs) -> Value {
    let mut meta = json!({});
    if prefs.permission_mode == "always-approve" {
        meta["yoloMode"] = json!(true);
    }
    if prefs.permission_mode == "auto" {
        meta["autoMode"] = json!(true);
    }
    meta
}

fn extract_session_id(v: &Value) -> Option<String> {
    v.get("sessionId")
        .or_else(|| v.get("session_id"))
        .or_else(|| v.get("id"))
        // session/load (CLI 0.2+) often only echoes the id under _meta
        .or_else(|| v.pointer("/_meta/sessionId"))
        .or_else(|| v.pointer("/_meta/session_id"))
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_key_prefers_tool_kind() {
        let p = json!({
            "toolCall": { "kind": "Execute", "title": "Run tests" }
        });
        assert_eq!(permission_scope_key(&p), "kind:execute");
    }

    #[test]
    fn scope_key_falls_back_to_title_first_word() {
        let p = json!({ "toolCall": { "title": "Bash git status" } });
        assert_eq!(permission_scope_key(&p), "tool:bash");
        assert_eq!(permission_scope_key(&json!({})), "unknown");
    }

    #[test]
    fn pick_allow_prefers_allow_once_kind() {
        let p = json!({
            "options": [
                { "optionId": "reject", "kind": "reject_once" },
                { "optionId": "proceed_once", "kind": "allow_once" },
                { "optionId": "proceed_always", "kind": "allow_always" }
            ]
        });
        assert_eq!(pick_allow_option(&p), "proceed_once");
    }

    #[test]
    fn pick_allow_never_escalates_to_always() {
        let p = json!({
            "options": [
                { "optionId": "allow-always" },
                { "optionId": "allow-for-now" }
            ]
        });
        assert_eq!(pick_allow_option(&p), "allow-for-now");
        assert_eq!(pick_allow_option(&json!({})), "allow-once");
    }
}
