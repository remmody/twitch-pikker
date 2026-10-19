import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  ConnStatus,
  Diagnostics,
  DownloadProgress,
  Engine,
  ModelPacks,
  ModelValidation,
  TtsStatus,
  VoiceInfo,
} from "./types";

/** Backend errors arrive as plain user-readable strings; normalize to Error. */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(typeof e === "string" ? e : e instanceof Error ? e.message : "Unexpected error");
  }
}

export const api = {
  connect: (channel: string) => call<void>("connect_to_channel", { channel }),
  disconnect: () => call<void>("disconnect_from_channel"),
  connectionStatus: () => call<ConnStatus>("get_connection_status"),

  getSettings: () => call<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => call<void>("save_settings", { settings }),
  diagnostics: () => call<Diagnostics>("get_diagnostics"),
  openConfigFolder: () => call<void>("open_config_folder"),
  openModelsFolder: () => call<void>("open_models_folder"),
  startupNotice: () => call<string | null>("take_startup_notice"),
  checkExecutable: (name: string) => call<boolean>("check_executable", { name }),

  listVoices: (engine: Engine) => call<VoiceInfo[]>("list_tts_voices", { engine }),
  testTts: (text: string, engine: Engine, voiceId: string) => call<void>("test_tts", { text, engine, voiceId }),
  pickTtsModel: () => call<string | null>("pick_tts_model"),
  validateTtsModel: (path: string) => call<ModelValidation>("validate_tts_model", { path }),
  ttsStatus: () => call<TtsStatus>("get_tts_status"),

  modelPacks: () => call<ModelPacks>("get_model_packs"),
  startDownload: (packIds: string[]) => call<void>("start_model_download", { packIds }),
  resumeDownload: () => call<void>("resume_model_download"),
  pauseDownload: () => call<void>("pause_model_download"),
  cancelDownload: () => call<void>("cancel_model_download"),
  downloadProgress: () => call<DownloadProgress>("get_download_progress"),

  testNotification: () => call<void>("test_notification_sound"),
  pickSound: () => call<string | null>("pick_notification_sound"),
  audioDevices: () => call<string[]>("list_audio_devices"),
  setMuted: (muted: boolean) => call<void>("set_muted", { muted }),
};
