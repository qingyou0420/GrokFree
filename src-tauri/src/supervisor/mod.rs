//! Agent Supervisor: one `grok agent stdio` process per active session (design D3)

use crate::acp::{AcpClient, AcpEvent};
use crate::config::{DesktopPrefs, DesktopState, SessionMeta};
use crate::paths;
use crate::terminal::TerminalHost;
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
}

pub struct Supervisor {
    sessions: Mutex<HashMap<String, SessionRuntime>>,
    state: Arc<StdMutex<DesktopState>>,
    /// ACP client-side terminal host (required when clientCapabilities.terminal=true)
    terminals: TerminalHost,
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
        }
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
        if !PathBuf::from(&cwd).is_dir() {
            return Err(anyhow!(
                "项目工作目录不存在或不可访问：{cwd}。项目可能被移动/重命名/删除。请在左侧项目菜单中移除后重新添加。"
            ));
        }

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
            map.insert(
                id.clone(),
                SessionRuntime {
                    meta: live.clone(),
                    client: Some(client.clone()),
                    client_gen: client.instance_id(),
                },
            );
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
        // Kill existing live process for this id if any
        self.hibernate(app.clone(), &desktop_session_id).await.ok();

        // 同 create_session：cwd 失效时先给出真实原因，而不是误导性的「无法启动 exe」
        if !PathBuf::from(&cwd).is_dir() {
            return Err(anyhow!(
                "项目工作目录不存在或不可访问：{cwd}。项目可能被移动/重命名/删除。请在左侧项目菜单中移除后重新添加。"
            ));
        }

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
                SessionRuntime {
                    meta: live.clone(),
                    client: Some(client.clone()),
                    client_gen: client.instance_id(),
                },
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
            // probe_environment spawns `grok --version` etc. synchronously — keep
            // it off the async worker.
            let installed = {
                let p = prefs.clone();
                tokio::task::spawn_blocking(move || {
                    crate::config::probe_environment(&p).grok_version
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

        client.initialize().await?;

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
                    // kill_on_drop on AcpClient child cleans up the process
                    return Err(anyhow!(
                        "session/load 失败（会话可能已失效、cwd 不匹配或 CLI 不支持恢复）：{e}。请从「磁盘会话历史」重新选择该会话。"
                    ));
                }
            }
        } else {
            client
                .session_new(PathBuf::from(cwd).as_path(), session_meta_json(prefs))
                .await?
        };

        // session/new returns top-level sessionId; session/load often only puts it in
        // result._meta.sessionId (or omits it entirely). Fall back to the id we asked to load.
        let grok_sid = extract_session_id(&result)
            .or_else(|| load_session_id.map(|s| s.to_string()))
            .ok_or_else(|| {
                if load_session_id.is_some() {
                    anyhow!("session/load 未返回 sessionId：{result}")
                } else {
                    anyhow!("session/new 未返回 sessionId：{result}")
                }
            })?;

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
            rt.meta.status = "running".into();
            rt.meta.error = None;
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

        let result = client.prompt(&grok_sid, text).await;

        let mut map = self.sessions.lock().await;
        if let Some(rt) = map.get_mut(session_id) {
            match &result {
                Ok(_) => {
                    rt.meta.status = "idle".into();
                }
                Err(e) => {
                    rt.meta.status = "error".into();
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
            rt.meta.status = if allow {
                "running".into()
            } else {
                "idle".into()
            };
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
            rt.meta.status = "hibernated".into();
            let _ = app.emit("agent://state", &rt.meta);
            let mut st = self.state.lock();
            if let Some(s) = st.sessions.iter_mut().find(|s| s.id == session_id) {
                s.status = "hibernated".into();
                s.last_active_at = Utc::now().to_rfc3339();
            }
            let _ = st.save();
        }
        Ok(())
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
            rt.meta.status = "error".into();
            rt.meta.error = Some(format!("Agent 进程已退出：{code:?}"));
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
}

fn spawn_event_forwarder(
    app: AppHandle,
    desktop_session_id: String,
    client_instance: u64,
    mut event_rx: mpsc::UnboundedReceiver<AcpEvent>,
) {
    tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            match ev {
                AcpEvent::Notification { method, params } => {
                    let _ = app.emit(
                        "agent://stream",
                        json!({
                            "sessionId": desktop_session_id,
                            "method": method,
                            "params": params
                        }),
                    );
                }
                AcpEvent::ServerRequest { id, method, params } => {
                    let method_l = method.to_ascii_lowercase();
                    if method_l.contains("permission") {
                        let _ = app.emit(
                            "agent://permission",
                            json!({
                                "sessionId": desktop_session_id,
                                "id": id,
                                "method": method,
                                "params": params
                            }),
                        );
                        let _ = app.emit(
                            "agent://state",
                            json!({
                                "id": desktop_session_id,
                                "status": "waiting_permission"
                            }),
                        );
                        continue;
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
                            let sid = desktop_session_id.clone();
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
                            continue;
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
                AcpEvent::Stderr { text } => {
                    let _ = app.emit(
                        "agent://stderr",
                        json!({ "sessionId": desktop_session_id, "text": text }),
                    );
                }
                AcpEvent::Exited { code } => {
                    // Route through mark_exited_if_current: emitting directly
                    // here would let a stale exit (replaced/hibernated client)
                    // clobber the new session state in the UI.
                    if let Some(state) = app.try_state::<crate::commands::AppState>() {
                        state
                            .supervisor
                            .mark_exited_if_current(
                                &app,
                                &desktop_session_id,
                                code,
                                client_instance,
                            )
                            .await;
                    }
                }
                AcpEvent::ParseError { line, error } => {
                    tracing::warn!("ACP parse error: {error} line={line}");
                }
            }
        }
    });
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
