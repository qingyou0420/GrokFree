import { useCallback, useEffect, useState } from "react";
import { api, errorText } from "../lib/api";
import type { AgentProfile } from "../lib/types";
import { useUiStore } from "../state";

const SELECTED_AGENT_KEY = "grok-selected-agent";

function loadSelectedAgent(): string {
  try {
    return localStorage.getItem(SELECTED_AGENT_KEY) || "grok";
  } catch {
    return "grok";
  }
}

/**
 * 智能体档案注册表（agents.json）：列表加载 + 当前选中档案（新建会话用）。
 * 选中值持久化在 localStorage；档案本身由 Settings 智能体页编辑保存。
 */
export function useAgents(opts: { flash: (text: string, kind?: "info" | "success" | "error") => void }) {
  const { flash } = opts;
  const [agents, setAgents] = useState<AgentProfile[]>([]);
  const selectedAgentId = useUiStore((s) => s.selectedAgentId);
  const setSelectedAgentId = useUiStore((s) => s.setSelectedAgentId);

  const refresh = useCallback(async () => {
    try {
      const list = await api.listAgents();
      setAgents(list);
      // 选中的档案被删/被禁用时回退 grok
      const cur = useUiStore.getState().selectedAgentId;
      const ok = list.some((a) => a.id === cur && a.enabled);
      if (!ok) {
        const fallback =
          list.find((a) => a.id === "grok")?.id ??
          list.find((a) => a.enabled)?.id ??
          "grok";
        useUiStore.getState().setSelectedAgentId(fallback);
      }
      return list;
    } catch (e) {
      flash(`读取智能体档案失败：${errorText(e)}`, "error");
      return [];
    }
  }, [flash]);

  const save = useCallback(
    async (profiles: AgentProfile[]) => {
      try {
        const list = await api.saveAgents(profiles);
        setAgents(list);
        const cur = useUiStore.getState().selectedAgentId;
        if (!list.some((a) => a.id === cur && a.enabled)) {
          useUiStore
            .getState()
            .setSelectedAgentId(
              list.find((a) => a.id === "grok")?.id ??
                list.find((a) => a.enabled)?.id ??
                "grok"
            );
        }
        return list;
      } catch (e) {
        flash(`保存智能体失败：${errorText(e)}`, "error");
        return null;
      }
    },
    [flash]
  );

  // 初始 selectedAgentId（uiStore 默认 null → 从 localStorage 恢复）
  useEffect(() => {
    if (useUiStore.getState().selectedAgentId === null) {
      useUiStore.getState().setSelectedAgentId(loadSelectedAgent());
    }
  }, []);

  // 选中变化持久化
  useEffect(() => {
    if (selectedAgentId) {
      try {
        localStorage.setItem(SELECTED_AGENT_KEY, selectedAgentId);
      } catch {
        /* ignore */
      }
    }
  }, [selectedAgentId]);

  const enabledAgents = agents.filter((a) => a.enabled);
  const selectedAgent =
    agents.find((a) => a.id === selectedAgentId && a.enabled) ??
    enabledAgents[0] ??
    agents.find((a) => a.id === "grok") ??
    null;

  const agentName = useCallback(
    (id?: string | null) =>
      agents.find((a) => a.id === (id || "grok"))?.name ?? null,
    [agents]
  );

  return {
    agents,
    enabledAgents,
    selectedAgent,
    selectedAgentId: selectedAgent?.id ?? "grok",
    setSelectedAgentId,
    refresh,
    save,
    agentName,
  };
}
