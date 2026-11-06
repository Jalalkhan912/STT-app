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
    recording = false;
    note = msg;
  }
  function onkeydown(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    if (e.repeat) return;
    if (!isSupportedKey(e.code)) {
      note = "That key can't be used as a hotkey.";
      return;
    }
    note = "";
    down.add(e.code);
    if (!collected.includes(e.code)) collected.push(e.code);
  }
  function onkeyup(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    down.delete(e.code);
    if (down.size > 0 || collected.length === 0) return;
    // All keys released: commit, unless it was a lone Escape.
    if (collected.length === 1 && collected[0] === "Escape") return cancel("Escape can't be used alone.");
    onchange([...collected]);
    cancel();
  }
</script>

<div class="row">
  <span class="chip">{recording ? "Press keys…" : hotkeyLabel(keys, isMac)}</span>
  <button bind:this={el} onclick={() => (recording ? cancel() : begin())} {onkeydown} {onkeyup} onblur={() => recording && cancel()}>
    {recording ? "Cancel" : "Record"}
  </button>
  {#if note}<span class="small muted">{note}</span>{/if}
</div>

<style>
  .chip { border: 1px solid var(--line); border-radius: 6px; padding: 3px 10px; min-width: 90px; text-align: center; background: var(--field); }
</style>
