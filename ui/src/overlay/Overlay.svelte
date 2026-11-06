<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { step, level, BAR_COUNT, MIN_BAR } from "./bars";

  type OverlayPayload = { kind: string; message?: string };

  const isMac = /Mac/i.test(navigator.platform || navigator.userAgent);
  const messages: Record<string, string> = {
    no_speech: "No speech",
    copied: isMac ? "Copied — press ⌘V" : "Copied — press Ctrl+V",
    mic_error: "Mic error",
    no_model: "No model",
    error: "Error",
  };

  let ov: OverlayPayload = $state({ kind: "hidden" });
  let canvas: HTMLCanvasElement | undefined = $state();
  let target = 0;
  let reduced = $state(false);
  let dotOpacity = $state(0.3);

  const recording = $derived(ov.kind === "recording");
  const visible = $derived(ov.kind !== "hidden");
  const text = $derived(messages[ov.kind]);

  // Animation loop: runs only while recording.
  let raf = 0;
  let bars: number[] = Array(BAR_COUNT).fill(MIN_BAR);
  function stop() {
    if (raf) cancelAnimationFrame(raf);
    raf = 0;
  }
  function start() {
    stop();
    bars = Array(BAR_COUNT).fill(MIN_BAR);
    let last = performance.now();
    let n = 0;
    const frame = (now: number) => {
      const dt = Math.min(100, now - last);
      last = now;
      if (reduced) {
        dotOpacity = 0.3 + 0.7 * target;
      } else {
        bars = step(bars, target, dt, n++);
        draw();
      }
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
  }
  function draw() {
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const w = 56, h = 20;
    if (canvas.width !== w * dpr) {
      canvas.width = w * dpr;
      canvas.height = h * dpr;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.fillStyle = getComputedStyle(canvas).color;
    const bw = 4, gap = 2.5;
    bars.forEach((v, i) => {
      const bh = Math.max(2, v * h);
      ctx.beginPath();
      ctx.roundRect(i * (bw + gap), (h - bh) / 2, bw, bh, 2);
      ctx.fill();
    });
  }

  $effect(() => {
    if (recording) {
      target = 0;
      start();
    } else {
      stop();
    }
    return stop;
  });

  onMount(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    reduced = mq.matches;
    const onMq = (e: MediaQueryListEvent) => (reduced = e.matches);
    mq.addEventListener("change", onMq);
    const unlisten = [
      listen<OverlayPayload>("overlay-state", (e) => (ov = e.payload)),
      listen<number>("audio-level", (e) => (target = level(e.payload))),
    ];
    return () => {
      mq.removeEventListener("change", onMq);
      unlisten.forEach((p) => p.then((f) => f()));
    };
  });
</script>

<div class="pill" class:visible>
  {#if recording}
    <span class="rec"></span>
    {#if reduced}
      <span class="level" style:opacity={dotOpacity}></span>
    {:else}
      <canvas bind:this={canvas} style="width:56px;height:20px"></canvas>
    {/if}
  {:else if ov.kind === "processing"}
    <span class="dots"><i></i><i></i><i></i></span>
  {:else if ov.kind === "done"}
    <span class="check">✓</span>
  {:else if text}
    <span class="msg">{text}</span>
  {/if}
</div>

<style>
  :global(html, body) {
    background: transparent;
    margin: 0;
    overflow: hidden;
  }
  :root {
    --pill-bg: rgba(255, 255, 255, 0.85);
    --pill-fg: #1d1d1f;
    --pill-border: rgba(0, 0, 0, 0.12);
  }
  @media (prefers-color-scheme: dark) {
    :root {
      --pill-bg: rgba(30, 30, 32, 0.85);
      --pill-fg: #f5f5f7;
      --pill-border: rgba(255, 255, 255, 0.16);
    }
  }
  .pill {
    box-sizing: border-box;
    width: 120px;
    height: 36px;
    border-radius: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--pill-fg);
    background: color-mix(in srgb, var(--pill-bg) 90%, transparent);
    border: 1px solid var(--pill-border);
    font: 500 12px/1 system-ui, -apple-system, "Segoe UI", sans-serif;
    white-space: nowrap;
    opacity: 0;
    transition: opacity 150ms ease-out;
  }
  .pill.visible {
    opacity: 1;
    transition: opacity 120ms ease-in;
  }
  .rec {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #ff3b30;
    flex: none;
  }
  .level {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--pill-fg);
  }
  canvas {
    color: var(--pill-fg);
  }
  .check {
    font-size: 16px;
  }
  .msg {
    font-size: 11px;
  }
  .dots {
    display: flex;
    gap: 5px;
  }
  .dots i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--pill-fg);
    animation: pulse 1s ease-in-out infinite;
  }
  .dots i:nth-child(2) {
    animation-delay: 0.15s;
  }
  .dots i:nth-child(3) {
    animation-delay: 0.3s;
  }
  @keyframes pulse {
    0%, 100% { opacity: 0.25; }
    50% { opacity: 1; }
  }
  @media (prefers-reduced-motion: reduce) {
    .dots i { animation-duration: 2s; }
  }
</style>
