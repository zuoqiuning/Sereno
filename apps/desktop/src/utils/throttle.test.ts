import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { debounce, throttle } from "./throttle";

describe("throttle", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("连续调用只在窗口开始时执行一次", () => {
    let count = 0;
    const fn = throttle(() => {
      count += 1;
    }, 100);

    fn();
    fn();
    fn();

    expect(count).toBe(1);
  });

  it("窗口结束后可以再次触发", () => {
    let count = 0;
    const fn = throttle(() => {
      count += 1;
    }, 100);

    fn();
    expect(count).toBe(1);

    vi.advanceTimersByTime(150);
    fn();
    expect(count).toBe(2);
  });

  it("尾部调用会被补执行一次", () => {
    let count = 0;
    const fn = throttle(() => {
      count += 1;
    }, 100);

    fn();
    vi.advanceTimersByTime(50);
    fn();
    expect(count).toBe(1);

    // 窗口末尾的这次调用应被补上
    vi.advanceTimersByTime(100);
    expect(count).toBe(2);
  });

  it("cancel 之后不再补执行", () => {
    let count = 0;
    const fn = throttle(() => {
      count += 1;
    }, 100);

    fn();
    vi.advanceTimersByTime(50);
    fn();
    fn.cancel();
    vi.advanceTimersByTime(200);

    expect(count).toBe(1);
  });
});

describe("debounce", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("静默期结束后只执行一次", () => {
    let count = 0;
    const fn = debounce(() => {
      count += 1;
    }, 100);

    fn();
    fn();
    fn();
    expect(count).toBe(0);

    vi.advanceTimersByTime(120);
    expect(count).toBe(1);
  });

  it("每次调用都会重置静默期", () => {
    let count = 0;
    const fn = debounce(() => {
      count += 1;
    }, 100);

    fn();
    vi.advanceTimersByTime(80);
    fn();
    vi.advanceTimersByTime(80);
    expect(count).toBe(0);

    vi.advanceTimersByTime(30);
    expect(count).toBe(1);
  });

  it("cancel 之后不再执行", () => {
    let count = 0;
    const fn = debounce(() => {
      count += 1;
    }, 100);

    fn();
    fn.cancel();
    vi.advanceTimersByTime(200);

    expect(count).toBe(0);
  });
});
