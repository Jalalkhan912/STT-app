<script lang="ts">
  import { onMount } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import {
    audioFile, deleteDictation, exportHistory, listHistory, onHistoryChanged, retranscribe,
    searchHistory,
    type Dictation,
  } from "./api";
  import { formatDuration, groupByDay } from "./format";

  const PAGE = 50;
  let items: Dictation[] = $state([]);
  let query = $state("");
  let hasMore = $state(false);
  let loading = $state(false);
  let error = $state("");
  let confirmId: string | null = $state(null);
  let busyId: string | null = $state(null);
  let copiedId: string | null = $state(null);
  let playing: { id: string; src: string } | null = $state(null);

  const groups = $derived(groupByDay(items, new Date()));

  const SEARCH_CAP = 200;
  let seq = 0;
  let capped = $state(false);
  let exporting = $state(false);
  let notice = $state("");

  async function load(reset: boolean) {
    const my = ++seq;
    const q = query.trim();
    loading = true;
    error = "";
    try {
      if (q) {
        // Search is not paginated.
        const page = await searchHistory(q, SEARCH_CAP);
        if (my !== seq) return;
        hasMore = false;
        capped = page.length >= SEARCH_CAP;
        items = page;
      } else {
        const before = reset ? undefined : items[items.length - 1]?.id;
        const page = await listHistory(before, PAGE);
        if (my !== seq) return;
        capped = false;
        hasMore = page.length === PAGE;
        items = reset ? page : [...items, ...page];
      }
    } catch (e) {
      if (my === seq) error = String(e);
    } finally {
      if (my === seq) loading = false;
    }
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  function onInput() {
    // Invalidate any in-flight load right away (including Load more).
    seq++;
    clearTimeout(timer);
    timer = setTimeout(() => load(true), 250);
  }
  onMount(() => {
    load(true);
    // A new dictation was saved: reload the first page, keeping the search query.
    const un = onHistoryChanged(() => load(true));
    return () => {
      clearTimeout(timer);
      un.then((f) => f());
    };
  });

  const time = (ms: number) =>
    new Date(ms).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });

  async function play(d: Dictation) {
    if (playing?.id === d.id) {
      playing = null;
      return;
    }
    try {
      const p = await audioFile(d.id);
      playing = p ? { id: d.id, src: convertFileSrc(p) } : null;
    } catch (e) {
      error = String(e);
    }
  }

  async function copy(d: Dictation) {
    try {
      await navigator.clipboard.writeText(d.text);
      copiedId = d.id;
      setTimeout(() => copiedId === d.id && (copiedId = null), 1200);
    } catch (e) {
      error = String(e);
    }
  }

  async function redo(d: Dictation) {
    busyId = d.id;
    error = "";
    try {
      const updated = await retranscribe(d.id);
      items = items.map((x) => (x.id === d.id ? updated : x));
    } catch (e) {
      error = String(e);
    } finally {
      busyId = null;
    }
  }

  async function remove(d: Dictation) {
    try {
      await deleteDictation(d.id);
      if (playing?.id === d.id) playing = null;
      items = items.filter((x) => x.id !== d.id);
    } catch (e) {
      error = String(e);
    } finally {
      confirmId = null;
    }
  }

  async function doExport(kind: "json" | "csv" | "audio") {
    if (exporting) return;
    exporting = true;
    notice = "";
    const ext = kind === "audio" ? "zip" : kind;
    try {
      const path = await save({
        defaultPath: `dictation-history.${ext}`,
        filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
      });
      if (path) {
        await exportHistory(kind, path);
        notice = "Export saved.";
        setTimeout(() => (notice = ""), 3000);
      }
    } catch (e) {
      error = String(e);
    } finally {
      exporting = false;
    }
  }
</script>

<div class="bar row">
  <input class="grow" type="search" placeholder="Search dictations" bind:value={query} oninput={onInput} />
  <button disabled={exporting} onclick={() => doExport("json")}>Export JSON</button>
  <button disabled={exporting} onclick={() => doExport("csv")}>Export CSV</button>
  <button disabled={exporting} onclick={() => doExport("audio")}>Export audio</button>
</div>

{#if error}<p class="error small">{error}</p>{/if}
{#if notice}<p class="small muted">{notice}</p>{/if}
{#if capped}<p class="small muted">Showing first {SEARCH_CAP} results</p>{/if}

{#each groups as g (g.label)}
  <h2>{g.label}</h2>
  {#each g.items as d (d.id)}
    <div class="item">
      <div class="meta muted small">
        <div>{time(d.created_at_ms)}</div>
        <div>{formatDuration(d.duration_ms)}</div>
      </div>
      <div class="grow">
        <div class="text" class:dim={d.status === "no_speech" || d.status === "error"}>
          {d.text || (d.status === "no_speech" ? "No speech detected" : d.status === "error" ? "Error" : "")}
        </div>
        <div class="muted small">
          {d.target_app ?? ""}{d.status === "copied_only" ? (d.target_app ? " · " : "") + "copied only" : ""}
        </div>
        {#if playing?.id === d.id}
          <!-- svelte-ignore a11y_media_has_caption -->
          <audio controls autoplay src={playing.src} onended={() => (playing = null)} onerror={() => { playing = null; error = "Can't play audio"; }}></audio>
        {/if}
      </div>
      <div class="actions row">
        {#if d.audio_path}
          <button onclick={() => play(d)}>{playing?.id === d.id ? "Stop" : "Play"}</button>
        {/if}
        <button onclick={() => copy(d)} disabled={!d.text}>{copiedId === d.id ? "Copied" : "Copy"}</button>
        {#if d.audio_path}
          <button onclick={() => redo(d)} disabled={busyId === d.id}>
            {busyId === d.id ? "Working…" : "Re-transcribe"}
          </button>
        {/if}
        {#if confirmId === d.id}
          <button class="danger" onclick={() => remove(d)}>Delete?</button>
          <button class="link" onclick={() => (confirmId = null)}>Cancel</button>
        {:else}
          <button class="danger" onclick={() => (confirmId = d.id)}>Delete</button>
        {/if}
      </div>
    </div>
  {/each}
{/each}

{#if !items.length && !loading}
  <p class="muted">{query.trim() ? "No matches." : "No dictations yet. Hold your hotkey and speak."}</p>
{/if}
{#if hasMore}
  <p><button onclick={() => load(false)} disabled={loading}>Load more</button></p>
{/if}

<style>
  .bar { position: sticky; top: -16px; background: var(--bg); padding: 0 0 8px; z-index: 1; }
  .bar input { min-width: 0; }
  .item { display: flex; gap: 12px; padding: 8px 0; border-bottom: 1px solid var(--line); align-items: flex-start; }
  .meta { width: 52px; flex: none; font-variant-numeric: tabular-nums; }
  .text {
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical;
    overflow: hidden; white-space: pre-wrap; word-break: break-word;
  }
  .text.dim { color: var(--muted); font-style: italic; }
  .actions { flex: none; flex-wrap: wrap; justify-content: flex-end; max-width: 270px; gap: 4px; }
  audio { width: 100%; height: 28px; margin-top: 6px; }
</style>
