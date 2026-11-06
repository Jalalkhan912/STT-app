<script lang="ts">
  import { hotkeyLabel, isSupportedKey } from "./format";
  import { isMac } from "./api";

  let { keys, onchange }: { keys: string[]; onchange: (keys: string[]) => void } = $props();

  let recording = $state(false);
  let note = $state("");
  let collected: string[] = [];
  let down = new Set<string>();
  let el: HTMLButtonElement | undefined = $state();

  function begin() {
    recording = true;
    note = "";
    collected = [];
    down = new Set();
    el?.focus();
  }
  function cancel(msg = "") {
    clearTimeout(stuckTimer);
    recording = false;
    note = msg;
  }
  function onkeydown(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    if (e.repeat) return;
    if (!isSupportedKey(e.code, isMac)) {
      note =
        isMac && e.code === "CapsLock"
          ? "Caps Lock can't be used as a hotkey on macOS."
          : "That key can't be used as a hotkey.";
      return;
    }
    note = "";
    clearTimeout(stuckTimer);
    down.add(e.code);
    if (!collected.includes(e.code)) collected.push(e.code);
  }
  let stuckTimer: ReturnType<typeof setTimeout> | undefined;
  function commit() {
    clearTimeout(stuckTimer);
    if (!recording || collected.length === 0) return;
    // A lone Escape is a cancel, not a hotkey.
    if (collected.length === 1 && collected[0] === "Escape") return cancel("Escape can't be used alone.");
    onchange([...collected]);
    cancel();
  }
  function onkeyup(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    down.delete(e.code);
    if (down.size === 0) return commit();
    // macOS does not deliver keyups for other keys while Meta is held, so
    // commit shortly after the last keyup rather than waiting forever.
    clearTimeout(stuckTimer);
    stuckTimer = setTimeout(commit, 400);
  }
</script>

<div class="row">
  <span class="chip">{recording ? "Press keys… (Esc to cancel)" : hotkeyLabel(keys, isMac)}</span>
  <button bind:this={el} onclick={() => (recording ? cancel() : begin())} {onkeydown} {onkeyup} onblur={() => recording && cancel()}>
    {recording ? "Cancel" : "Record"}
  </button>
  {#if note}<span class="small muted">{note}</span>{/if}
</div>

<style>
  .chip { border: 1px solid var(--line); border-radius: 6px; padding: 3px 10px; min-width: 90px; white-space: nowrap; text-align: center; background: var(--field); }
</style>
