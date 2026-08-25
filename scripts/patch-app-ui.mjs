import fs from "fs";

const path = "src/App.tsx";
let s = fs.readFileSync(path, "utf8");

const startMarker = '    <div className={`app ${showReview ? "with-review" : ""}`}>';
const endMarker = "      {showSettings && state && (";
const start = s.indexOf(startMarker);
const end = s.indexOf(endMarker);
if (start < 0 || end < 0) {
  console.error("markers not found", { start, end });
  process.exit(1);
}

// Include the `return (` line before startMarker
const returnIdx = s.lastIndexOf("  return (", start);
if (returnIdx < 0) {
  console.error("return not found");
  process.exit(1);
}

const newJsx = `  return (
    <div className={\`app \${showReview ? "with-review" : ""}\`}>
      <aside className="sidebar">
        <div className="sidebar-header">
          <div className="logo-mark">G</div>
          <div className="logo-text">
            <strong>Grok Build</strong>
            <span className="logo-version-row">
              <span>
                Desktop · v{localUpdate?.currentVersion || APP_VERSION}
              </span>
              {localUpdate?.isNewer && (
                <button
                  type="button"
                  className="version-update-btn"
                  disabled={updateBusy}
                  title={\`发现新版本 v\${localUpdate.version} — 点击安装\`}
                  onClick={() => void launchLocalUpdate()}
                >
                  {updateBusy ? "…" : "更新"}
                </button>
              )}
            </span>
          </div>
        </div>

        <div className="sidebar-section">
          <div className="section-label">
            项目
            <button
              type="button"
              className="icon-btn"
              title="添加项目"
              onClick={addProject}
            >
              <IconPlus />
            </button>
          </div>
          <div className="project-list" style={{ maxHeight: 160 }}>
            {projects.length === 0 && (
              <div className="list-empty">暂无项目，请添加文件夹</div>
            )}
            {projects.map((p) => (
              <div
                key={p.id}
                className={\`project-item \${
                  p.id === activeProjectId ? "active" : ""
                }\`}
              >
                <button
                  type="button"
                  className="project-item-body"
                  onClick={() => {
                    setActiveProjectId(p.id);
                    setShowDashboard(false);
                    setProjectMenuId(null);
                  }}
                  onDoubleClick={() => void createSession(p)}
                  title="双击新建会话"
                >
                  <span className="name">{p.name}</span>
                  <span className="path">{p.cwd}</span>
                </button>
                <div className="menu-shell">
                  <button
                    type="button"
                    className="icon-btn more"
                    title="项目操作"
                    onClick={(e) => {
                      e.stopPropagation();
                      setProjectMenuId((cur) =>
                        cur === p.id ? null : p.id
                      );
                      setSessionMenuId(null);
                      setTopMenuOpen(false);
                    }}
                  >
                    <IconMore />
                  </button>
                  <OverflowMenu
                    open={projectMenuId === p.id}
                    onClose={() => setProjectMenuId(null)}
                    align="right"
                    items={[
                      {
                        id: "new",
                        label: "新建会话",
                        onSelect: () => void createSession(p),
                      },
                      {
                        id: "remove",
                        label: "移除项目",
                        danger: true,
                        onSelect: () => removeProjectById(p.id, p.name),
                      },
                    ]}
                  />
                </div>
              </div>
            ))}
          </div>
        </div>

        <div
          className="sidebar-section"
          style={{
            flex: 1,
            minHeight: 0,
            display: "flex",
            flexDirection: "column",
          }}
        >
          <div className="section-label">
            会话
            <span style={{ display: "flex", gap: 2 }}>
              <button
                type="button"
                className="icon-btn"
                title="清理空/占位会话"
                onClick={() => void purgePlaceholderSessions()}
              >
                <IconBroom />
              </button>
              <button
                type="button"
                className="icon-btn"
                title="磁盘历史"
                onClick={() => void loadDiskHistory()}
              >
                <IconHistory />
              </button>
              <button
                type="button"
                className="icon-btn"
                title="新建会话 (Ctrl+N)"
                disabled={!activeProject || busy}
                onClick={() => void createSession()}
              >
                <IconPlus />
              </button>
            </span>
          </div>
          <input
            className="session-search"
            placeholder="搜索会话…"
            value={sessionQuery}
            onChange={(e) => setSessionQuery(e.target.value)}
          />
          <div className="session-list" style={{ flex: 1 }}>
            {projectSessions.fromLive.length === 0 &&
              projectSessions.fromMeta.length === 0 && (
                <div className="list-empty">
                  暂无会话。选中项目后点「新建会话」
                </div>
              )}
            {projectSessions.fromLive.length > 0 && (
              <div className="session-group-label">运行中</div>
            )}
            {projectSessions.fromLive.map((s) => (
              <div
                key={s.id}
                className={\`session-item \${
                  s.id === activeSessionId ? "active" : ""
                }\`}
              >
                <button
                  type="button"
                  className="session-item-body"
                  onClick={() => {
                    setActiveSessionId(s.id);
                    setShowDashboard(false);
                  }}
                  onDoubleClick={() => void renameSessionById(s.id, s.title)}
                  title="双击重命名 · Ctrl+1..9 快速切换"
                >
                  <span className="name">
                    <span
                      className={\`status-dot \${
                        s.status === "running" || s.status === "starting"
                          ? "running"
                          : s.status === "error"
                            ? "error"
                            : s.status === "waiting_permission"
                              ? "needs-input"
                              : ""
                      }\`}
                    />
                    {s.title}
                  </span>
                  <span className="meta">
                    {statusLabel(s.status)}
                  </span>
                </button>
                <div className="menu-shell">
                  <button
                    type="button"
                    className="session-item-del"
                    title="更多"
                    onClick={(e) => {
                      e.stopPropagation();
                      setSessionMenuId((cur) =>
                        cur === s.id ? null : s.id
                      );
                      setProjectMenuId(null);
                      setTopMenuOpen(false);
                    }}
                  >
                    <IconMore size={14} />
                  </button>
                  <OverflowMenu
                    open={sessionMenuId === s.id}
                    onClose={() => setSessionMenuId(null)}
                    align="right"
                    items={[
                      {
                        id: "rename",
                        label: "重命名",
                        onSelect: () =>
                          void renameSessionById(s.id, s.title),
                      },
                      {
                        id: "hibernate",
                        label: "休眠",
                        onSelect: () => void hibernate(s.id),
                      },
                      {
                        id: "delete",
                        label: "删除",
                        danger: true,
                        onSelect: () => void deleteSession(s.id, s.title),
                      },
                    ]}
                  />
                </div>
              </div>
            ))}
            {projectSessions.fromMeta.length > 0 && (
              <div className="session-group-label">休眠</div>
            )}
            {projectSessions.fromMeta.map((s) => (
              <div key={s.id} className="session-item">
                <button
                  type="button"
                  className="session-item-body"
                  onClick={() => void resumeMeta(s)}
                  title="点击恢复"
                >
                  <span className="name">
                    <span className="status-dot" />
                    {s.title}
                  </span>
                  <span className="meta">
                    休眠
                    {s.lastActiveAt
                      ? \` · \${relativeTime(s.lastActiveAt)}\`
                      : ""}
                  </span>
                </button>
                <div className="menu-shell">
                  <button
                    type="button"
                    className="session-item-del"
                    title="更多"
                    onClick={(e) => {
                      e.stopPropagation();
                      setSessionMenuId((cur) =>
                        cur === s.id ? null : s.id
                      );
                      setProjectMenuId(null);
                      setTopMenuOpen(false);
                    }}
                  >
                    <IconMore size={14} />
                  </button>
                  <OverflowMenu
                    open={sessionMenuId === s.id}
                    onClose={() => setSessionMenuId(null)}
                    align="right"
                    items={[
                      {
                        id: "resume",
                        label: "恢复",
                        onSelect: () => void resumeMeta(s),
                      },
                      {
                        id: "delete",
                        label: "删除",
                        danger: true,
                        onSelect: () => void deleteSession(s.id, s.title),
                      },
                    ]}
                  />
                </div>
              </div>
            ))}
          </div>
        </div>

        <div className="sidebar-footer">
          <div
            className={\`agent-status \${
              env?.grokExists && env.cliVersionOk ? "online" : ""
            }\`}
          >
            <span className="pulse" />
            <span style={{ flex: 1 }}>
              {!env?.grokExists
                ? "未检测到 CLI"
                : !env.cliVersionOk
                  ? \`CLI 过旧 · \${env.grokVersion || "?"}\`
                  : env.grokVersion || "CLI 已就绪"}
            </span>
          </div>
          <div className="sidebar-footer-actions">
            <button
              type="button"
              className="btn"
              onClick={() => {
                setShowDashboard(true);
                setActiveSessionId(null);
              }}
              title="指挥中心"
            >
              <IconGrid size={14} />
              总览
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => setShowSettings(true)}
            >
              <IconSettings size={14} />
              设置
            </button>
          </div>
        </div>
      </aside>

      <main className="main">
        <div className="topbar">
          <div style={{ minWidth: 0 }}>
            <h1>
              {showDashboard
                ? "指挥中心"
                : activeLive?.title ||
                  activeProject?.name ||
                  "Grok Build Desktop"}
            </h1>
            <div className="cwd" title={activeLive?.cwd || activeProject?.cwd || ""}>
              {showDashboard
                ? \`\${live.length} 个活跃会话\`
                : activeLive?.cwd ||
                  activeProject?.cwd ||
                  "请选择项目，然后启动会话"}
            </div>
          </div>
          <div className="topbar-actions">
            <span
              className={\`mode-pill \${
                env && env.grokExists && !env.cliVersionOk ? "warn" : ""
              }\`}
              title={
                prefs.sandboxMode !== "off"
                  ? \`沙箱偏好（仅说明）：\${sandboxModeLabel(prefs.sandboxMode)}\`
                  : "权限模式"
              }
            >
              {permissionModeLabel(prefs.permissionMode)}
              {env && env.grokExists && !env.cliVersionOk
                ? " · CLI 门禁"
                : ""}
            </span>
            <button
              type="button"
              className={\`btn sm icon-label \${showReview ? "primary" : ""}\`}
              onClick={() => setShowReview((v) => !v)}
              title="Ctrl+B"
            >
              <IconReview size={14} />
              审查
              {diffs.filter((d) => !d.decision || d.decision === "pending")
                .length > 0 && (
                <span className="badge-count">
                  {
                    diffs.filter(
                      (d) => !d.decision || d.decision === "pending"
                    ).length
                  }
                </span>
              )}
            </button>
            <button
              type="button"
              className="btn sm primary"
              disabled={!activeProject || busy}
              onClick={() => void createSession()}
            >
              {busy ? "启动中…" : "新建会话"}
            </button>
            <div className="menu-shell">
              <button
                type="button"
                className="btn sm"
                title="更多操作"
                onClick={() => {
                  setTopMenuOpen((v) => !v);
                  setProjectMenuId(null);
                  setSessionMenuId(null);
                }}
              >
                <IconMore />
              </button>
              <OverflowMenu
                open={topMenuOpen}
                onClose={() => setTopMenuOpen(false)}
                align="right"
                items={[
                  ...(cwd
                    ? [
                        {
                          id: "terminal",
                          label: "外部终端",
                          onSelect: () =>
                            void api
                              .openExternalTerminal(cwd)
                              .catch((e) => flash(String(e), "error")),
                        },
                        {
                          id: "editor",
                          label: "打开编辑器",
                          onSelect: () =>
                            void api
                              .openInEditor(cwd)
                              .catch((e) => flash(String(e), "error")),
                        },
                      ]
                    : []),
                  ...(activeLive
                    ? [
                        {
                          id: "rename",
                          label: "重命名会话",
                          onSelect: () => void renameActive(),
                        },
                        {
                          id: "hibernate",
                          label: "休眠会话",
                          onSelect: () => void hibernate(activeLive.id),
                        },
                        {
                          id: "delete",
                          label: "删除会话",
                          danger: true,
                          onSelect: () =>
                            void deleteSession(
                              activeLive.id,
                              activeLive.title
                            ),
                        },
                      ]
                    : []),
                  {
                    id: "history",
                    label: "磁盘历史",
                    onSelect: () => void loadDiskHistory(),
                  },
                  {
                    id: "settings",
                    label: "设置",
                    onSelect: () => setShowSettings(true),
                  },
                ]}
              />
            </div>
          </div>
        </div>

        {activeLive?.status === "error" && (
          <div className="error-bar">
            <span className="msg">
              会话出错：{activeLive.error || "未知错误"}
            </span>
            <button
              type="button"
              className="btn sm"
              disabled={busy}
              onClick={() => void restartSession()}
            >
              重启会话
            </button>
            <button
              type="button"
              className="btn sm"
              onClick={() => void api.revealLogs()}
            >
              查看日志
            </button>
            <button
              type="button"
              className="btn sm"
              onClick={async () => {
                try {
                  const dir = await api.exportDiagnostics();
                  flash(\`诊断包已导出：\${dir}\`, "success");
                } catch (e) {
                  flash(\`导出失败：\${e}\`, "error");
                }
              }}
            >
              导出诊断
            </button>
          </div>
        )}

        <div className="chat-scroll" ref={scrollRef}>
          {showDashboard ? (
            <Dashboard
              live={live}
              projects={projects}
              activeSessionId={activeSessionId}
              onSelect={(id) => {
                setActiveSessionId(id);
                setShowDashboard(false);
              }}
              onNew={() => void createSession()}
              onHibernate={(id) => void hibernate(id)}
              onDelete={(id, title) => void deleteSession(id, title)}
            />
          ) : !activeSessionId ? (
            <div className="empty-workspace">
              <div className="empty-illustration" aria-hidden>
                <span className="empty-orb" />
              </div>
              <h2>开始工作</h2>
              <p>
                {activeProject
                  ? \`当前项目「\${activeProject.name}」。新建会话或从磁盘历史恢复对话。\`
                  : "请先在左侧添加并选择一个项目，然后新建会话。"}
              </p>
              <div className="empty-workspace-actions">
                <button
                  type="button"
                  className="btn primary"
                  disabled={!activeProject || busy}
                  onClick={() => void createSession()}
                >
                  新建会话
                </button>
                <button
                  type="button"
                  className="btn"
                  onClick={() => void loadDiskHistory()}
                >
                  磁盘历史
                </button>
                <button
                  type="button"
                  className="btn ghost"
                  onClick={() => {
                    setShowDashboard(true);
                  }}
                >
                  打开总览
                </button>
              </div>
            </div>
          ) : (
            <>
              {showPlanBanner && plan && (
                <PlanBanner
                  plan={plan}
                  onDismiss={() =>
                    setPlanDismissed((p) => ({
                      ...p,
                      [activeSessionId!]: true,
                    }))
                  }
                  onConfirm={() => {
                    setInput("请按计划执行");
                    setPlanDismissed((p) => ({
                      ...p,
                      [activeSessionId!]: true,
                    }));
                    flash("已填入确认指令，按 Enter 发送", "info");
                  }}
                />
              )}
              {blocks.length > 0 && (
                <TranscriptToolbar
                  filter={transcriptFilter}
                  onFilter={setTranscriptFilter}
                  count={blocks.length}
                  onScrollBottom={() => {
                    const el = scrollRef.current;
                    if (el) el.scrollTop = el.scrollHeight;
                  }}
                />
              )}
              <ChatBlocks
                blocks={blocks}
                showRawAcpEvents={prefs.showRawAcpEvents === true}
                filter={transcriptFilter}
              />
            </>
          )}
        </div>

        <div className="composer-wrap">
          <div className="composer">
            <textarea
              placeholder={
                activeSessionId
                  ? "向 Grok 发送消息… Enter 发送 · Shift+Enter 换行 · Ctrl+N 新建"
                  : "请先新建或恢复一个会话"
              }
              value={input}
              disabled={!activeSessionId || busy}
              rows={2}
              onChange={(e) => {
                setInput(e.target.value);
                const ta = e.target;
                ta.style.height = "auto";
                ta.style.height = \`\${Math.min(ta.scrollHeight, 200)}px\`;
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  void sendPrompt();
                }
              }}
            />
            <div className="composer-bar">
              <span className="hint">{statusLabel(activeLive?.status)}</span>
              <span className="spacer" />
              {(activeLive?.status === "running" || busy) && activeLive && (
                <button
                  type="button"
                  className="btn sm danger"
                  onClick={() =>
                    void api
                      .cancelPrompt(activeLive.id)
                      .then(() => flash("已请求停止", "info"))
                      .catch((e) => flash(String(e), "error"))
                  }
                >
                  停止
                </button>
              )}
              <button
                type="button"
                className="btn sm primary"
                disabled={!activeSessionId || !input.trim() || busy}
                onClick={() => void sendPrompt()}
              >
                发送
              </button>
            </div>
          </div>
        </div>
      </main>

      {showReview && (
        <div className="review-col">
          <div
            className="review-resize"
            onMouseDown={(e) => {
              e.preventDefault();
              resizing.current = true;
            }}
            title="拖拽调整审查面板宽度"
          />
          <ReviewPane
            diffs={diffs}
            git={git}
            busyPath={diffBusyPath}
            batchBusy={batchBusy}
            onClose={() => setShowReview(false)}
            onOpen={(path) => void api.openInEditor(path)}
            onAccept={(path, patch) => void acceptDiff(path, patch)}
            onReject={(path) => void rejectDiff(path)}
            onAcceptAll={() => void acceptAllPending()}
            onRejectAll={() => void rejectAllPending()}
            onRefreshGit={() => void refreshGit(undefined, true)}
            onOpenProject={
              cwd
                ? () =>
                    void api
                      .openInEditor(cwd)
                      .catch((e) => flash(String(e), "error"))
                : undefined
            }
            onCopyPatch={(path, patch) => void copyPatch(path, patch)}
          />
        </div>
      )}

`;

const before = s.slice(0, returnIdx);
const after = s.slice(end);
// Insert confirm dialog before showSettings
const confirmBlock = `      <ConfirmDialog
        open={!!confirm}
        title={confirm?.title || ""}
        message={confirm?.message || ""}
        danger={confirm?.danger}
        confirmLabel={confirm?.confirmLabel}
        busy={confirmBusy}
        onCancel={() => {
          if (!confirmBusy) setConfirm(null);
        }}
        onConfirm={() => void runConfirm()}
      />

`;

s = before + newJsx + confirmBlock + after;
fs.writeFileSync(path, s);
console.log("patched App.tsx, bytes", s.length);
`;

// Wait - the template literal for newJsx uses nested template strings. Writing as a .mjs with carefully escaped content is hard.
// Simpler: write newJsx to a separate file and assemble.

console.log("script template written - will use alternate approach");
