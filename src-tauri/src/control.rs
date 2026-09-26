//! 本地控制口：给外部程序（助手 / 脚本）一个最小 HTTP 接口，
//! 可以直接建会话、发 prompt、查状态、应答授权、取消，而不必去驱动 WebView。
//!
//! 设计约束：
//! - 只绑 127.0.0.1，绝不对外网暴露。
//! - 每个请求必须带 token（`x-grokfree-token` 头或 `Authorization: Bearer`）。
//!   端口与 token 写在 `<desktop_data_dir>/control.json`。本机任意进程读到
//!   这个文件就能驱动会改文件的 Agent，这是单人机器上可接受的边界。
//! - 不引入新的 crate：用 tokio + serde_json 手搓一个够用的 HTTP/1.1。
//!
//! 路由：
//!   GET  /health
//!   GET  /projects
//!   GET  /sessions
//!   GET  /sessions/:id
//!   POST /prompt     {"project"?, "sessionId"?, "text", "newSession"?}
//!   POST /cancel     {"sessionId"}
//!   POST /permission {"sessionId", "allow", "remember"?}

use crate::config::{DesktopState, Project};
use crate::paths;
use crate::supervisor::Supervisor;
use parking_lot::Mutex as StdMutex;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// 默认端口；可用环境变量 GROKFREE_CONTROL_PORT 覆盖。
pub const DEFAULT_PORT: u16 = 8791;
const TOKEN_HEADER: &str = "x-grokfree-token";
const MAX_BODY: usize = 4 * 1024 * 1024;

/// 启动控制口。绑定失败只记日志，不影响主程序。
pub fn spawn(app: AppHandle, desktop: Arc<StdMutex<DesktopState>>, supervisor: Arc<Supervisor>) {
    let port = std::env::var("GROKFREE_CONTROL_PORT")
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let token = uuid::Uuid::new_v4().simple().to_string();
    let inflight: Arc<StdMutex<HashSet<String>>> = Arc::new(StdMutex::new(HashSet::new()));

    tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("控制口绑定 127.0.0.1:{port} 失败（已跳过，不影响主程序）：{e}");
                return;
            }
        };
        let actual = listener
            .local_addr()
            .map(|a| a.port())
            .unwrap_or(port);
        write_discovery(actual, &token);
        tracing::info!("控制口就绪：http://127.0.0.1:{actual}（token 见 control.json）");

        loop {
            match listener.accept().await {
                Ok((stream, _peer)) => {
                    let app = app.clone();
                    let desktop = desktop.clone();
                    let supervisor = supervisor.clone();
                    let token = token.clone();
                    let inflight = inflight.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) =
                            serve(stream, app, desktop, supervisor, token, inflight).await
                        {
                            tracing::debug!("控制口请求处理失败：{e}");
                        }
                    });
                }
                Err(e) => tracing::warn!("控制口 accept 失败：{e}"),
            }
        }
    });
}

fn write_discovery(port: u16, token: &str) {
    let dir = paths::desktop_data_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("控制口：创建数据目录失败 {e}");
        return;
    }
    let payload = json!({
        "port": port,
        "token": token,
        "pid": std::process::id(),
        "url": format!("http://127.0.0.1:{port}"),
        "startedAt": chrono::Utc::now().to_rfc3339(),
    });
    let path = dir.join("control.json");
    match serde_json::to_string_pretty(&payload) {
        Ok(s) => {
            if let Err(e) = std::fs::write(&path, s) {
                tracing::warn!("控制口：写 {} 失败 {e}", path.display());
            }
        }
        Err(e) => tracing::warn!("控制口：序列化发现文件失败 {e}"),
    }
}

// ── 纯函数：路由决策，单测不碰网络 ────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectKey {
    pub id: String,
    pub name: String,
    pub cwd: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveBrief {
    pub id: String,
    pub project_id: String,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PromptPlan {
    /// 点名一个已有会话。project 可省略。
    Session { session_id: String, text: String },
    /// 按项目复用空闲会话，或新建。
    Project {
        project: String,
        text: String,
        force_new: bool,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum SessionChoice {
    Reuse(String),
    Create,
    Busy,
    NotFound,
}

pub fn header_authorized(headers: &[(String, String)], token: &str) -> bool {
    for (k, v) in headers {
        if k.eq_ignore_ascii_case(TOKEN_HEADER) && v.trim() == token {
            return true;
        }
        if k.eq_ignore_ascii_case("authorization") {
            if let Some(rest) = v.trim().strip_prefix("Bearer ") {
                if rest.trim() == token {
                    return true;
                }
            }
        }
    }
    false
}

pub fn plan_prompt(body: &Value) -> Result<PromptPlan, &'static str> {
    let text = body
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if text.is_empty() {
        return Err("text 不能为空");
    }
    let session_id = body
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if !session_id.is_empty() {
        return Ok(PromptPlan::Session { session_id, text });
    }
    let project = body
        .get("project")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if project.is_empty() {
        return Err("project 不能为空");
    }
    let force_new = body
        .get("newSession")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(PromptPlan::Project {
        project,
        text,
        force_new,
    })
}

pub fn match_project<'a>(
    projects: &'a [ProjectKey],
    wanted: &str,
) -> Result<&'a ProjectKey, String> {
    let wanted = wanted.trim();
    if wanted.is_empty() {
        return Err("project 不能为空".into());
    }
    projects
        .iter()
        .find(|p| {
            p.id == wanted
                || p.name.eq_ignore_ascii_case(wanted)
                || paths_eq(&p.cwd, wanted)
        })
        .ok_or_else(|| format!("找不到项目：{wanted}"))
}

/// 显式 sessionId 可以复用出错会话；省略时只复用同项目的 idle，绝不碰 error。
pub fn choose_session(
    live: &[LiveBrief],
    project_id: &str,
    explicit: Option<&str>,
    force_new: bool,
) -> SessionChoice {
    if let Some(sid) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        let Some(s) = live.iter().find(|s| s.id == sid) else {
            return SessionChoice::NotFound;
        };
        if is_turn_busy(&s.status) {
            return SessionChoice::Busy;
        }
        return SessionChoice::Reuse(s.id.clone());
    }
    if force_new {
        return SessionChoice::Create;
    }
    if let Some(s) = live
        .iter()
        .find(|s| s.project_id == project_id && s.status == "idle")
    {
        return SessionChoice::Reuse(s.id.clone());
    }
    SessionChoice::Create
}

fn is_turn_busy(status: &str) -> bool {
    matches!(status, "running" | "waiting_permission" | "starting")
}

fn paths_eq(a: &str, b: &str) -> bool {
    norm_path(a) == norm_path(b)
}

fn norm_path(p: &str) -> String {
    p.replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

#[derive(Debug, PartialEq, Eq)]
pub struct PermissionPlan {
    pub session_id: String,
    pub allow: bool,
    pub remember: bool,
}

pub fn plan_permission(body: &Value) -> Result<PermissionPlan, &'static str> {
    let session_id = body
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if session_id.is_empty() {
        return Err("缺少 sessionId");
    }
    let allow = body
        .get("allow")
        .and_then(|v| v.as_bool())
        .ok_or("缺少 allow")?;
    let remember = body
        .get("remember")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(PermissionPlan {
        session_id,
        allow,
        remember,
    })
}

fn project_key(p: &Project) -> ProjectKey {
    ProjectKey {
        id: p.id.clone(),
        name: p.name.clone(),
        cwd: p.cwd.clone(),
    }
}

// ── HTTP ─────────────────────────────────────────────────────────────────

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn json(&self) -> Option<Value> {
        if self.body.is_empty() {
            return None;
        }
        serde_json::from_slice::<Value>(&self.body).ok()
    }
}

async fn serve(
    mut stream: TcpStream,
    app: AppHandle,
    desktop: Arc<StdMutex<DesktopState>>,
    supervisor: Arc<Supervisor>,
    token: String,
    inflight: Arc<StdMutex<HashSet<String>>>,
) -> std::io::Result<()> {
    let req = match read_request(&mut stream).await {
        Some(r) => r,
        None => return Ok(()),
    };

    if !header_authorized(&req.headers, &token) {
        return respond(&mut stream, 401, &json!({ "error": "unauthorized" })).await;
    }

    let path = req.path.split('?').next().unwrap_or("").to_string();

    if req.method == "GET" {
        if let Some(id) = path.strip_prefix("/sessions/") {
            if !id.is_empty() && !id.contains('/') {
                return session_detail(&mut stream, &desktop, &supervisor, id).await;
            }
        }
    }

    match (req.method.as_str(), path.as_str()) {
        ("GET", "/health") => {
            respond(
                &mut stream,
                200,
                &json!({
                    "ok": true,
                    "app": "grokfree",
                    "version": env!("CARGO_PKG_VERSION"),
                    "pid": std::process::id(),
                }),
            )
            .await
        }

        ("GET", "/projects") => {
            let projects: Vec<Value> = {
                let st = desktop.lock();
                st.projects
                    .iter()
                    .map(|p| json!({ "id": p.id, "name": p.name, "cwd": p.cwd }))
                    .collect()
            };
            respond(&mut stream, 200, &json!({ "projects": projects })).await
        }

        ("GET", "/sessions") => {
            let live = supervisor.list_live().await;
            let items: Vec<Value> = live.iter().map(live_json).collect();
            respond(&mut stream, 200, &json!({ "sessions": items })).await
        }

        ("POST", "/prompt") => {
            handle_prompt(&mut stream, app, desktop, supervisor, inflight, &req).await
        }

        ("POST", "/cancel") => {
            let body = req.json().unwrap_or(Value::Null);
            let sid = body
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if sid.is_empty() {
                return respond(&mut stream, 400, &json!({ "error": "缺少 sessionId" })).await;
            }
            match supervisor.cancel(&sid).await {
                Ok(()) => respond(&mut stream, 200, &json!({ "ok": true })).await,
                Err(e) => {
                    respond(
                        &mut stream,
                        500,
                        &json!({ "ok": false, "error": e.to_string() }),
                    )
                    .await
                }
            }
        }

        ("POST", "/permission") => {
            let body = req.json().unwrap_or(Value::Null);
            let plan = match plan_permission(&body) {
                Ok(p) => p,
                Err(e) => {
                    return respond(&mut stream, 400, &json!({ "error": e })).await;
                }
            };
            match supervisor
                .respond_pending_permission(app, &plan.session_id, plan.allow, plan.remember)
                .await
            {
                Ok(()) => respond(&mut stream, 200, &json!({ "ok": true })).await,
                Err(e) => {
                    let msg = e.to_string();
                    let status = if msg.contains("没有待处理") { 409 } else { 500 };
                    respond(&mut stream, status, &json!({ "ok": false, "error": msg })).await
                }
            }
        }

        _ => respond(&mut stream, 404, &json!({ "error": "no such route" })).await,
    }
}

fn live_json(s: &crate::supervisor::LiveSession) -> Value {
    json!({
        "id": s.id,
        "projectId": s.project_id,
        "title": s.title,
        "cwd": s.cwd,
        "status": s.status,
        "agentId": s.agent_id,
        "error": s.error,
        "live": true,
        "busy": is_turn_busy(&s.status),
    })
}

async fn session_detail(
    stream: &mut TcpStream,
    desktop: &Arc<StdMutex<DesktopState>>,
    supervisor: &Supervisor,
    id: &str,
) -> std::io::Result<()> {
    let live = supervisor.list_live().await;
    if let Some(s) = live.iter().find(|s| s.id == id) {
        let mut body = live_json(s);
        if let Some((scope, _)) = supervisor.permission_pending(id).await {
            body["permission"] = json!({ "pending": true, "scopeKey": scope });
        }
        return respond(stream, 200, &body).await;
    }
    let meta = {
        let st = desktop.lock();
        st.sessions.iter().find(|s| s.id == id).map(|s| {
            json!({
                "id": s.id,
                "projectId": s.project_id,
                "title": s.title,
                "cwd": s.cwd,
                "status": s.status,
                "agentId": s.agent_id,
                "error": null,
                "live": false,
                "busy": false,
            })
        })
    };
    match meta {
        Some(body) => respond(stream, 200, &body).await,
        None => respond(stream, 404, &json!({ "error": "找不到会话" })).await,
    }
}

async fn handle_prompt(
    stream: &mut TcpStream,
    app: AppHandle,
    desktop: Arc<StdMutex<DesktopState>>,
    supervisor: Arc<Supervisor>,
    inflight: Arc<StdMutex<HashSet<String>>>,
    req: &Request,
) -> std::io::Result<()> {
    let body = req.json().unwrap_or(Value::Null);
    let plan = match plan_prompt(&body) {
        Ok(p) => p,
        Err(e) => return respond(stream, 400, &json!({ "error": e })).await,
    };

    let (text, choice_project, explicit, force_new) = match plan {
        PromptPlan::Session { session_id, text } => (text, None, Some(session_id), false),
        PromptPlan::Project {
            project,
            text,
            force_new,
        } => (text, Some(project), None, force_new),
    };

    let project = if let Some(wanted) = choice_project {
        let keys: Vec<ProjectKey> = {
            let st = desktop.lock();
            st.projects.iter().map(project_key).collect()
        };
        match match_project(&keys, &wanted) {
            Ok(p) => Some(p.clone()),
            Err(e) => return respond(stream, 404, &json!({ "error": e })).await,
        }
    } else {
        None
    };

    let live = supervisor.list_live().await;
    let briefs: Vec<LiveBrief> = live
        .iter()
        .map(|s| LiveBrief {
            id: s.id.clone(),
            project_id: s.project_id.clone(),
            status: s.status.clone(),
        })
        .collect();
    let project_id = project.as_ref().map(|p| p.id.as_str()).unwrap_or("");
    let choice = choose_session(&briefs, project_id, explicit.as_deref(), force_new);

    let session_id = match choice {
        SessionChoice::Busy => {
            return respond(
                stream,
                409,
                &json!({ "error": "会话正在执行中，请等待本轮完成或先取消" }),
            )
            .await;
        }
        SessionChoice::NotFound => {
            return respond(
                stream,
                404,
                &json!({ "error": "会话不存在或已休眠" }),
            )
            .await;
        }
        SessionChoice::Reuse(id) => id,
        SessionChoice::Create => {
            let Some(p) = project.as_ref() else {
                return respond(stream, 400, &json!({ "error": "project 不能为空" })).await;
            };
            match supervisor
                .create_session(app.clone(), p.id.clone(), p.cwd.clone(), None, None, None, None)
                .await
            {
                Ok(s) => s.id,
                Err(e) => {
                    let msg = e.to_string();
                    let status = if msg.contains("正在启动") { 409 } else { 500 };
                    return respond(stream, status, &json!({ "error": msg })).await;
                }
            }
        }
    };

    if let Some(msg) = supervisor.turn_conflict(&session_id).await {
        let status = if msg.contains("正在执行") { 409 } else { 500 };
        return respond(stream, status, &json!({ "error": msg })).await;
    }
    let reserved = {
        let mut guard = inflight.lock();
        guard.insert(session_id.clone())
    };
    if !reserved {
        return respond(
            stream,
            409,
            &json!({ "error": "会话正在执行中，请等待本轮完成或先取消" }),
        )
        .await;
    }

    let _ = app.emit(
        "agent://userPrompt",
        json!({ "sessionId": session_id, "text": text }),
    );

    {
        let sup = supervisor.clone();
        let app2 = app.clone();
        let sid2 = session_id.clone();
        let text2 = text.clone();
        let inflight2 = inflight.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = sup.send_prompt(app2.clone(), &sid2, &text2).await {
                tracing::warn!("控制口：prompt 派发后失败 session={sid2} err={e}");
                let _ = app2.emit(
                    "agent://userPromptFailed",
                    json!({ "sessionId": sid2, "error": e.to_string() }),
                );
            }
            inflight2.lock().remove(&sid2);
        });
    }

    let project_id = live
        .iter()
        .find(|s| s.id == session_id)
        .map(|s| s.project_id.clone())
        .or_else(|| project.map(|p| p.id));

    respond(
        stream,
        200,
        &json!({
            "ok": true,
            "dispatched": true,
            "sessionId": session_id,
            "projectId": project_id,
        }),
    )
    .await
}

async fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];

    let end = loop {
        if let Some(pos) = find_headers_end(&buf) {
            break pos;
        }
        if buf.len() > 64 * 1024 {
            return None;
        }
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();

    let mut headers = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }

    let content_length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0)
        .min(MAX_BODY);

    let body_start = end + 4;
    let mut body = buf.get(body_start..).unwrap_or(&[]).to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);

    Some(Request {
        method,
        path,
        headers,
        body,
    })
}

fn find_headers_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

async fn respond(stream: &mut TcpStream, status: u16, payload: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(payload).unwrap_or_else(|_| b"{}".to_vec());
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        _ => "Internal Server Error",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&body).await?;
    stream.flush().await?;
    let _ = stream.shutdown().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proj(id: &str, name: &str, cwd: &str) -> ProjectKey {
        ProjectKey {
            id: id.into(),
            name: name.into(),
            cwd: cwd.into(),
        }
    }

    #[test]
    fn auth_accepts_header_or_bearer_only() {
        let headers = vec![("X-GrokFree-Token".into(), "abc".into())];
        assert!(header_authorized(&headers, "abc"));
        assert!(!header_authorized(&headers, "nope"));
        let bearer = vec![("Authorization".into(), "Bearer abc".into())];
        assert!(header_authorized(&bearer, "abc"));
        let bad = vec![("Authorization".into(), "abc".into())];
        assert!(!header_authorized(&bad, "abc"));
    }

    #[test]
    fn empty_project_is_rejected() {
        let err = plan_prompt(&json!({ "text": "hi" })).unwrap_err();
        assert!(err.contains("project"));
        assert!(match_project(&[], "").is_err());
        assert!(match_project(&[proj("a", "A", "D:\\work")], "").is_err());
    }

    #[test]
    fn project_matches_id_name_or_path() {
        let list = vec![proj("p1", "Demo", r"D:\Work\Demo")];
        assert_eq!(match_project(&list, "p1").unwrap().id, "p1");
        assert_eq!(match_project(&list, "demo").unwrap().id, "p1");
        assert_eq!(
            match_project(&list, "D:/work/demo").unwrap().id,
            "p1"
        );
        assert!(match_project(&list, "missing").is_err());
    }

    #[test]
    fn reuse_idle_only_unless_session_named() {
        let live = vec![
            LiveBrief {
                id: "idle1".into(),
                project_id: "p".into(),
                status: "idle".into(),
            },
            LiveBrief {
                id: "bad".into(),
                project_id: "p".into(),
                status: "error".into(),
            },
            LiveBrief {
                id: "run".into(),
                project_id: "p".into(),
                status: "running".into(),
            },
        ];
        assert_eq!(
            choose_session(&live, "p", None, false),
            SessionChoice::Reuse("idle1".into())
        );
        assert_eq!(
            choose_session(&live, "p", None, true),
            SessionChoice::Create
        );
        assert_eq!(
            choose_session(&live, "p", Some("bad"), false),
            SessionChoice::Reuse("bad".into())
        );
        assert_eq!(
            choose_session(&live, "p", Some("run"), false),
            SessionChoice::Busy
        );
        assert_eq!(
            choose_session(&live, "p", Some("gone"), false),
            SessionChoice::NotFound
        );
        let only_err = vec![LiveBrief {
            id: "bad".into(),
            project_id: "p".into(),
            status: "error".into(),
        }];
        assert_eq!(
            choose_session(&only_err, "p", None, false),
            SessionChoice::Create
        );
    }

    #[test]
    fn permission_plan_requires_bool() {
        assert!(plan_permission(&json!({ "sessionId": "s" })).is_err());
        let p = plan_permission(&json!({ "sessionId": "s", "allow": true, "remember": true }))
            .unwrap();
        assert!(p.allow && p.remember);
    }
}
