<script lang="ts">
  import { onMount } from "svelte";
  import { getSettings, isMac, listModels, permissionStatus } from "./api";
  import FirstRun from "./FirstRun.svelte";
  import History from "./History.svelte";
  import Settings from "./Settings.svelte";
  import Models from "./Models.svelte";

  type Tab = "history" | "settings" | "models";
  let tab: Tab = $state("history");
  // null while the initial check runs.
  let needsSetup: boolean | null = $state(null);
  let activeMissing = $state(false);

  // Another model is installed but the active one is not: dictation would fail.
  async function checkActive() {
    try {
      const [models, s] = await Promise.all([listModels(), getSettings()]);
      const active = models.find((m) => m.info.id === s.active_model);
      activeMissing = !active?.installed && models.some((m) => m.installed);
    } catch {
      activeMissing = false;
    }
  }
  $effect(() => {
    tab; // re-check whenever the user switches screens
    if (needsSetup === false) checkActive();
  });

  onMount(async () => {
    try {
      const [models, perms] = await Promise.all([listModels(), permissionStatus()]);
      const noModel = !models.some((m) => m.installed);
      const permMissing =
        isMac && !(perms.microphone && perms.accessibility && perms.input_monitoring);
      needsSetup = noModel || permMissing;
    } catch (e) {
      console.warn("startup check failed", e);
      needsSetup = false;
    }
  });

  const tabs: [Tab, string][] = [
    ["history", "History"],
    ["settings", "Settings"],
    ["models", "Models"],
  ];
</script>

{#if needsSetup === null}
  <!-- checking -->
{:else if needsSetup}
  <FirstRun ondone={() => (needsSetup = false)} />
{:else}
  <div class="shell">
    <nav>
      {#each tabs as [id, label]}
        <button class="navitem" class:active={tab === id} onclick={() => (tab = id)}>{label}</button>
      {/each}
    </nav>
    <main>
      {#if activeMissing && tab !== "models"}
        <div class="banner">
          Active model not installed —
          <button class="link" onclick={() => (tab = "models")}>choose one in Models</button>
        </div>
      {/if}
      {#if tab === "history"}<History />{:else if tab === "settings"}<Settings />{:else}<Models />{/if}
    </main>
  </div>
{/if}

<style>
  .shell { display: flex; height: 100%; }
  nav {
    width: 160px; flex: none; background: var(--bg-side);
    border-right: 1px solid var(--line); padding: 12px 8px;
    display: flex; flex-direction: column; gap: 2px;
  }
  .navitem {
    text-align: left; border: none; background: none; padding: 6px 10px; border-radius: 6px;
  }
  .navitem:hover { background: var(--hover) !important; }
  .navitem.active { background: var(--accent) !important; color: var(--accent-fg); }
  .banner {
    border: 1px solid var(--line); border-radius: 6px; padding: 6px 10px; margin-bottom: 12px;
    color: var(--danger);
  }
  main { flex: 1; min-width: 0; overflow-y: auto; padding: 16px 20px; }
</style>
