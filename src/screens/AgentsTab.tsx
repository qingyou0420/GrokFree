import { useState } from "react";
import { DEFAULT_MODEL, MODEL_OPTIONS } from "../lib/models";
import type { AgentProfile } from "../lib/types";

/** grok 档案模型下拉：常用档 + 自定义 ID */
function GrokModelSelect({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const known = MODEL_OPTIONS.some((o) => o.id === value);
  return (
    <>
      <select
        value={!value.trim() ? "" : known ? value : "__custom__"}
        onChange={(e) => {
          const v = e.target.value;
          if (v === "__custom__") {
            onChange(value.trim() || DEFAULT_MODEL);
            return;
          }
          onChange(v);
        }}
      >
        <option value="">跟随 CLI 默认</option>
        {MODEL_OPTIONS.map((o) => (
          <option key={o.id} value={o.id}>
            {o.label}
            {o.hint ? ` · ${o.hint}` : ""}
          </option>
        ))}
        <option value="__custom__">自定义 ID…</option>
      </select>
      {value.trim() && !known && (
        <input
          style={{ marginTop: 4 }}
          value={value}
          placeholder="例如 grok-4.6"
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </>
  );
}

/**
 * 设置 → 智能体：agents.json 的增删改。
 * 密钥以明文存在本地 agents.json（自用机，可接受）；
 * args 用空格分隔一行，env 用 KEY=value 一行一条。
 */

function blankProfile(): AgentProfile {
  return {
    id: "",
    name: "",
    command: "npx",
    args: [],
    env: {},
    models: [],
    defaultModel: "",
    isGrok: false,
    supportsResume: false,
    enabled: false,
    note: "",
  };
}

function envToText(env: Record<string, string>): string {
  return Object.entries(env)
    .map(([k, v]) => `${k}=${v}`)
    .join("\n");
}

function textToEnv(text: string): Record<string, string> {
  const env: Record<string, string> = {};
  for (const line of text.split(/\r?\n/)) {
    const t = line.trim();
    if (!t || t.startsWith("#")) continue;
    const eq = t.indexOf("=");
    if (eq <= 0) continue;
    env[t.slice(0, eq).trim()] = t.slice(eq + 1);
  }
  return env;
}

/** 含 KEY/TOKEN/SECRET 的 env 值用密码框展示 */
function isSecretKey(k: string) {
  return /KEY|TOKEN|SECRET|PASSWORD/i.test(k);
}

export function AgentsTab({
  agents,
  onSaveAgents,
  onFlash,
}: {
  agents: AgentProfile[];
  onSaveAgents: (profiles: AgentProfile[]) => Promise<unknown>;
  onFlash?: (text: string, kind?: "info" | "error" | "success") => void;
}) {
  const [draft, setDraft] = useState<AgentProfile[]>(agents);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [envText, setEnvText] = useState<Record<string, string>>(() =>
    Object.fromEntries(agents.map((a) => [a.id, envToText(a.env)]))
  );

  const patch = (id: string, p: Partial<AgentProfile>) => {
    setDraft((prev) => prev.map((a) => (a.id === id ? { ...a, ...p } : a)));
  };

  const addProfile = () => {
    const base = blankProfile();
    let id = "custom";
    let n = 2;
    while (draft.some((a) => a.id === id)) id = `custom-${n++}`;
    base.id = id;
    base.name = "自定义小精灵";
    setDraft((prev) => [...prev, base]);
    setEnvText((prev) => ({ ...prev, [id]: "" }));
    setExpandedId(id);
  };

  const removeProfile = (id: string) => {
    const target = draft.find((a) => a.id === id);
    if (target?.isGrok) {
      onFlash?.("Grok 原生档案不可删除", "error");
      return;
    }
    setDraft((prev) => prev.filter((a) => a.id !== id));
    if (expandedId === id) setExpandedId(null);
  };

  const saveAll = async () => {
    if (saving) return;
    setSaving(true);
    try {
      // env 文本 → env 对象（只对展开过编辑的档案应用当前文本）
      const merged = draft.map((a) => ({
        ...a,
        env: textToEnv(envText[a.id] ?? envToText(a.env)),
        id: a.id.trim(),
        name: a.name.trim() || a.id.trim(),
        args: a.args.map((s) => s.trim()).filter(Boolean),
        models: a.models.map((s) => s.trim()).filter(Boolean),
        defaultModel: a.defaultModel.trim(),
      }));
      await onSaveAgents(merged);
    } finally {
      setSaving(false);
    }
  };

  const dirty = JSON.stringify(draft) !== JSON.stringify(agents);

  return (
    <div className="agents-settings">
      <div className="settings-banner">
        每个档案描述如何启动一个说 ACP 的小精灵（Sprite）。模型经 <code>{"{model}"}</code>{" "}
        占位符注入参数或环境变量；密钥以明文保存在本地 agents.json。
        非 Grok 档案暂不支持磁盘历史恢复。
      </div>

      {draft.map((a) => {
        const expanded = expandedId === a.id;
        return (
          <div key={a.id} className={`agent-card ${a.enabled ? "" : "disabled"}`}>
            <div className="agent-card-head">
              <label className="disk-filter-check" title="启用后出现在侧栏小精灵选择器">
                <input
                  type="checkbox"
                  checked={a.enabled}
                  onChange={(e) => patch(a.id, { enabled: e.target.checked })}
                />
              </label>
              <button
                type="button"
                className="agent-card-title"
                onClick={() => setExpandedId(expanded ? null : a.id)}
              >
                <strong>{a.name || a.id}</strong>
                <span className="meta">
                  {a.id}
                  {a.isGrok ? " · Grok 原生" : ""}
                  {a.enabled ? "" : " · 已停用"}
                </span>
              </button>
              <button
                type="button"
                className="btn sm danger"
                title={a.isGrok ? "Grok 原生档案不可删除" : "删除档案"}
                disabled={a.isGrok}
                onClick={() => removeProfile(a.id)}
              >
                删除
              </button>
            </div>
            {!expanded && a.note && <div className="help agent-note">{a.note}</div>}
            {expanded && (
              <div className="agent-edit-form">
                <label className="field">
                  <span>名称</span>
                  <input
                    value={a.name}
                    onChange={(e) => patch(a.id, { name: e.target.value })}
                  />
                </label>
                <label className="field">
                  <span>id（字母/数字/-/_）</span>
                  <input
                    value={a.id}
                    disabled={a.isGrok}
                    onChange={(e) => {
                      const oldId = a.id;
                      const newId = e.target.value;
                      setDraft((prev) =>
                        prev.map((x) => (x.id === oldId ? { ...x, id: newId } : x))
                      );
                      setEnvText((prev) => {
                        const next = { ...prev };
                        next[newId] = next[oldId] ?? "";
                        delete next[oldId];
                        return next;
                      });
                      setExpandedId(newId);
                    }}
                  />
                </label>
                {!a.isGrok && (
                  <>
                    <label className="field">
                      <span>命令（可执行文件）</span>
                      <input
                        value={a.command}
                        placeholder="npx"
                        onChange={(e) => patch(a.id, { command: e.target.value })}
                      />
                    </label>
                    <label className="field">
                      <span>参数（空格分隔，支持 {"{model}"}）</span>
                      <input
                        value={a.args.join(" ")}
                        placeholder="@agentclientprotocol/claude-agent-acp@latest"
                        onChange={(e) =>
                          patch(a.id, {
                            args: e.target.value.split(/\s+/).filter(Boolean),
                          })
                        }
                      />
                    </label>
                    <label className="field">
                      <span>环境变量（一行一条 KEY=value）</span>
                      <div className="agent-env-list">
                        {Object.entries(textToEnv(envText[a.id] ?? "")).map(
                          ([k, v]) =>
                            isSecretKey(k) ? (
                              <input
                                key={k}
                                type="password"
                                className="agent-env-secret"
                                value={v}
                                placeholder={`${k}=（密钥）`}
                                onChange={(e) => {
                                  const env = textToEnv(envText[a.id] ?? "");
                                  env[k] = e.target.value;
                                  setEnvText((prev) => ({
                                    ...prev,
                                    [a.id]: envToText(env),
                                  }));
                                }}
                              />
                            ) : null
                        )}
                        <textarea
                          rows={Math.max(4, (envText[a.id] ?? "").split(/\r?\n/).length)}
                          value={envText[a.id] ?? ""}
                          onChange={(e) =>
                            setEnvText((prev) => ({ ...prev, [a.id]: e.target.value }))
                          }
                        />
                      </div>
                    </label>
                  </>
                )}
                <label className="field">
                  <span>模型列表（逗号分隔，下拉可选）</span>
                  <input
                    value={a.models.join(", ")}
                    placeholder="glm-4.6, glm-4.5"
                    onChange={(e) =>
                      patch(a.id, {
                        models: e.target.value
                          .split(/[,，]/)
                          .map((s) => s.trim())
                          .filter(Boolean),
                      })
                    }
                  />
                </label>
                <label className="field">
                  <span>默认模型（留空 = CLI 默认）</span>
                  {a.isGrok ? (
                    <GrokModelSelect
                      value={a.defaultModel}
                      onChange={(v) => patch(a.id, { defaultModel: v })}
                    />
                  ) : (
                    <input
                      value={a.defaultModel}
                      placeholder={a.models[0] ?? ""}
                      onChange={(e) => patch(a.id, { defaultModel: e.target.value })}
                    />
                  )}
                </label>
                <label className="field">
                  <span>备注</span>
                  <input
                    value={a.note}
                    onChange={(e) => patch(a.id, { note: e.target.value })}
                  />
                </label>
                {a.isGrok ? (
                  <div className="help">
                    Grok 原生档案：路径 / 版本门禁 / --model 标志走「常规」页的 Grok CLI
                    设置，此处仅调整启用与默认模型。
                  </div>
                ) : (
                  <label className="field">
                    <span>能力</span>
                    <span className="agent-caps">
                      <label className="disk-filter-check">
                        <input
                          type="checkbox"
                          checked={a.supportsResume}
                          onChange={(e) =>
                            patch(a.id, { supportsResume: e.target.checked })
                          }
                        />
                        支持 session/load 恢复
                      </label>
                    </span>
                  </label>
                )}
              </div>
            )}
          </div>
        );
      })}

      <div className="settings-footer-row">
        <button type="button" className="btn" onClick={addProfile}>
          添加小精灵
        </button>
        <button
          type="button"
          className="btn primary"
          disabled={saving || !dirty}
          onClick={() => void saveAll()}
        >
          {saving ? "保存中…" : dirty ? "保存全部" : "已保存"}
        </button>
      </div>
    </div>
  );
}
