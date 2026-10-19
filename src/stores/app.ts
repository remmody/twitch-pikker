import { create } from "zustand";
import { api } from "../lib/tauri";
import { on } from "../lib/events";
import type {
  AppSettings,
  ChatMessage,
  ConnStatus,
  DownloadProgress,
  Engine,
  ModelPacks,
  RuntimeInfo,
  RuntimeProgress,
  TtsStatus,
  VoiceInfo,
} from "../lib/types";
import { useUi } from "./ui";

const MAX_MESSAGES = 1000;
const SAVE_DELAY_MS = 250;

export type LogMessage = ChatMessage & { id: number };
export interface Toast {
  id: number;
  kind: "ok" | "error" | "info";
  text: string;
}

interface AppStore {
  ready: boolean;
  settings: AppSettings | null;
  status: ConnStatus;
  messages: LogMessage[];
  messageCount: number;
  queueLen: number;
  ttsError: string | null;
  audioError: string | null;
  ttsStatus: TtsStatus | null;
  voices: Partial<Record<Engine, VoiceInfo[]>>;
  packs: ModelPacks | null;
  download: DownloadProgress | null;
  runtime: RuntimeInfo | null;
  runtimeProgress: RuntimeProgress | null;
  toasts: Toast[];
  wizardOpen: boolean;

  init: () => Promise<void>;
  toast: (kind: Toast["kind"], text: string) => void;
  dismissToast: (id: number) => void;
  update: (mutate: (draft: AppSettings) => void) => void;
  connect: (channel: string) => Promise<void>;
  disconnect: () => Promise<void>;
  setMuted: (muted: boolean) => Promise<void>;
  clearMessages: () => void;
  refreshModels: () => Promise<void>;
  refreshTtsStatus: () => Promise<void>;
  openWizard: (open: boolean) => void;
  startDownload: (packIds: string[]) => Promise<void>;
  resumeDownload: () => Promise<void>;
  pauseDownload: () => Promise<void>;
  cancelDownload: () => Promise<void>;
  installRuntimes: (python: boolean, pytorch: boolean) => Promise<void>;
  cancelRuntimeInstall: () => Promise<void>;
  /** Opens the setup dialog and returns false if offline TTS is not ready yet. */
  ensureTtsReady: () => boolean;
}

let toastSeq = 0;
let msgSeq = 0;
let initialized = false;
let saveTimer: ReturnType<typeof setTimeout> | undefined;
let saving = false;
let pending: LogMessage[] = [];
let frame = 0;

export const useApp = create<AppStore>((set, get) => {
  const toast: AppStore["toast"] = (kind, text) => {
    const id = ++toastSeq;
    set((s) => ({ toasts: [...s.toasts.slice(-3), { id, kind, text }] }));
    setTimeout(() => get().dismissToast(id), kind === "error" ? 7000 : 4000);
  };
  const fail = (e: unknown) => toast("error", e instanceof Error ? e.message : String(e));

  /** Batch bursts of chat messages into one render per animation frame. */
  const flushMessages = () => {
    frame = 0;
    const batch = pending;
    pending = [];
    set((s) => ({
      messages: [...s.messages, ...batch].slice(-MAX_MESSAGES),
      messageCount: s.messageCount + batch.length,
    }));
  };

  const flushSave = async () => {
    saveTimer = undefined;
    const settings = get().settings;
    if (!settings) return;
    saving = true;
    try {
      await api.saveSettings(settings);
    } catch (e) {
      fail(e);
      // Backend rejected the values: fall back to what it actually has.
      try {
        set({ settings: await api.getSettings() });
      } catch {
        /* keep local copy */
      }
    } finally {
      saving = false;
    }
    void get().refreshTtsStatus();
  };

  return {
    ready: false,
    settings: null,
    status: { state: "disconnected" },
    messages: [],
    messageCount: 0,
    queueLen: 0,
    ttsError: null,
    audioError: null,
    ttsStatus: null,
    voices: {},
    packs: null,
    download: null,
    runtime: null,
    runtimeProgress: null,
    toasts: [],
    wizardOpen: false,

    toast,
    dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

    init: async () => {
      if (initialized) return;
      initialized = true;
      await Promise.all([
        on("chat:message", (m) => {
          pending.push({ ...m, id: ++msgSeq });
          if (!frame) frame = requestAnimationFrame(flushMessages);
        }),
        on("chat:status", (status) => set({ status })),
        on("tts:queue-updated", ({ len }) => set({ queueLen: len })),
        on("tts:error", ({ message }) => set({ ttsError: message })),
        on("audio:error", ({ message }) => set({ audioError: message })),
        on("settings:updated", (settings) => {
          if (!saveTimer && !saving) set({ settings });
        }),
        on("models:progress", (download) => set({ download })),
        on("runtime:progress", (runtimeProgress) => set({ runtimeProgress })),
        on("models:changed", () => {
          void get().refreshModels();
          toast("ok", "Offline TTS components installed.");
        }),
      ]);
      try {
        const [settings, status, download, runtimeProgress] = await Promise.all([
          api.getSettings(),
          api.connectionStatus(),
          api.downloadProgress(),
          api.runtimeProgress(),
        ]);
        set({ settings, status, download, runtimeProgress });
        await api.setMuted(useUi.getState().muted);
        await get().refreshModels();
        const notice = await api.startupNotice();
        if (notice) toast("info", notice);

        // Anything missing (models or the programs they run on)? Ask before installing.
        if (get().ttsStatus?.state !== "ready") set({ wizardOpen: true });
      } catch (e) {
        fail(e);
      }
      set({ ready: true });
    },

    update: (mutate) => {
      const cur = get().settings;
      if (!cur) return;
      const next = structuredClone(cur);
      mutate(next);
      set({ settings: next });
      clearTimeout(saveTimer);
      saveTimer = setTimeout(() => void flushSave(), SAVE_DELAY_MS);
    },

    connect: async (channel) => {
      try {
        await api.connect(channel);
      } catch (e) {
        fail(e);
      }
    },
    disconnect: async () => {
      try {
        await api.disconnect();
      } catch (e) {
        fail(e);
      }
    },
    setMuted: async (muted) => {
      useUi.getState().set({ muted });
      try {
        await api.setMuted(muted);
      } catch (e) {
        fail(e);
      }
    },
    clearMessages: () => set({ messages: [] }),

    refreshModels: async () => {
      try {
        const [silero, piper, packs, runtime] = await Promise.all([
          api.listVoices("silero"),
          api.listVoices("piper"),
          api.modelPacks(),
          api.runtimeInfo(),
        ]);
        set({ voices: { silero, piper }, packs, runtime });
        await get().refreshTtsStatus();
      } catch (e) {
        fail(e);
      }
    },
    refreshTtsStatus: async () => {
      try {
        set({ ttsStatus: await api.ttsStatus() });
      } catch (e) {
        fail(e);
      }
    },

    openWizard: (open) => set({ wizardOpen: open }),
    ensureTtsReady: () => {
      if (get().ttsStatus?.state === "ready") return true;
      set({ wizardOpen: true });
      return false;
    },
    installRuntimes: async (python, pytorch) => {
      try {
        await api.installRuntimes(python, pytorch);
      } catch (e) {
        fail(e);
      }
    },
    cancelRuntimeInstall: async () => {
      try {
        await api.cancelRuntimeInstall();
      } catch (e) {
        fail(e);
      }
    },
    startDownload: async (ids) => {
      try {
        await api.startDownload(ids);
      } catch (e) {
        fail(e);
      }
    },
    resumeDownload: async () => {
      try {
        await api.resumeDownload();
      } catch (e) {
        fail(e);
      }
    },
    pauseDownload: async () => {
      try {
        await api.pauseDownload();
      } catch (e) {
        fail(e);
      }
    },
    cancelDownload: async () => {
      try {
        await api.cancelDownload();
      } catch (e) {
        fail(e);
      }
    },
  };
});
