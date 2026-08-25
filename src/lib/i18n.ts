/** 界面状态与通用文案 */

export function statusLabel(status: string | undefined | null): string {
  switch (status) {
    case "idle":
      return "空闲";
    case "running":
      return "运行中";
    case "waiting_permission":
      return "等待授权";
    case "error":
      return "错误";
    case "hibernated":
      return "已休眠";
    case "interrupted":
      return "上轮中断";
    case "starting":
      return "启动中";
    case "pending":
      return "等待中";
    case "completed":
    case "success":
      return "已完成";
    case "failed":
      return "失败";
    case "in_progress":
      return "进行中";
    default:
      return status || "空闲";
  }
}

export function permissionModeLabel(mode: string): string {
  switch (mode) {
    case "ask":
      return "每次询问";
    case "auto":
      return "自动";
    case "always-approve":
      return "始终允许";
    default:
      return mode;
  }
}

export function sandboxModeLabel(mode: string): string {
  switch (mode) {
    case "off":
      return "关闭";
    case "workspace":
      return "工作区";
    case "read-only":
      return "只读";
    case "strict":
      return "严格";
    default:
      return mode;
  }
}
