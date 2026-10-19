import { useState } from "react";
import { FileAudio, Play } from "lucide-react";
import { api } from "../../lib/tauri";
import { useApp } from "../../stores/app";
import { Button, Card, Slider, Switch, useAsync } from "../../components/ui";
import type { SoundChoice } from "../../lib/types";

const SOUNDS: { value: SoundChoice; label: string; hint: string }[] = [
  { value: "default_ping", label: "Default ping", hint: "Clean single tone" },
  { value: "soft_pop", label: "Soft pop", hint: "Short, low and gentle" },
  { value: "telegram", label: "Telegram-like ping", hint: "Two quick rising notes" },
  { value: "custom", label: "Custom file", hint: "WAV, MP3 or OGG" },
];

export function NotificationsView() {
  const settings = useApp((s) => s.settings);
  const update = useApp((s) => s.update);
  const toast = useApp((s) => s.toast);
  const audioError = useApp((s) => s.audioError);
  const [fileErr, setFileErr] = useState<string | null>(null);

  const [test, testing] = useAsync(async () => {
    try {
      await api.testNotification();
    } catch (e) {
      toast("error", e instanceof Error ? e.message : String(e));
    }
  });
  const [pick, picking] = useAsync(async () => {
    try {
      const path = await api.pickSound();
      if (!path) return;
      setFileErr(null);
      update((s) => {
        s.audio.custom_sound_path = path;
        s.audio.sound = "custom";
      });
    } catch (e) {
      setFileErr(e instanceof Error ? e.message : String(e));
    }
  });

  if (!settings) return null;
  const a = settings.audio;
  const noFile = a.sound === "custom" && !a.custom_sound_path;

  return (
    <div className="mx-auto max-w-3xl space-y-6 p-6">
      <Card title="New message sound" description="Played by the app's audio engine, so volume and output device apply.">
        <Switch label="Play notification sound for new messages" checked={settings.notifications.enabled} onChange={(v) => update((s) => void (s.notifications.enabled = v))} />

        <fieldset>
          <legend className="mb-2 text-sm font-medium">Sound</legend>
          <div className="grid gap-2 sm:grid-cols-2" role="radiogroup" aria-label="Notification sound">
            {SOUNDS.map((o) => (
              <button
                key={o.value}
                type="button"
                role="radio"
                aria-checked={a.sound === o.value}
                onClick={() => update((s) => void (s.audio.sound = o.value))}
                className={`rounded-lg border p-3 text-left transition ${a.sound === o.value ? "border-accent bg-accent/10" : "border-line hover:border-muted/50"}`}
              >
                <span className="block text-sm font-medium">{o.label}</span>
                <span className="block text-xs text-muted">{o.hint}</span>
              </button>
            ))}
          </div>
        </fieldset>

        {a.sound === "custom" && (
          <div>
            <div className="flex flex-wrap items-center gap-3">
              <Button icon={<FileAudio className="h-4 w-4" />} onClick={() => void pick()} loading={picking}>Choose file…</Button>
              <span className="min-w-0 flex-1 truncate text-sm text-muted" title={a.custom_sound_path}>{a.custom_sound_path || "No file selected"}</span>
            </div>
            {(fileErr || noFile) && <p role="alert" className="mt-2 text-sm text-bad">{fileErr ?? "Choose a file, or the default ping is used."}</p>}
          </div>
        )}
        {audioError && <p role="alert" className="rounded-lg border border-bad/40 bg-bad/10 p-3 text-sm text-bad">{audioError}</p>}

        <div className="grid gap-x-8 gap-y-5 md:grid-cols-2">
          <Slider label="Volume" value={a.notification_volume} min={0} max={100} format={(v) => `${v}%`} onChange={(v) => update((s) => void (s.audio.notification_volume = v))} />
          <Slider label="Cooldown between sounds" value={a.cooldown_ms} min={0} max={5000} step={100} format={(v) => (v === 0 ? "off" : `${(v / 1000).toFixed(1)} s`)} onChange={(v) => update((s) => void (s.audio.cooldown_ms = v))} />
        </div>
        <Switch label="Play even while TTS is speaking" hint="Off: sounds are skipped during speech" checked={a.play_while_speaking} onChange={(v) => update((s) => void (s.audio.play_while_speaking = v))} />

        <div>
          <Button variant="primary" icon={<Play className="h-4 w-4" />} loading={testing} onClick={() => void test()}>Test sound</Button>
        </div>
      </Card>
    </div>
  );
}
