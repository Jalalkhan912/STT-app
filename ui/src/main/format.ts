import type { Dictation } from "./api";

function startOfDay(t: number): number {
  const d = new Date(t);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

export function groupByDay(
  items: Dictation[],
  now: Date,
): { label: string; items: Dictation[] }[] {
  const today = startOfDay(now.getTime());
  const yesterday = startOfDay(today - 1);
  const groups: { label: string; items: Dictation[] }[] = [];
  let lastKey = NaN;
  for (const it of items) {
    const day = startOfDay(it.created_at_ms);
    if (day !== lastKey || groups.length === 0) {
      const label =
        day === today
          ? "Today"
          : day === yesterday
            ? "Yesterday"
            : new Date(it.created_at_ms).toLocaleDateString(undefined, {
                weekday: "short",
                month: "short",
                day: "numeric",
              });
      groups.push({ label, items: [] });
      lastKey = day;
    }
    groups[groups.length - 1].items.push(it);
  }
  return groups;
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(1)} ${units[i]}`;
}

export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

/** Codes the Rust hotkey listener can report (see platform/keymap.rs). */
export function isSupportedKey(code: string, mac = false): boolean {
  // macOS only reports CapsLock as a toggle (no key-up while it stays on), so a
  // hold-to-talk hotkey on it would get stuck.
  if (mac && code === "CapsLock") return false;
  return /^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-2])|(Alt|Control|Shift|Meta)(Left|Right)|Space|Escape|Tab|CapsLock|Arrow(Left|Right|Up|Down))$/.test(
    code,
  );
}

function keyName(code: string, mac: boolean): string {
  const mod = /^(Alt|Control|Shift|Meta)(Left|Right)$/.exec(code);
  if (mod) {
    const side = mod[2] === "Left" ? "Left" : "Right";
    const names: Record<string, [string, string]> = {
      Alt: ["⌥", "Alt"],
      Control: ["⌃", "Ctrl"],
      Shift: ["⇧", "Shift"],
      Meta: ["⌘", "Win"],
    };
    const [m, w] = names[mod[1]];
    // A lone-sided modifier reads "Right ⌥"; inside a combo the side is kept too.
    return `${side} ${mac ? m : w}`;
  }
  const k = /^Key([A-Z])$/.exec(code) ?? /^Digit([0-9])$/.exec(code);
  if (k) return k[1];
  const arrow = /^Arrow(.+)$/.exec(code);
  if (arrow) return arrow[1];
  return code;
}

export function hotkeyLabel(keys: string[], mac: boolean): string {
  // Combos drop the side for the common left-hand modifiers: "Ctrl + Space".
  if (keys.length > 1) {
    return keys
      .map((c) => {
        const n = keyName(c, mac);
        return c.endsWith("Left") ? n.replace(/^Left /, "") : n;
      })
      .join(" + ");
  }
  return keys.map((c) => keyName(c, mac)).join("");
}
