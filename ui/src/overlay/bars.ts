export const BAR_COUNT = 9;
export const MIN_BAR = 0.15;
const WEIGHTS = [0.5, 0.7, 0.85, 1, 0.9, 1, 0.85, 0.7, 0.5];

/** Deterministic hash of an integer to [0, 1). */
function hash01(n: number): number {
  let x = (n | 0) ^ 0x9e3779b9;
  x = Math.imul(x ^ (x >>> 16), 0x85ebca6b);
  x = Math.imul(x ^ (x >>> 13), 0xc2b2ae35);
  x ^= x >>> 16;
  return (x >>> 0) / 4294967296;
}

const clamp = (v: number) => Math.min(1, Math.max(MIN_BAR, v));

/** Move each bar toward target * weight[i] +/- 0.1 jitter, eased by dt. */
export function step(current: number[], target: number, dt_ms: number, seed: number): number[] {
  const k = 1 - Math.exp(-dt_ms / 60);
  return WEIGHTS.map((w, i) => {
    const jitter = (hash01(seed + i) * 2 - 1) * 0.1;
    const goal = clamp(target * w + jitter);
    const from = current[i] ?? MIN_BAR;
    return clamp(from + (goal - from) * k);
  });
}

/** Perceptual boost of a raw RMS value. */
export function level(rms: number): number {
  return Math.min(1, Math.sqrt(Math.max(0, rms)) * 3);
}
