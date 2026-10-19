import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { EventMap } from "./types";

/** Typed wrapper over Tauri's `listen`. */
export function on<K extends keyof EventMap>(name: K, cb: (payload: EventMap[K]) => void): Promise<UnlistenFn> {
  return listen<EventMap[K]>(name, (e) => cb(e.payload));
}
