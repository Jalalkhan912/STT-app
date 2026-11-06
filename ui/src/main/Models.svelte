<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    cancelDownload, downloadModel, importModel, listModels, setActiveModel, type ModelStatus,
  } from "./api";
  import { Downloads, percent } from "./downloads.svelte";
  import { formatBytes } from "./format";

  const dl = new Downloads();
  let models: ModelStatus[] = $state([]);
  let error = $state("");

  async function refresh() {
    try {
      models = await listModels();
    } catch (e) {
      error = String(e);
    }
  }
  onMount(() => {
    refresh();
    return dl.listen();
  });
  $effect(() => {
    dl.doneTick;
    refresh();
  });

  const size = (m: ModelStatus) => m.info.files.reduce((a, f) => a + f.size, 0);

  async function run(fn: () => Promise<void>) {
    error = "";
    try {
      await fn();
    } catch (e) {
      error = String(e);
    }
  }
  const download = (id: string) =>
    run(async () => {
      dl.start(id);
      try {
        await downloadModel(id);
      } catch (e) {
        delete dl.progress[id];
        throw e;
      }
    });
  const useModel = (id: string) => run(async () => { await setActiveModel(id); await refresh(); });
  const importFile = (id: string, name: string) =>
    run(async () => {
      const path = await open({ multiple: false, title: `Select ${name}` });
      if (typeof path === "string") {
        await importModel(id, name, path);
        await refresh();
      }
    });
</script>

<h1>Models</h1>
{#if error}<p class="error small">{error}</p>{/if}

{#each models as m (m.info.id)}
  {@const p = dl.progress[m.info.id]}
  <section class="card">
    <div class="row">
      <strong class="grow">{m.info.label}</strong>
      {#if m.installed}<span class="badge ok">Installed</span>{/if}
      {#if m.active}<span class="badge ok">Active</span>{/if}
    </div>
    <div class="muted small">{formatBytes(size(m))}</div>

    {#if p}
      <div class="row prog">
        <progress max="100" value={percent(p)}></progress>
        <span class="small muted nowrap">{formatBytes(p.done)} / {p.total ? formatBytes(p.total) : "…"}</span>
        <button onclick={() => cancelDownload(m.info.id)}>Cancel</button>
      </div>
    {:else}
      <div class="row btns">
        {#if !m.installed}
          <button class="primary" onclick={() => download(m.info.id)}>Download</button>
        {/if}
        {#if m.installed && !m.active}
          <button class="primary" onclick={() => useModel(m.info.id)}>Use this model</button>
        {/if}
      </div>
    {/if}
    {#if dl.errors[m.info.id]}<div class="error small">{dl.errors[m.info.id]}</div>{/if}

    {#if !m.installed && !p}
      <details>
        <summary class="small muted">Import files manually</summary>
        {#each m.info.files as f}
          <div class="row small file">
            <span class="grow mono">{f.name}</span>
            <span class="muted">{formatBytes(f.size)}</span>
            <button onclick={() => importFile(m.info.id, f.name)}>Import file…</button>
          </div>
        {/each}
      </details>
    {/if}
  </section>
{/each}

<style>
  .card { border: 1px solid var(--line); border-radius: 8px; padding: 10px 12px; margin-bottom: 10px; max-width: 640px; }
  .prog, .btns { margin-top: 8px; }
  .nowrap { white-space: nowrap; }
  .file { padding: 4px 0; }
  .mono { font-family: ui-monospace, Menlo, Consolas, monospace; overflow-wrap: anywhere; }
  details { margin-top: 8px; }
  summary { cursor: pointer; }
</style>
