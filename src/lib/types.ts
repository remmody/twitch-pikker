// Mirrors the Rust structs (snake_case, as serialized by serde).

export type Engine = "silero" | "piper";
export type Gender = "female" | "male" | "unknown";
export type SoundChoice = "default_ping" | "soft_pop" | "telegram" | "custom";
export type Theme = "dark" | "light" | "system";

export interface AppSettings {
  twitch: { channel: string; username: string; oauth_token: string };
  audio: {
    output_device: string;
    sound: SoundChoice;
    custom_sound_path: string;
    notification_volume: number;
    cooldown_ms: number;
    play_while_speaking: boolean;
  };
  tts: {
    enabled: boolean;
    engine: Engine;
    voice: string;
    speed: number;
    volume: number;
    max_message_chars: number;
    queue_capacity: number;
    announce_username: boolean;
    username_template: string;
    read_emotes: boolean;
    read_links: boolean;
    models_dir: string;
    python: string;
    piper_binary: string;
  };
  notifications: { enabled: boolean };
  ui: { theme: Theme };
}

export type ConnStatus =
  | { state: "disconnected" | "connecting" | "connected" | "reconnecting" }
  | { state: "error"; message: string };

export interface ChatMessage {
  time: string;
  username: string;
  text: string;
  is_action: boolean;
}

export interface VoiceInfo {
  engine: Engine;
  id: string;
  name: string;
  language: string;
  gender: Gender;
  installed: boolean;
}

export type TtsReadiness = "ready" | "model_missing" | "runtime_missing";
export type MissingRuntime = "no_python" | "no_torch" | "no_piper";
export interface TtsStatus {
  state: TtsReadiness;
  missing: MissingRuntime | null;
  detail: string;
}

export interface RuntimeInfo {
  piper: boolean;
  python: boolean;
  torch: boolean;
  can_install_python: boolean;
}

export interface RuntimeProgress {
  state: DlState;
  step: "python" | "pytorch" | null;
  line: string;
  error: string | null;
}

export interface ModelValidation {
  ok: boolean;
  silero: boolean;
  piper_voices: string[];
  problems: string[];
}

export interface PackInfo {
  id: string;
  label: string;
  description: string;
  engine: Engine;
  approx_mb: number;
  default_selected: boolean;
  installed: boolean;
}
export interface ModelPacks {
  models_dir: string;
  packs: PackInfo[];
}

export type DlState = "idle" | "running" | "paused" | "failed" | "done" | "cancelled";
export interface FileProgress {
  name: string;
  received: number;
  total: number;
  done: boolean;
}
export interface DownloadProgress {
  state: DlState;
  error: string | null;
  files: FileProgress[];
  received: number;
  total: number;
  speed_bps: number;
}

export interface Diagnostics {
  connection: string;
  engine: string;
  voice: string;
  audio_backend: string;
  queue_len: number;
  last_error: string | null;
  config_path: string;
  models_dir: string;
  version: string;
}

/** Event name → payload. */
export interface EventMap {
  "chat:message": ChatMessage;
  "chat:status": ConnStatus;
  "tts:queue-updated": { len: number };
  "tts:error": { message: string | null };
  "audio:error": { message: string | null };
  "settings:updated": AppSettings;
  "models:progress": DownloadProgress;
  "models:changed": Record<string, never>;
  "runtime:progress": RuntimeProgress;
}
