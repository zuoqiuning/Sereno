import { describe, expect, it } from "vitest";

import { gameVersionGroup } from "./gameVersion";

describe("gameVersionGroup", () => {
  it("1.21 的所有补丁版本归到一起", () => {
    expect(gameVersionGroup("1.21")).toBe("1.21");
    expect(gameVersionGroup("1.21.1")).toBe("1.21");
    expect(gameVersionGroup("1.21.2")).toBe("1.21");
  });

  it("26.x 保持独立分组", () => {
    expect(gameVersionGroup("26.1")).toBe("26.1");
    expect(gameVersionGroup("26.2")).toBe("26.2");
    expect(gameVersionGroup("26.3")).toBe("26.3");
  });

  it("旧版本同样只保留主次版本", () => {
    expect(gameVersionGroup("1.20.4")).toBe("1.20");
    expect(gameVersionGroup("1.7.10")).toBe("1.7");
  });

  it("快照与未知版本原样返回", () => {
    expect(gameVersionGroup("24w14a")).toBe("24w14a");
    expect(gameVersionGroup("未知版本")).toBe("未知版本");
  });
});
