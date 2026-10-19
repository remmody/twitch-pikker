import { useEffect, useRef, useState } from "react";
import { CheckCircle2, Download, FolderOpen, Pause, Play, RotateCw, X } from "lucide-react";
import { api } from "../../lib/tauri";
import { useApp } from "../../stores/app";
import { formatBytes, formatEta } from "../../lib/format";
import { Badge, Button, ProgressBar, useAsync } from "../../components/ui";

export function SetupWizard() {
  const open = useApp((s) => s.wizardOpen);
  const close = useApp((s) => s.openWizard);
  const packs = useApp((s) => s.packs);
  const dl = useApp((s) => s.download);
  const settings = useApp((s) => s.settings);
  const voices = useApp((s) => s.voices);
  const status = useApp((s) => s.ttsStatus);
  const update = useApp((s) => s.update);
  const toast = useApp((s) => s.toast);
  const refreshModels = useApp((s) => s.refreshModels);
  const { startDownload, resumeDownload, pauseDownload, cancelDownload } = useApp.getState();

  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [consent, setConsent] = useState(false);
  const [folderErr, setFolderErr] = useState<string | null>(null);
  const [forceSelect, setForceSelect] = useState(false);
  const seeded = useRef(false);
  const first = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (packs && !seeded.current) {
      seeded.current = true;
      setSelected(new Set(packs.packs.filter((p) => p.default_selected && !p.installed).map((p) => p.id)));
    }
  }, [packs]);

  useEffect(() => {
    if (!open) return;
    first.current?.focus();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && dl?.state !== "running" && close(false);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, close, dl?.state]);

  const [start, starting] = useAsync(async () => {
    setForceSelect(false);
    await startDownload([...selected]);
  });
  const [pickFolder, picking] = useAsync(async () => {
    try {
      const path = await api.pickTtsModel();
      if (!path) return;
      const v = await api.validateTtsModel(path);
      if (!v.ok) return setFolderErr(v.problems.join(" "));
      setFolderErr(null);
      update((s) => void (s.tts.models_dir = path));
      setTimeout(() => void refreshModels(), 600);
      toast("ok", "Using your existing models folder.");
      close(false);
    } catch (e) {
      setFolderErr(e instanceof Error ? e.message : String(e));
    }
  });

  if (!open || !packs || !settings) return null;

  const active = dl && dl.state !== "idle" && dl.files.length > 0 && !forceSelect;
  const totalMb = packs.packs.filter((p) => selected.has(p.id) && !p.installed).reduce((n, p) => n + p.approx_mb, 0);
  const pct = dl && dl.total > 0 ? (dl.received / dl.total) * 100 : 0;

  const engineVoices = voices[settings.tts.engine] ?? [];
  const currentVoice = engineVoices.find((v) => v.id === settings.tts.voice);
  const otherInstalled = (g: "male" | "female") => engineVoices.find((v) => v.installed && v.gender === g);

  const toggle = (id: string) =>
    setSelected((s) => {
      const n = new Set(s);
      n.has(id) ? n.delete(id) : n.add(id);
      return n;
    });

  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/60 p-4" role="presentation">
      <div role="dialog" aria-modal="true" aria-labelledby="wiz-title" className="max-h-full w-full max-w-2xl overflow-y-auto rounded-2xl border border-line bg-surface p-6 shadow-card">
        <div className="flex items-start justify-between gap-4">
          <div>
            <h2 id="wiz-title" className="text-xl font-semibold">Set up offline Russian voices</h2>
            <p className="mt-1 text-sm text-muted">Download voice models once. After that, speech is generated entirely on this computer — no internet needed.</p>
          </div>
          <button type="button" ref={first} aria-label="Close" disabled={dl?.state === "running"} onClick={() => close(false)} className="rounded p-1 text-muted hover:text-fg disabled:opacity-40">
            <X className="h-5 w-5" />
          </button>
        </div>

        {!active && (
          <div className="mt-5 space-y-4">
            <ul className="space-y-2">
              {packs.packs.map((p) => (
                <li key={p.id}>
                  <label className={`flex items-start gap-3 rounded-lg border border-line p-3 ${p.installed ? "opacity-60" : "cursor-pointer hover:border-muted/50"}`}>
                    <input type="checkbox" className="mt-1" disabled={p.installed} checked={p.installed || selected.has(p.id)} onChange={() => toggle(p.id)} />
                    <span className="min-w-0 flex-1">
                      <span className="block text-sm font-medium">{p.label}</span>
                      <span className="block text-xs text-muted">{p.description}</span>
                    </span>
                    {p.installed ? <Badge tone="ok">Installed</Badge> : <span className="text-sm tabular-nums text-muted">≈ {p.approx_mb} MB</span>}
                  </label>
                </li>
              ))}
            </ul>

            <dl className="space-y-1 rounded-lg bg-surface2 p-3 text-sm">
              <div className="flex justify-between gap-4"><dt className="text-muted">Download size</dt><dd className="tabular-nums">≈ {totalMb} MB</dd></div>
              <div className="flex justify-between gap-4"><dt className="shrink-0 text-muted">Stored in</dt><dd className="break-all text-right">{packs.models_dir}</dd></div>
              <div className="flex justify-between gap-4"><dt className="text-muted">Sources</dt><dd className="text-right">models.silero.ai, huggingface.co (rhasspy/piper-voices)</dd></div>
            </dl>
            <p className="text-xs text-muted">
              Files are verified with SHA-256 and stored locally. Silero needs Python with PyTorch, and Piper needs the Piper program; these are installed separately (see the README).
            </p>

            <label className="flex items-start gap-3 text-sm">
              <input type="checkbox" className="mt-1" checked={consent} onChange={(e) => setConsent(e.target.checked)} />
              I agree to download these files from the sources above. Nothing else is sent anywhere.
            </label>

            <div className="flex flex-wrap items-center gap-2">
              <Button variant="primary" icon={<Download className="h-4 w-4" />} disabled={!consent || selected.size === 0} loading={starting} onClick={() => void start()}>
                Download {totalMb > 0 ? `${totalMb} MB` : ""}
              </Button>
              <Button icon={<FolderOpen className="h-4 w-4" />} loading={picking} onClick={() => void pickFolder()}>I already have models…</Button>
              <Button variant="ghost" onClick={() => close(false)}>Not now</Button>
            </div>
            {folderErr && <p role="alert" className="text-sm text-bad">{folderErr}</p>}
          </div>
        )}

        {active && dl && (
          <div className="mt-5 space-y-4" aria-live="polite">
            {dl.state === "done" ? (
              <div className="space-y-3">
                <p className="flex items-center gap-2 font-medium text-ok"><CheckCircle2 className="h-5 w-5" aria-hidden /> Voices installed and verified.</p>
                <p className="text-sm">
                  Default voice: <strong>{currentVoice?.name ?? settings.tts.voice}</strong> ({currentVoice?.gender ?? "—"}), {settings.tts.engine === "silero" ? "Silero" : "Piper"}.
                </p>
                {status && status.state !== "ready" && <p role="status" className="rounded-lg border border-warn/40 bg-warn/10 p-3 text-sm text-warn">{status.detail}</p>}
                <div className="flex flex-wrap gap-2">
                  {currentVoice?.gender !== "female" && otherInstalled("female") && (
                    <Button onClick={() => update((s) => void (s.tts.voice = otherInstalled("female")!.id))}>Use female voice</Button>
                  )}
                  {currentVoice?.gender !== "male" && otherInstalled("male") && (
                    <Button onClick={() => update((s) => void (s.tts.voice = otherInstalled("male")!.id))}>Switch to a male voice</Button>
                  )}
                  <Button variant="primary" onClick={() => close(false)}>Finish</Button>
                  <Button variant="ghost" onClick={() => setForceSelect(true)}>Add more voices</Button>
                </div>
              </div>
            ) : (
              <>
                <div>
                  <div className="mb-1 flex justify-between text-sm">
                    <span className="font-medium">{dl.state === "running" ? "Downloading…" : dl.state === "paused" ? "Paused" : dl.state === "failed" ? "Download failed" : "Cancelled"}</span>
                    <span className="tabular-nums text-muted">{formatBytes(dl.received)} / {dl.total ? formatBytes(dl.total) : "?"}</span>
                  </div>
                  <ProgressBar value={pct} label="Overall progress" />
                  {dl.state === "running" && (
                    <p className="mt-1 text-xs tabular-nums text-muted">{formatBytes(dl.speed_bps)}/s · about {formatEta(Math.max(0, dl.total - dl.received), dl.speed_bps)} left</p>
                  )}
                </div>
                <ul className="space-y-2">
                  {dl.files.map((f) => (
                    <li key={f.name}>
                      <div className="mb-1 flex justify-between text-xs">
                        <span className="truncate">{f.name}</span>
                        <span className="tabular-nums text-muted">{f.done ? "done" : `${formatBytes(f.received)} / ${f.total ? formatBytes(f.total) : "?"}`}</span>
                      </div>
                      <ProgressBar value={f.done ? 100 : f.total ? (f.received / f.total) * 100 : 0} label={`${f.name} progress`} />
                    </li>
                  ))}
                </ul>
                {dl.error && <p role="alert" className="rounded-lg border border-bad/40 bg-bad/10 p-3 text-sm text-bad">{dl.error}</p>}
                <div className="flex flex-wrap gap-2">
                  {dl.state === "running" && <Button icon={<Pause className="h-4 w-4" />} onClick={() => void pauseDownload()}>Pause</Button>}
                  {dl.state === "paused" && <Button variant="primary" icon={<Play className="h-4 w-4" />} onClick={() => void resumeDownload()}>Resume</Button>}
                  {dl.state === "failed" && <Button variant="primary" icon={<RotateCw className="h-4 w-4" />} onClick={() => void resumeDownload()}>Retry</Button>}
                  {(dl.state === "running" || dl.state === "paused" || dl.state === "failed") && <Button variant="danger" onClick={() => void cancelDownload()}>Cancel</Button>}
                  {(dl.state === "failed" || dl.state === "cancelled") && (
                    <>
                      <Button icon={<FolderOpen className="h-4 w-4" />} loading={picking} onClick={() => void pickFolder()}>Choose existing models folder…</Button>
                      <Button variant="ghost" onClick={() => setForceSelect(true)}>Back</Button>
                    </>
                  )}
                </div>
                {folderErr && <p role="alert" className="text-sm text-bad">{folderErr}</p>}
              </>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
