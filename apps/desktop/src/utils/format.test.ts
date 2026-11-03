import { describe, expect, it } from "vitest";

import { formatBytes, formatDuration, formatPercent, formatSpeed } from "./format";

describe("formatBytes", () => {
  it("零与负数返回 0 B", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(-1)).toBe("0 B");
    expect(formatBytes(Number.NaN)).toBe("0 B");
  });

  it("按 1024 进制换算", () => {
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1024 * 1024)).toBe("1.0 MB");
    expect(formatBytes(1024 * 1024 * 1024)).toBe("1.0 GB");
    expect(formatBytes(1024 ** 4)).toBe("1.0 TB");
  });

  it("小于 1 KB 时用字节", () => {
    expect(formatBytes(512)).toBe("512 B");
  });

  it("不保留小数位的是字节", () => {
    expect(formatBytes(256)).toBe("256 B");
  });
});

describe("formatSpeed", () => {
  it("在字节数后加 /s", () => {
    expect(formatSpeed(1024)).toBe("1.0 KB/s");
    expect(formatSpeed(0)).toBe("0 B/s");
  });
});

describe("formatDuration", () => {
  it("按秒 / 分 / 时分段", () => {
    expect(formatDuration(5_000)).toBe("5s");
    expect(formatDuration(65_000)).toBe("1m 5s");
    expect(formatDuration(3_600_000)).toBe("1h 0m");
  });

  it("非法输入返回占位符", () => {
    expect(formatDuration(-1)).toBe("--");
    expect(formatDuration(Number.NaN)).toBe("--");
  });
});

describe("formatPercent", () => {
  it("计算百分比并夹在 0–100", () => {
    expect(formatPercent(50, 100)).toBe(50);
    expect(formatPercent(150, 100)).toBe(100);
    expect(formatPercent(-5, 100)).toBe(0);
  });

  it("总量为 0 时返回 0 而不是 NaN", () => {
    expect(formatPercent(10, 0)).toBe(0);
  });
});
