import { test, expect } from "vitest";
import { step, level } from "./bars";
test("returns 9 bars within bounds", () => {
  const b = step(Array(9).fill(0.15), 1, 16, 1);
  expect(b).toHaveLength(9); b.forEach(v => { expect(v).toBeGreaterThanOrEqual(0.15); expect(v).toBeLessThanOrEqual(1); });
});
test("silence settles to minimum", () => {
  let b = Array(9).fill(1); for (let i = 0; i < 100; i++) b = step(b, 0, 16, i);
  b.forEach(v => expect(v).toBeCloseTo(0.15, 2));
});
test("loud input raises middle bar more than edges", () => {
  let b = Array(9).fill(0.15); for (let i = 0; i < 100; i++) b = step(b, 1, 16, 7);
  expect(b[3]).toBeGreaterThan(b[0]);
});
test("deterministic for same seed", () => { expect(step(Array(9).fill(.5), .5, 16, 3)).toEqual(step(Array(9).fill(.5), .5, 16, 3)); });
test("level boosts and caps", () => { expect(level(0)).toBe(0); expect(level(1)).toBe(1); expect(level(0.01)).toBeCloseTo(0.3, 5); });
