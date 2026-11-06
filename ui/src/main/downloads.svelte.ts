import { onModelDone, onModelProgress, type ModelProgress } from "./api";

/** Live download state shared by the Models and FirstRun screens. */
export class Downloads {
  progress = $state<Record<string, ModelProgress>>({});
  errors = $state<Record<string, string>>({});
  /** Bumped whenever a download or import finishes, so lists can refresh. */
  doneTick = $state(0);

  start(id: string) {
    delete this.errors[id];
    this.progress[id] = { id, done: 0, total: 0 };
  }

  /** Subscribes to backend events; returns an unsubscribe function. */
  listen(): () => void {
    const subs = [
      onModelProgress((p) => (this.progress[p.id] = p)),
      onModelDone((d) => {
        delete this.progress[d.id];
        if (d.error) this.errors[d.id] = d.error;
        this.doneTick++;
      }),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }
}

export function percent(p: ModelProgress | undefined): number {
  return p && p.total > 0 ? Math.min(100, (p.done / p.total) * 100) : 0;
}
