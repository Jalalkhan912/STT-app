import { modelState, onModelState, type ModelState } from "./api";

/** Live model load state (loading / ready / error), shared across screens. */
export class ModelStore {
  value = $state<ModelState>({ kind: "none" });

  /** Subscribes to backend events and fetches the current state; returns an unsubscribe function. */
  listen(): () => void {
    let live = true;
    // An event that arrives before the initial fetch resolves is newer; keep it.
    let gotEvent = false;
    const sub = onModelState((s) => {
      gotEvent = true;
      this.value = s;
    });
    modelState()
      .then((s) => live && !gotEvent && (this.value = s))
      .catch(() => {});
    return () => {
      live = false;
      sub.then((f) => f());
    };
  }
}

/** Banner text for the states that need one. */
export function modelBanner(s: ModelState): string | null {
  if (s.kind === "loading") return "Loading model…";
  if (s.kind === "error") return `Model failed to load: ${s.message}`;
  return null;
}
