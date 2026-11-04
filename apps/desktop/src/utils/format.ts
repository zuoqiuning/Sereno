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

/**
 * 相对时间：「3 天前」「2 小时前」。
 *
 * 实例卡片上的时间用相对值比绝对日期更有信息量——用户关心的是
 * 「多久没玩了」，而不是「2026 年 10 月 28 日」。精确日期放 `title` 里。
 */
export function formatRelativeTime(value: string | number | null | undefined): string {
  if (value === null || value === undefined || value === "") return "从未";

  // 后端的时间既可能是秒级时间戳字符串，也可能是 ISO 字符串
  const seconds = typeof value === "string" ? Number.parseInt(value, 10) : NaN;
  const date = Number.isNaN(seconds) ? new Date(value) : new Date(seconds * 1000);
  if (Number.isNaN(date.getTime())) return "未知";

  const diff = Date.now() - date.getTime();
  if (diff < 0) return formatAbsoluteDate(date);

  const minutes = Math.floor(diff / 60_000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;

  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;

  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} 天前`;

  return formatAbsoluteDate(date);
}

/** 绝对日期：「2026-10-28」。 */
export function formatAbsoluteDate(date: Date): string {
  const year = date.getFullYear();
  const month = `${date.getMonth() + 1}`.padStart(2, "0");
  const day = `${date.getDate()}`.padStart(2, "0");
  return `${year}-${month}-${day}`;
}

/**
 * 下载剩余时间：「约 3 分 12 秒」。
 *
 * 只说速度是不够的——用户真正想知道的是「还要等多久」，
 * 尤其是整合包里几百个文件的时候。
 */
export function formatRemaining(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "—";
  const totalSeconds = Math.round(ms / 1000);
  if (totalSeconds < 60) return `${totalSeconds} 秒`;

  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  if (minutes < 60) return `${minutes} 分 ${seconds} 秒`;

  const hours = Math.floor(minutes / 60);
  return `${hours} 小时 ${minutes % 60} 分`;
}
