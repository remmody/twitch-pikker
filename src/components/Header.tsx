import { useState } from "react";
import { Plug, Unplug, VolumeX, Volume2 } from "lucide-react";
import { useApp } from "../stores/app";
import { useUi } from "../stores/ui";
import { channelError } from "../lib/format";
import { Button, inputCls, Switch, useAsync } from "./ui";
import type { ConnStatus } from "../lib/types";

const PILL: Record<ConnStatus["state"], { dot: string; text: string; label: string }> = {
  connected: { dot: "bg-ok", text: "text-ok", label: "Connected" },
  connecting: { dot: "bg-warn animate-pulse", text: "text-warn", label: "Connecting…" },
  reconnecting: { dot: "bg-warn animate-pulse", text: "text-warn", label: "Reconnecting…" },
  error: { dot: "bg-bad", text: "text-bad", label: "Error" },
  disconnected: { dot: "bg-muted", text: "text-muted", label: "Idle" },
};

export function StatusPill({ status }: { status: ConnStatus }) {
  const p = PILL[status.state];
  return (
    <span
      className={`inline-flex items-center gap-2 rounded-full border border-line bg-surface2 px-3 py-1 text-sm font-medium ${p.text}`}
      role="status"
      title={status.state === "error" ? status.message : undefined}
    >
      <span className={`h-2 w-2 rounded-full ${p.dot}`} aria-hidden />
      {p.label}
    </span>
  );
}

export function Header() {
  const settings = useApp((s) => s.settings);
  const status = useApp((s) => s.status);
  const count = useApp((s) => s.messageCount);
  const update = useApp((s) => s.update);
  const connect = useApp((s) => s.connect);
  const disconnect = useApp((s) => s.disconnect);
  const setMuted = useApp((s) => s.setMuted);
  const muted = useUi((s) => s.muted);
  const [draft, setDraft] = useState<string | null>(null);
  const [touched, setTouched] = useState(false);
  const [doConnect, connecting] = useAsync(connect);
  const [doDisconnect, disconnecting] = useAsync(disconnect);

  if (!settings) return <header className="h-16 border-b border-line" />;
  const live = status.state === "connected" || status.state === "connecting" || status.state === "reconnecting";
  const channel = draft ?? settings.twitch.channel;
  const err = touched ? channelError(channel) : null;

  const submit = () => {
    setTouched(true);
    if (channelError(channel)) return;
    update((s) => void (s.twitch.channel = channel.trim().replace(/^#/, "").toLowerCase()));
    void doConnect(channel);
  };

  return (
    <header className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-line bg-surface px-5 py-3">
      <div className="flex min-w-0 items-center gap-3">
        {live ? (
          <h1 className="truncate text-lg font-semibold">#{settings.twitch.channel}</h1>
        ) : (
          <form
            className="flex items-start gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              submit();
            }}
          >
            <div>
              <input
                aria-label="Twitch channel"
                aria-invalid={!!err}
                aria-describedby={err ? "channel-err" : undefined}
                className={`${inputCls} w-48`}
                placeholder="channel name"
                value={channel}
                onChange={(e) => setDraft(e.target.value)}
                onBlur={() => setTouched(true)}
              />
              {err && <p id="channel-err" role="alert" className="mt-1 text-xs text-bad">{err}</p>}
            </div>
          </form>
        )}
        <StatusPill status={status} />
        {status.state === "error" && <span className="max-w-xs truncate text-xs text-bad" title={status.message}>{status.message}</span>}
      </div>

      <div className="ml-auto flex flex-wrap items-center gap-4">
        <span className="text-sm tabular-nums text-muted" aria-label="Messages this session">{count.toLocaleString()} msgs</span>
        <div className="flex items-center gap-4">
          <Switch label="TTS" checked={settings.tts.enabled} onChange={(v) => update((s) => void (s.tts.enabled = v))} />
          <Switch label="Sound" checked={settings.notifications.enabled} onChange={(v) => update((s) => void (s.notifications.enabled = v))} />
        </div>
        <Button
          variant={muted ? "danger" : "secondary"}
          onClick={() => void setMuted(!muted)}
          aria-pressed={muted}
          icon={muted ? <VolumeX className="h-4 w-4" /> : <Volume2 className="h-4 w-4" />}
          title="Temporarily silence TTS and sounds without changing saved settings"
        >
          {muted ? "Muted" : "Mute all"}
        </Button>
        {live ? (
          <Button onClick={() => void doDisconnect()} loading={disconnecting} icon={<Unplug className="h-4 w-4" />}>Disconnect</Button>
        ) : (
          <Button variant="primary" onClick={submit} loading={connecting} icon={<Plug className="h-4 w-4" />}>Connect</Button>
        )}
      </div>
    </header>
  );
}
