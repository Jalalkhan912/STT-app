<script lang="ts">
  import { onMount } from "svelte";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import {
    getSettings, isMac, listInputDevices, onSettingsChanged, setSettings, storageUsage,
    type OverlayPosition, type Retention, type Settings,
  } from "./api";
  import { formatBytes, hotkeyLabel } from "./format";
  import HotkeyInput from "./HotkeyInput.svelte";
  import StringList from "./StringList.svelte";

  let s = $state(null as Settings | null);
  let devices: string[] = $state([]);
  let usage: number | null = $state(null);
  let error = $state("");
  let lastSaved = "";
  let timer: ReturnType<typeof setTimeout> | undefined;
  // The edit waiting for the debounce timer; flushed on destroy.
  let pending: { snapshot: Settings; json: string } | null = null;

  async function flush() {
    clearTimeout(timer);
    timer = undefined;
    const p = pending;
    pending = null;
    if (!p) return;
    try {
      await setSettings(p.snapshot);
      lastSaved = p.json;
      error = "";
    } catch (e) {
      error = String(e);
      // Leave lastSaved stale; the next edit retries the save.
    }
  }

  const nl = (v: string) => v.replaceAll("\n", "\\n");
  const unnl = (v: string) => v.replaceAll("\\n", "\n");

  function adopt(incoming: Settings) {
    lastSaved = JSON.stringify(incoming);
    s = incoming;
  }

  onMount(() => {
    getSettings().then(adopt).catch((e) => (error = String(e)));
    listInputDevices().then((d) => (devices = d)).catch(() => {});
    storageUsage().then((n) => (usage = n)).catch(() => {});
    const un = onSettingsChanged((incoming) => {
      // Ignore echoes of our own save and anything arriving mid-edit.
      if (!timer && JSON.stringify(incoming) !== lastSaved) adopt(incoming);
    });
    return () => {
      un.then((f) => f());
      // Save what was typed in the last 300 ms instead of dropping it.
      void flush();
    };
  });

  // Debounced save on any change.
  $effect(() => {
    if (!s) return;
    const json = JSON.stringify(s);
    clearTimeout(timer);
    if (json === lastSaved) {
      // Edited back to the saved value within the debounce window.
      pending = null;
      timer = undefined;
      return;
    }
    pending = { snapshot: $state.snapshot(s) as Settings, json };
    timer = setTimeout(flush, 300);
  });

  const retMode = $derived(
    !s ? "Forever" : typeof s.audio_retention === "string" ? s.audio_retention : "Days",
  );
  const retDays = $derived(s && typeof s.audio_retention === "object" ? s.audio_retention.Days : 30);
  /** True when switching to `next` deletes recordings that `cur` keeps. */
  function deletesMore(cur: Retention, next: Retention): boolean {
    if (cur === "Never") return false;
    if (next === "Never") return true;
    if (typeof next === "string") return false; // Forever
    return cur === "Forever" || (typeof cur === "object" && next.Days < cur.Days);
  }
  /** Applies a retention change, asking first if it deletes audio right away. */
  async function changeRetention(next: Retention): Promise<boolean> {
    if (!s) return false;
    if (deletesMore(s.audio_retention, next)) {
      let ok = false;
      try {
        ok = await confirm("This deletes older recordings now. Continue?", {
          title: "Delete recordings?",
          kind: "warning",
        });
      } catch {
        ok = false;
      }
      if (!ok) return false;
    }
    if (s) s.audio_retention = next;
    return true;
  }
  async function setRetMode(el: HTMLSelectElement) {
    const m = el.value;
    if (!(await changeRetention((m === "Days" ? { Days: retDays } : m) as Retention))) {
      el.value = retMode; // declined: show the unchanged choice again
    }
  }
  async function setRetDays(el: HTMLInputElement) {
    const n = el.valueAsNumber;
    if (!(n >= 1) || !(await changeRetention({ Days: Math.floor(n) }))) el.value = String(retDays);
  }

  const positions: [OverlayPosition, string][] = [
    ["BottomCenter", "Bottom center"],
    ["TopCenter", "Top center"],
    ["BottomLeft", "Bottom left"],
    ["BottomRight", "Bottom right"],
  ];
  const idleOptions: [number | null, string][] = [
    [null, "Never"], [5, "5 minutes"], [15, "15 minutes"], [30, "30 minutes"], [60, "1 hour"],
  ];
</script>

<h1>Settings</h1>
{#if error}<p class="error small">{error}</p>{/if}

{#if s}
  <div class="form">
    <h2>Dictation</h2>
    <div class="field">
      <span class="lbl">Hold to dictate</span>
      <div>
        <HotkeyInput keys={s.hotkey.keys} onchange={(k) => s && (s.hotkey.keys = k)} />
        <div class="small muted">Currently {hotkeyLabel(s.hotkey.keys, isMac)}. Hold it, speak, release.</div>
      </div>
    </div>
    <div class="field">
      <label for="mic">Microphone</label>
      <div>
        <select id="mic" bind:value={s.input_device}>
          <option value={null}>System default</option>
          {#each devices as d}<option value={d}>{d}</option>{/each}
        </select>
        <span class="small muted">Takes effect after restart</span>
      </div>
    </div>
    <div class="field">
      <label for="always">Keep mic open</label>
      <div>
        <input id="always" type="checkbox" bind:checked={s.mic_always_on} />
        <span class="small muted">Instant start; the system mic indicator stays on.</span>
        <div class="small muted">Takes effect after restart</div>
      </div>
    </div>

    <h2>Cleanup</h2>
    <div class="field">
      <label for="cap">Capitalize first letter</label>
      <input id="cap" type="checkbox" bind:checked={s.cleanup.capitalize_first} />
    </div>
    <div class="field">
      <label for="ts">Add trailing space</label>
      <input id="ts" type="checkbox" bind:checked={s.cleanup.trailing_space} />
    </div>
    <div class="field">
      <label for="rf">Remove filler words</label>
      <input id="rf" type="checkbox" bind:checked={s.cleanup.remove_fillers} />
    </div>
    {#if s.cleanup.remove_fillers}
      <div class="field">
        <span class="lbl">Fillers</span>
        <StringList bind:items={s.cleanup.fillers} placeholder="um" />
      </div>
    {/if}
    <div class="field">
      <span class="lbl">Replacements</span>
      <div class="pairs">
        {#each s.cleanup.replacements as _, i}
          <div class="row">
            <input type="text" class="grow" placeholder="Say" value={nl(s.cleanup.replacements[i][0])}
              oninput={(e) => s && (s.cleanup.replacements[i][0] = unnl(e.currentTarget.value))} />
            <span class="muted">→</span>
            <input type="text" class="grow" placeholder="Write (\n = new line)" value={nl(s.cleanup.replacements[i][1])}
              oninput={(e) => s && (s.cleanup.replacements[i][1] = unnl(e.currentTarget.value))} />
            <button aria-label="Remove" onclick={() => s?.cleanup.replacements.splice(i, 1)}>−</button>
          </div>
        {/each}
        <div><button onclick={() => s?.cleanup.replacements.push(["", ""])}>Add</button></div>
      </div>
    </div>
    <div class="field">
      <span class="lbl">Vocabulary</span>
      <StringList bind:items={s.cleanup.vocabulary} placeholder="Name or term to spell correctly" />
    </div>

    <h2>General</h2>
    <div class="field">
      <label for="pos">Overlay position</label>
      <select id="pos" bind:value={s.overlay_position}>
        {#each positions as [v, l]}<option value={v}>{l}</option>{/each}
      </select>
    </div>
    <div class="field">
      <label for="login">Launch at login</label>
      <input id="login" type="checkbox" bind:checked={s.launch_at_login} />
    </div>
    <div class="field">
      <label for="idle">Unload model after idle</label>
      <select id="idle" bind:value={s.unload_after_idle_min}>
        {#each idleOptions as [v, l]}<option value={v}>{l}</option>{/each}
      </select>
    </div>

    <h2>Recordings</h2>
    <div class="field">
      <label for="ret">Keep audio</label>
      <div class="row">
        <select id="ret" value={retMode} onchange={(e) => setRetMode(e.currentTarget)}>
          <option value="Forever">Forever</option>
          <option value="Days">For a number of days</option>
          <option value="Never">Never</option>
        </select>
        {#if retMode === "Days"}
          <input type="number" min="1" style="width:70px" value={retDays}
            onchange={(e) => setRetDays(e.currentTarget)} />
          <span class="small muted">days</span>
        {/if}
      </div>
    </div>
    <div class="field">
      <span class="lbl">Storage used</span>
      <span>{usage === null ? "…" : formatBytes(usage)}</span>
    </div>
  </div>
{/if}

<style>
  .form { max-width: 640px; }
  .field { display: grid; grid-template-columns: 190px 1fr; gap: 12px; padding: 5px 0; align-items: start; }
  .field > label, .lbl { padding-top: 3px; }
  .pairs { display: flex; flex-direction: column; gap: 4px; }
</style>
