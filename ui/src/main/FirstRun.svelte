<script lang="ts">
  import { onMount } from "svelte";
  import {
    downloadModel, cancelDownload, getSettings, isMac, listModels, openPermissionSettings,
    permissionStatus, requestPermission, setActiveModel, type ModelStatus, type PermissionKind,
    type PermissionStatus,
  } from "./api";
  import { ModelStore } from "./modelstate.svelte";
  import { Downloads, percent } from "./downloads.svelte";
  import { formatBytes, hotkeyLabel } from "./format";

  let { ondone }: { ondone: () => void } = $props();

  const PARAKEET = "parakeet-tdt-0.6b-v2-int8";
  const dl = new Downloads();
  const modelSt = new ModelStore();
  const modelReady = $derived(modelSt.value.kind === "ready");

  let perms = $state(null as PermissionStatus | null);
  let model = $state(null as ModelStatus | null);
  let anyInstalled = $state(false);
  let keys: string[] = $state([]);
  let error = $state("");
  let test = $state("");
  // Steps are decided once, from the state at mount.
  let steps: ("perms" | "model" | "try")[] = $state([]);
  let idx = $state(0);
  let ready = $state(false);

  const rows: [PermissionKind, string, string][] = [
    ["microphone", "Microphone", "Needed to hear you."],
    ["accessibility", "Accessibility", "Needed to paste the text into other apps."],
    ["input_monitoring", "Input Monitoring", "Needed to detect the hotkey."],
  ];

  async function refreshModels() {
    const all = await listModels();
    model = all.find((m) => m.info.id === PARAKEET) ?? null;
    anyInstalled = all.some((m) => m.installed);
  }

  onMount(() => {
    const stop = dl.listen();
    const stopModel = modelSt.listen();
    (async () => {
      try {
        const [p, s] = await Promise.all([permissionStatus(), getSettings(), refreshModels()]);
        keys = s.hotkey.keys;
        perms = p;
        const list: typeof steps = [];
        if (isMac && !(p.microphone && p.accessibility && p.input_monitoring)) list.push("perms");
        if (!anyInstalled) list.push("model");
        list.push("try");
        steps = list;
      } catch (e) {
        error = String(e);
        steps = ["try"];
      }
      ready = true;
    })();
    // Permissions are granted in System Settings, so poll while step 1 is up.
    const poll = setInterval(async () => {
      if (steps[idx] === "perms") perms = await permissionStatus().catch(() => perms);
    }, 2000);
    return () => {
      stop();
      stopModel();
      clearInterval(poll);
    };
  });

  $effect(() => {
    dl.doneTick;
    refreshModels().catch(() => {});
  });

  const step = $derived(steps[idx]);
  const allGranted = $derived(!!perms && perms.microphone && perms.accessibility && perms.input_monitoring);
  const total = $derived(model ? model.info.files.reduce((a, f) => a + f.size, 0) : 0);
  const p = $derived(dl.progress[PARAKEET]);
  const next = () => (idx < steps.length - 1 ? idx++ : ondone());

  async function guard(fn: () => Promise<unknown>) {
    error = "";
    try {
      await fn();
    } catch (e) {
      error = String(e);
    }
  }
  const allow = (k: PermissionKind) => guard(() => requestPermission(k));
  const openSettings = (k: PermissionKind) => guard(() => openPermissionSettings(k));
  const download = () =>
    guard(async () => {
      dl.start(PARAKEET);
      try {
        await downloadModel(PARAKEET);
      } catch (e) {
        delete dl.progress[PARAKEET];
        throw e;
      }
    });
  const finishModel = () =>
    guard(async () => {
      if (model && !model.active) await setActiveModel(PARAKEET);
      next();
    });
</script>

<div class="wrap">
  <div class="panel">
    <h1>Welcome to Dictation</h1>
    {#if ready}
      <div class="muted small steps">Step {idx + 1} of {steps.length}</div>
      {#if error}<p class="error small">{error}</p>{/if}

      {#if step === "perms" && perms}
        <p>Dictation needs a few permissions. Grant each one, then come back here.</p>
        {#each rows as [kind, label, why]}
          {@const ok = perms[kind]}
          <div class="perm row">
            <div class="grow">
              <strong>{label}</strong>
              <div class="muted small">{why}</div>
            </div>
            <span class="badge" class:ok>{ok ? "Granted" : "Not granted"}</span>
            {#if !ok}
              <button onclick={() => allow(kind)}>Allow…</button>
              <button onclick={() => openSettings(kind)}>Open Settings</button>
            {/if}
          </div>
        {/each}
        <p class="muted small">
          macOS may ask you to restart the app after granting Input Monitoring or Accessibility.
        </p>
        <div class="nav"><button class="primary" onclick={next}>{allGranted ? "Continue" : "Continue anyway"}</button></div>
      {:else if step === "model"}
        <p>
          Dictation runs entirely on your computer. Download the speech model once
          ({total ? formatBytes(total) : "~660 MB"}).
        </p>
        {#if p}
          <div class="row">
            <progress max="100" value={percent(p)}></progress>
            <span class="small muted nowrap">{formatBytes(p.done)} / {p.total ? formatBytes(p.total) : "…"}</span>
            <button onclick={() => cancelDownload(PARAKEET)}>Cancel</button>
          </div>
        {:else if !anyInstalled}
          <button class="primary" onclick={download}>Download Parakeet</button>
        {:else}
          <p class="ok-text">Model installed.</p>
        {/if}
        {#if dl.errors[PARAKEET]}<p class="error small">{dl.errors[PARAKEET]}</p>{/if}
        <div class="nav"><button class="primary" disabled={!anyInstalled} onclick={finishModel}>Continue</button></div>
      {:else if step === "try"}
        <p>Hold <strong>{hotkeyLabel(keys, isMac)}</strong> and speak. Release to paste. Try it here:</p>
        {#if modelSt.value.kind === "error"}
          <p class="error small">Model failed to load: {modelSt.value.message}</p>
        {:else if !modelReady}
          <p class="muted small">Loading model…</p>
        {/if}
        <textarea rows="5" bind:value={test} disabled={!modelReady} placeholder="Your words will appear here"></textarea>
        <div class="nav"><button class="primary" onclick={ondone}>Done</button></div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .wrap { height: 100%; display: flex; align-items: center; justify-content: center; padding: 24px; overflow-y: auto; }
  .panel { width: 100%; max-width: 560px; }
  .steps { margin: -6px 0 12px; }
  .perm { padding: 10px 0; border-bottom: 1px solid var(--line); }
  .nav { margin-top: 20px; display: flex; justify-content: flex-end; }
  textarea { width: 100%; resize: vertical; }
  .nowrap { white-space: nowrap; }
  .ok-text { color: var(--ok); }
</style>
