import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Types mirror the Rust serde shapes (crates/core, src-tauri/commands.rs).
export type Status = "pasted" | "copied_only" | "no_speech" | "error";
export type OverlayPosition = "BottomCenter" | "TopCenter" | "BottomLeft" | "BottomRight";
export type Retention = "Forever" | "Never" | { Days: number };

export interface Dictation {
  id: string;
  created_at_ms: number;
  duration_ms: number;
  text: string;
  raw_text: string;
  model: string;
  target_app: string | null;
  status: Status;
  audio_path: string | null;
}

export interface CleanupConfig {
  remove_fillers: boolean;
  fillers: string[];
  replacements: [string, string][];
  vocabulary: string[];
  capitalize_first: boolean;
  trailing_space: boolean;
}

export interface Settings {
  version: number;
  hotkey: { keys: string[] };
  input_device: string | null;
  mic_always_on: boolean;
  active_model: string;
  cleanup: CleanupConfig;
  overlay_position: OverlayPosition;
  launch_at_login: boolean;
  audio_retention: Retention;
  unload_after_idle_min: number | null;
}

export interface ModelFile {
  name: string;
  url: string;
  sha256: string;
  size: number;
}
export interface ModelInfo {
  id: string;
  label: string;
  kind: "Parakeet" | "Whisper";
  files: ModelFile[];
}
export interface ModelStatus {
  info: ModelInfo;
  installed: boolean;
  active: boolean;
}

export interface PermissionStatus {
  microphone: boolean;
  accessibility: boolean;
  input_monitoring: boolean;
}
export type PermissionKind = "microphone" | "accessibility" | "input_monitoring";

export interface ModelProgress {
  id: string;
  done: number;
  total: number;
}
export interface ModelDone {
  id: string;
  error: string | null;
}

export type ModelState =
  | { kind: "none" }
  | { kind: "loading" }
  | { kind: "ready" }
  | { kind: "error"; message: string };

export const getSettings = () => invoke<Settings>("get_settings");
export const setSettings = (s: Settings) => invoke<void>("set_settings", { s });
export const listInputDevices = () => invoke<string[]>("list_input_devices");

export const listHistory = (beforeId?: string, limit = 50) =>
  invoke<Dictation[]>("list_history", { beforeId: beforeId ?? null, limit });
export const searchHistory = (query: string, limit = 50) =>
  invoke<Dictation[]>("search_history", { query, limit });
export const deleteDictation = (id: string) => invoke<void>("delete_dictation", { id });
export const retranscribe = (id: string) => invoke<Dictation>("retranscribe", { id });
export const audioFile = (id: string) => invoke<string | null>("audio_file", { id });
export const exportHistory = (kind: "json" | "csv" | "audio", path: string) =>
  invoke<void>("export_history", { kind, path });
export const storageUsage = () => invoke<number>("storage_usage");

export const listModels = () => invoke<ModelStatus[]>("list_models");
export const downloadModel = (id: string) => invoke<void>("download_model", { id });
export const cancelDownload = (id: string) => invoke<void>("cancel_download", { id });
export const importModel = (id: string, fileName: string, path: string) =>
  invoke<void>("import_model", { id, fileName, path });
export const setActiveModel = (id: string) => invoke<void>("set_active_model", { id });

export const modelState = () => invoke<ModelState>("model_state");

export const permissionStatus = () => invoke<PermissionStatus>("permission_status");
export const openPermissionSettings = (kind: PermissionKind) =>
  invoke<void>("open_permission_settings", { kind });
export const requestPermission = (kind: PermissionKind) =>
  invoke<void>("request_permission", { kind });

export const onModelProgress = (cb: (p: ModelProgress) => void): Promise<UnlistenFn> =>
  listen<ModelProgress>("model-progress", (e) => cb(e.payload));
export const onModelDone = (cb: (p: ModelDone) => void): Promise<UnlistenFn> =>
  listen<ModelDone>("model-done", (e) => cb(e.payload));
export const onSettingsChanged = (cb: (s: Settings) => void): Promise<UnlistenFn> =>
  listen<Settings>("settings-changed", (e) => cb(e.payload));

export const onModelState = (cb: (s: ModelState) => void): Promise<UnlistenFn> =>
  listen<ModelState>("model-state", (e) => cb(e.payload));
export const onHistoryChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen<null>("history-changed", () => cb());

export const isMac = /Mac/i.test(
  typeof navigator === "undefined" ? "" : navigator.platform || navigator.userAgent,
);
