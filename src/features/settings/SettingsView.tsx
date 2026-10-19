import { useEffect, useState } from "react";
import { Eye, EyeOff, FolderOpen, RefreshCw } from "lucide-react";
import { api } from "../../lib/tauri";
import { channelError } from "../../lib/format";
import { useApp } from "../../stores/app";
import { Badge, Button, Card, Field, inputCls, Segmented } from "../../components/ui";
import type { Diagnostics, Theme } from "../../lib/types";

function ExecutableField({ label, value, onChange }: { label: string; value: string; onChange: (v: string) => void }) {
  const [found, setFound] = useState<boolean | null>(null);
  useEffect(() => {
    const t = setTimeout(() => void api.checkExecutable(value).then(setFound).catch(() => setFound(null)), 300);
    return () => clearTimeout(t);
  }, [value]);
  return (
    <Field label={label} error={found === false ? "Not found on this computer." : null} hint={found ? "Found." : undefined}>
      {(p) => <input {...p} className={inputCls} value={value} onChange={(e) => onChange(e.target.value)} spellCheck={false} />}
    </Field>
  );
}

export function SettingsView() {
  const settings = useApp((s) => s.settings);
  const update = useApp((s) => s.update);
  const status = useApp((s) => s.status);
  const queueLen = useApp((s) => s.queueLen);
  const ttsError = useApp((s) => s.ttsError);
  const audioError = useApp((s) => s.audioError);
  const toast = useApp((s) => s.toast);
  const [devices, setDevices] = useState<string[]>([]);
  const [diag, setDiag] = useState<Diagnostics | null>(null);
  const [showToken, setShowToken] = useState(false);

  const engine = settings?.tts.engine;
  const voice = settings?.tts.voice;
  const device = settings?.audio.output_device;

  useEffect(() => {
    void api.audioDevices().then(setDevices).catch(() => setDevices([]));
  }, []);

  // Diagnostics refresh when something it reports changes (event-driven, no timer).
  const refreshDiag = () => void api.diagnostics().then(setDiag).catch(() => setDiag(null));
  useEffect(refreshDiag, [status, queueLen, ttsError, audioError, engine, voice, device]);

  if (!settings) return null;
  const fail = (e: Error) => toast("error", e.message);
  const chErr = settings.twitch.channel ? channelError(settings.twitch.channel) : null;

  return (
    <div className="mx-auto max-w-3xl space-y-6 p-6">
      <Card title="Appearance">
        <Segmented<Theme> label="Theme" value={settings.ui.theme} onChange={(v) => update((s) => void (s.ui.theme = v))} options={[{ value: "dark", label: "Dark" }, { value: "light", label: "Light" }, { value: "system", label: "System" }]} />
      </Card>

      <Card title="Audio output">
        <Field label="Output device" hint="TTS and notification sounds use this device.">
          {(p) => (
            <select {...p} className={inputCls} value={settings.audio.output_device} onChange={(e) => update((s) => void (s.audio.output_device = e.target.value))}>
              <option value="">System default</option>
              {settings.audio.output_device && !devices.includes(settings.audio.output_device) && <option value={settings.audio.output_device}>{settings.audio.output_device} (unavailable)</option>}
              {devices.map((d) => <option key={d} value={d}>{d}</option>)}
            </select>
          )}
        </Field>
      </Card>

      <Card title="Twitch" description="Chat is read anonymously. The account fields are optional and stay on this computer.">
        <Field label="Channel" error={chErr}>
          {(p) => <input {...p} className={inputCls} value={settings.twitch.channel} onChange={(e) => update((s) => void (s.twitch.channel = e.target.value))} placeholder="channel name" />}
        </Field>
        <div className="grid gap-4 md:grid-cols-2">
          <Field label="Username (optional)">{(p) => <input {...p} className={inputCls} autoComplete="off" value={settings.twitch.username} onChange={(e) => update((s) => void (s.twitch.username = e.target.value))} />}</Field>
          <Field label="OAuth token (optional)">
            {(p) => (
              <div className="flex gap-2">
                <input {...p} className={inputCls} type={showToken ? "text" : "password"} autoComplete="off" spellCheck={false} value={settings.twitch.oauth_token} onChange={(e) => update((s) => void (s.twitch.oauth_token = e.target.value))} />
                <Button variant="ghost" aria-label={showToken ? "Hide token" : "Show token"} onClick={() => setShowToken(!showToken)}>{showToken ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}</Button>
              </div>
            )}
          </Field>
        </div>
        <p className="text-xs text-muted">Changes to the channel apply the next time you connect.</p>
      </Card>

      <Card title="Offline TTS runtimes" description="Programs the voices run on. They are not downloaded by this app.">
        <ExecutableField label="Python (for Silero)" value={settings.tts.python} onChange={(v) => update((s) => void (s.tts.python = v))} />
        <ExecutableField label="Piper executable" value={settings.tts.piper_binary} onChange={(v) => update((s) => void (s.tts.piper_binary = v))} />
      </Card>

      <Card title="Files">
        <dl className="space-y-2 text-sm">
          <div><dt className="text-muted">Config file</dt><dd className="break-all">{diag?.config_path ?? "…"}</dd></div>
          <div><dt className="text-muted">Models folder</dt><dd className="break-all">{diag?.models_dir ?? "…"}</dd></div>
        </dl>
        <div className="flex flex-wrap gap-2">
          <Button icon={<FolderOpen className="h-4 w-4" />} onClick={() => void api.openConfigFolder().catch(fail)}>Open config folder</Button>
          <Button icon={<FolderOpen className="h-4 w-4" />} onClick={() => void api.openModelsFolder().catch(fail)}>Open models folder</Button>
        </div>
      </Card>

      <Card title="Diagnostics">
        <dl className="grid gap-x-8 gap-y-3 text-sm sm:grid-cols-2">
          <Row k="Twitch connection" v={diag?.connection} />
          <Row k="TTS engine" v={diag?.engine} />
          <Row k="Selected voice" v={diag?.voice} />
          <Row k="Audio backend" v={diag?.audio_backend} tone={diag ? (diag.audio_backend === "OK" ? "ok" : "bad") : undefined} />
          <Row k="Speech queue" v={diag ? String(diag.queue_len) : undefined} />
          <Row k="App version" v={diag?.version} />
          <div className="sm:col-span-2"><dt className="text-muted">Last error</dt><dd className={diag?.last_error ? "text-bad" : ""}>{diag?.last_error ?? "None"}</dd></div>
        </dl>
        <Button variant="ghost" icon={<RefreshCw className="h-4 w-4" />} onClick={refreshDiag}>Refresh</Button>
      </Card>
    </div>
  );
}

function Row({ k, v, tone }: { k: string; v?: string; tone?: "ok" | "bad" }) {
  return (
    <div>
      <dt className="text-muted">{k}</dt>
      <dd>{tone && v ? <Badge tone={tone}>{v}</Badge> : (v ?? "…")}</dd>
    </div>
  );
}
