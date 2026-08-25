import { useCallback, useState } from "react";
import { api } from "../lib/api";
import type { PermissionReq } from "../lib/types";

/** Permission modal state + respond helper (ACP session permission). */
export function usePermission(
  onAfterRespond?: (sessionId: string) => void,
  onRespondError?: (sessionId: string, message: string) => void
) {
  const [permission, setPermission] = useState<PermissionReq | null>(null);

  const respondPermission = useCallback(
    async (allow: boolean, optionId?: string) => {
      if (!permission) return;
      const { sessionId, id } = permission;
      try {
        await api.respondPermission(sessionId, id, allow, optionId);
        setPermission(null);
        onAfterRespond?.(sessionId);
      } catch (e) {
        // 保留弹窗：后端请求仍在等待应答，直接关掉会让 agent 永远挂着
        onRespondError?.(sessionId, e instanceof Error ? e.message : String(e));
      }
    },
    [permission, onAfterRespond, onRespondError]
  );

  return {
    permission,
    setPermission,
    respondPermission,
  };
}
