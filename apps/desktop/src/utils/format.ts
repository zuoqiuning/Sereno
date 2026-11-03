/** 通用格式化工具。 */

const UNITS = ["B", "KB", "MB", "GB", "TB"];

/** 字节数格式化为人类可读字符串，如 `1.4 MB`。 */
export function formatBytes(bytes: number, precision = 1): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), UNITS.length - 1);
  const value = bytes / 1024 ** exp;
  return `${value.toFixed(exp === 0 ? 0 : precision)} ${UNITS[exp]}`;
}

/** 速度格式化为人类可读字符串，如 `2.4 MB/s`。 */
export function formatSpeed(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

/** 毫秒格式化为时长字符串，如 `1m 23s`。 */
export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) {
    return "--";
  }
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  if (minutes > 0) {
    return `${minutes}m ${seconds}s`;
  }
  return `${seconds}s`;
}

/** 百分比（0–100），用于进度条。 */
export function formatPercent(current: number, total: number): number {
  if (!Number.isFinite(total) || total <= 0) {
    return 0;
  }
  return Math.min(100, Math.max(0, (current / total) * 100));
}
