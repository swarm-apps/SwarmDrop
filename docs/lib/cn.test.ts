import { describe, expect, it } from "vitest";
import { cn } from "./cn";

describe("cn", () => {
  it("joins plain class strings", () => {
    expect(cn("a", "b")).toBe("a b");
  });

  // 这条是接 shadcn/ui 的前提：它的组件大量用对象形参传条件类名。
  // 旧实现（只有 twMerge）会把对象 String() 成 "[object Object]" 混进 class，不报错、静默失效。
  it("resolves object and array forms", () => {
    expect(cn("base", { active: true, hidden: false })).toBe("base active");
    expect(cn(["a", ["b", { c: true }]])).toBe("a b c");
  });

  it("drops falsy arguments", () => {
    expect(cn("a", undefined, null, false, "", "b")).toBe("a b");
  });

  it("never emits [object Object]", () => {
    expect(cn({ active: true })).not.toContain("[object Object]");
  });

  it("still resolves Tailwind conflicts, last one winning", () => {
    expect(cn("px-2", "px-4")).toBe("px-4");
    expect(cn("text-sm", { "text-lg": true })).toBe("text-lg");
  });
});
