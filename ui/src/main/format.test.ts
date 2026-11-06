import { describe, it, expect } from "vitest";
import { groupByDay, formatBytes, formatDuration, hotkeyLabel, isSupportedKey } from "./format";
import { modelBanner } from "./modelstate.svelte";
import type { Dictation } from "./api";

const DAY = 86_400_000;
function d(id: string, ms: number): Dictation {
  return {
    id,
    created_at_ms: ms,
    duration_ms: 1000,
    text: "t",
    raw_text: "t",
    model: "m",
    target_app: null,
    status: "pasted",
    audio_path: null,
  };
}

describe("formatBytes", () => {
  it("formats", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(1.9 * 1024 * 1024)).toBe("1.9 MB");
    expect(formatBytes(2 * 1024 ** 3)).toBe("2.0 GB");
  });
});

describe("formatDuration", () => {
  it("formats", () => {
    expect(formatDuration(8000)).toBe("0:08");
    expect(formatDuration(65000)).toBe("1:05");
    expect(formatDuration(720000)).toBe("12:00");
  });
});

describe("hotkeyLabel", () => {
  it("formats", () => {
    expect(hotkeyLabel(["AltRight"], true)).toBe("Right ⌥");
    expect(hotkeyLabel(["ControlRight"], false)).toBe("Right Ctrl");
    expect(hotkeyLabel(["ControlLeft", "Space"], false)).toBe("Ctrl + Space");
  });
});

describe("groupByDay", () => {
  it("groups Today, Yesterday, dated; preserves order", () => {
    const now = new Date(2026, 10, 6, 15, 0, 0);
    const t = now.getTime();
    const g = groupByDay(
      [d("a", t), d("b", t - 3600_000), d("c", t - DAY), d("e", t - 3 * DAY)],
      now,
    );
    expect(g.map((x) => x.label)).toEqual([
      "Today",
      "Yesterday",
      new Date(t - 3 * DAY).toLocaleDateString(undefined, {
        weekday: "short",
        month: "short",
        day: "numeric",
      }),
    ]);
    expect(g[0].items.map((i) => i.id)).toEqual(["a", "b"]);
  });
});

describe("isSupportedKey", () => {
  it("rejects CapsLock on macOS only", () => {
    expect(isSupportedKey("CapsLock", true)).toBe(false);
    expect(isSupportedKey("CapsLock", false)).toBe(true);
    expect(isSupportedKey("AltRight", true)).toBe(true);
  });
});

describe("modelBanner", () => {
  it("only shows for loading and error", () => {
    expect(modelBanner({ kind: "loading" })).toBe("Loading model…");
    expect(modelBanner({ kind: "error", message: "boom" })).toBe("Model failed to load: boom");
    expect(modelBanner({ kind: "ready" })).toBeNull();
    expect(modelBanner({ kind: "none" })).toBeNull();
  });
});
