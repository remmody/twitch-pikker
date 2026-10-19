import { useEffect, useRef, useState } from "react";
import { CheckCircle2, Download, FolderOpen, Pause, Play, RotateCw, X } from "lucide-react";
import { api } from "../../lib/tauri";
import { useApp } from "../../stores/app";
import { formatBytes, formatEta } from "../../lib/format";
import { Badge, Button, ProgressBar, useAsync } from "../../components/ui";

const RUNTIME_PACK = "piper_runtime";
const PYTHON_MB = 30;
const PYTORCH_MB = 200;

export function SetupWizard() {
  const open = useApp((s) => s.wizardOpen);
  const close = useApp((s) => s.openWizard);
  const packs = useApp((s) => s.packs);
  const dl = useApp((s) => s.download);
  const rt = useApp((s) => s.runtime);
  const rtp = useApp((s) => s.runtimeProgress);
  const settings = useApp((s) => s.settings);
  const voices = useApp((s) => s.voices);
  const status = useApp((s) => s.ttsStatus);
  const update = useApp((s) => s.update);
  const toast = useApp((s) => s.toast);
  const refreshModels = useApp((s) => s.refreshModels);
  const { startDownload, resumeDownload, pauseDownload, cancelDownload, installRuntimes, cancelRuntimeInstall } = useApp.getState();

  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [wantPython, setWantPython] = useState(false);
  const [wantTorch, setWantTorch] = useState(false);
  const [consent, setConsent] = useState(false);
  const [folderErr, setFolderErr] = useState<string | null>(null);
  const [forceSelect, setForceSelect] = useState(false);
  const seeded = useRef(false);
  const first = useRef<HTMLButtonElement>(null);

  // Preselect whatever is missing (once, when data first arrives).
  useEffect(() => {
    if (packs && rt && !seeded.current) {
      seeded.current = true;
      setSelected(new Set(packs.packs.filter((p) => (p.default_selected || p.id === RUNTIME_PACK) && !p.installed).map((p) => p.id)));
      setWantPython(!rt.python && rt.can_install_python);
      setWantTorch(!rt.torch);
    }
  }, [packs, rt]);

  const dlRunning = dl?.state === "running";
  const rtRunning = rtp?.state === "running";
  const busy = dlRunning || rtRunning;

  useEffect(() => {
    if (!open) return;
    first.current?.focus();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && !busy && close(false);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, close, busy]);

  const [start, starting] = useAsync(async () => {
    setForceSelect(false);
    const ids = [...selected];
    if (ids.length > 0) await startDownload(ids);
    if (wantPython || wantTorch) await installRuntimes(wantPython, wantTorch);
  });
  const [recheck, rechecking] = useAsync(async () => {
    await api.recheckRuntimes();
    await refreshModels();
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

  if (!open || !packs || !settings || !rt) return null;

  const dlShown = !!dl && dl.state !== "idle" && dl.files.length > 0 && !forceSelect;
  const rtShown = !!rtp && rtp.state !== "idle" && !forceSelect;
  const progressMode = dlShown || rtShown;

  const voicePacks = packs.packs.filter((p) => p.id !== RUNTIME_PACK);
  const piperRuntime = packs.packs.find((p) => p.id === RUNTIME_PACK);
  const totalMb =
    packs.packs.filter((p) => selected.has(p.id) && !p.installed).reduce((n, p) => n + p.approx_mb, 0) +
    (wantPython ? PYTHON_MB : 0) +
    (wantTorch ? PYTORCH_MB : 0);
  const nothingToDo = selected.size === 0 && !wantPython && !wantTorch;
  const needsPythonFirst = wantTorch && !rt.python && !wantPython;

  const pct = dl && dl.total > 0 ? (dl.received / dl.total) * 100 : 0;
  const failed = dl?.state === "failed" || rtp?.state === "failed";
  const allDone = progressMode && !busy && !failed && dl?.state !== "paused" && (dl?.state === "done" || !dlShown) && (rtp?.state === "done" || !rtShown);

  const engineVoices = voices[settings.tts.engine] ?? [];
  const currentVoice = engineVoices.find((v) => v.id === settings.tts.voice);
  const otherInstalled = (g: "male" | "female") => engineVoices.find((v) => v.installed && v.gender === g);

  const toggle = (id: string) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/60 p-4" role="presentation">
      <div role="dialog" aria-modal="true" aria-labelledby="wiz-title" className="max-h-full w-full max-w-2xl overflow-y-auto rounded-2xl border border-line bg-surface p-6 shadow-card">
        <div className="flex items-start justify-between gap-4">
          <div>
            <h2 id="wiz-title" className="text-xl font-semibold">Set up offline Russian voices</h2>
            <p className="mt-1 text-sm text-muted">Install the missing parts once. After that, speech is generated entirely on this computer — no internet needed.</p>
          </div>
          <button type="button" ref={first} aria-label="Close" disabled={busy} onClick={() => close(false)} className="rounded p-1 text-muted hover:text-fg disabled:opacity-40">
            <X className="h-5 w-5" />
          </button>
        </div>

        {status && status.state !== "ready" && !progressMode && (
          <p role="status" className="mt-4 rounded-lg border border-warn/40 bg-warn/10 p-3 text-sm text-warn">Offline TTS is not ready: {status.detail}</p>
        )}

        {!progressMode && (
          <div className="mt-5 space-y-5">
            <section>
              <h3 className="mb-2 text-sm font-semibold">Voices</h3>
              <ul className="space-y-2">
                {voicePacks.map((p) => (
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
            </section>

            <section>
              <h3 className="mb-2 text-sm font-semibold">Programs the voices run on</h3>
              <ul className="space-y-2">
                {piperRuntime && (
                  <li>
                    <label className={`flex items-start gap-3 rounded-lg border border-line p-3 ${piperRuntime.installed ? "opacity-60" : "cursor-pointer hover:border-muted/50"}`}>
                      <input type="checkbox" className="mt-1" disabled={piperRuntime.installed} checked={piperRuntime.installed || selected.has(RUNTIME_PACK)} onChange={() => toggle(RUNTIME_PACK)} />
                      <span className="min-w-0 flex-1">
                        <span className="block text-sm font-medium">Piper program</span>
                        <span className="block text-xs text-muted">Official release from github.com/rhasspy/piper. Needed for Piper voices.</span>
                      </span>
                      {piperRuntime.installed ? <Badge tone="ok">Installed</Badge> : <span className="text-sm tabular-nums text-muted">≈ {piperRuntime.approx_mb} MB</span>}
                    </label>
                  </li>
                )}
                <li>
                  <label className={`flex items-start gap-3 rounded-lg border border-line p-3 ${rt.python || !rt.can_install_python ? "opacity-70" : "cursor-pointer hover:border-muted/50"}`}>
                    <input type="checkbox" className="mt-1" disabled={rt.python || !rt.can_install_python} checked={rt.python || wantPython} onChange={(e) => setWantPython(e.target.checked)} />
                    <span className="min-w-0 flex-1">
                      <span className="block text-sm font-medium">Python 3.12 (for Silero)</span>
                      <span className="block text-xs text-muted">
                        {rt.python ? "Found on this computer." : rt.can_install_python ? "Installed for your user with winget (Windows Package Manager)." : "Not found. Install Python 3.10+ with your package manager, then press Re-check."}
                      </span>
                    </span>
                    {rt.python ? <Badge tone="ok">Installed</Badge> : rt.can_install_python ? <span className="text-sm tabular-nums text-muted">≈ {PYTHON_MB} MB</span> : <Badge tone="warn">Missing</Badge>}
                  </label>
                </li>
                <li>
                  <label className={`flex items-start gap-3 rounded-lg border border-line p-3 ${rt.torch ? "opacity-70" : "cursor-pointer hover:border-muted/50"}`}>
                    <input type="checkbox" className="mt-1" disabled={rt.torch} checked={rt.torch || wantTorch} onChange={(e) => setWantTorch(e.target.checked)} />
                    <span className="min-w-0 flex-1">
                      <span className="block text-sm font-medium">PyTorch, CPU build (for Silero)</span>
                      <span className="block text-xs text-muted">Installed into a private environment inside the models folder; your system Python is not modified.</span>
                    </span>
                    {rt.torch ? <Badge tone="ok">Installed</Badge> : <span className="text-sm tabular-nums text-muted">≈ {PYTORCH_MB} MB</span>}
                  </label>
                </li>
              </ul>
              {needsPythonFirst && <p role="alert" className="mt-2 text-sm text-bad">PyTorch needs Python. Install Python first, then press Re-check.</p>}
            </section>

            <dl className="space-y-1 rounded-lg bg-surface2 p-3 text-sm">
              <div className="flex justify-between gap-4"><dt className="text-muted">Total download</dt><dd className="tabular-nums">≈ {totalMb} MB</dd></div>
              <div className="flex justify-between gap-4"><dt className="shrink-0 text-muted">Stored in</dt><dd className="break-all text-right">{packs.models_dir}</dd></div>
              <div className="flex justify-between gap-4"><dt className="shrink-0 text-muted">Sources</dt><dd className="text-right">models.silero.ai, huggingface.co, github.com, pypi.org, download.pytorch.org{rt.can_install_python && wantPython ? ", winget" : ""}</dd></div>
            </dl>
            <p className="text-xs text-muted">Downloads are SHA-256 verified. Nothing is installed until you agree below, and nothing is sent anywhere except these download requests.</p>

            <label className="flex items-start gap-3 text-sm">
              <input type="checkbox" className="mt-1" checked={consent} onChange={(e) => setConsent(e.target.checked)} />
              I agree to download and install the selected items from the sources above.
            </label>

            <div className="flex flex-wrap items-center gap-2">
              <Button variant="primary" icon={<Download className="h-4 w-4" />} disabled={!consent || nothingToDo || needsPythonFirst} loading={starting} onClick={() => void start()}>
                Download &amp; install {totalMb > 0 ? `(≈ ${totalMb} MB)` : ""}
              </Button>
              <Button icon={<RotateCw className="h-4 w-4" />} loading={rechecking} onClick={() => void recheck()}>Re-check</Button>
              <Button icon={<FolderOpen className="h-4 w-4" />} loading={picking} onClick={() => void pickFolder()}>I already have models…</Button>
              <Button variant="ghost" onClick={() => close(false)}>Not now</Button>
            </div>
            {folderErr && <p role="alert" className="text-sm text-bad">{folderErr}</p>}
          </div>
        )}

        {progressMode && (
          <div className="mt-5 space-y-5" aria-live="polite">
            {dlShown && dl && (
              <section className="space-y-3">
                <div>
                  <div className="mb-1 flex justify-between text-sm">
                    <span className="font-medium">
                      {dl.state === "running" ? "Downloading…" : dl.state === "paused" ? "Download paused" : dl.state === "failed" ? "Download failed" : dl.state === "cancelled" ? "Download cancelled" : "Downloads complete"}
                    </span>
                    <span className="tabular-nums text-muted">{formatBytes(dl.received)} / {dl.total ? formatBytes(dl.total) : "?"}</span>
                  </div>
                  <ProgressBar value={dl.state === "done" ? 100 : pct} label="Overall progress" />
                  {dl.state === "running" && <p className="mt-1 text-xs tabular-nums text-muted">{formatBytes(dl.speed_bps)}/s · about {formatEta(Math.max(0, dl.total - dl.received), dl.speed_bps)} left</p>}
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
                  {dl.state === "failed" && <Button variant="primary" icon={<RotateCw className="h-4 w-4" />} onClick={() => void resumeDownload()}>Retry download</Button>}
                  {(dl.state === "running" || dl.state === "paused" || dl.state === "failed") && <Button variant="danger" onClick={() => void cancelDownload()}>Cancel download</Button>}
                </div>
              </section>
            )}

            {rtShown && rtp && (
              <section className="space-y-3">
                <div className="flex items-center justify-between text-sm">
                  <span className="font-medium">
                    {rtp.state === "running" ? (rtp.step === "python" ? "Installing Python…" : "Installing PyTorch…") : rtp.state === "failed" ? "Install failed" : rtp.state === "cancelled" ? "Install cancelled" : "Programs installed"}
                  </span>
                </div>
                {rtp.state === "running" && (
                  <>
                    <div className="h-2 w-full overflow-hidden rounded-full bg-line" role="progressbar" aria-label="Installing" aria-busy="true">
                      <div className="h-full w-1/3 animate-pulse rounded-full bg-accent" />
                    </div>
                    <p className="truncate text-xs text-muted" title={rtp.line}>{rtp.line || "Starting…"}</p>
                  </>
                )}
                {rtp.error && <p role="alert" className="rounded-lg border border-bad/40 bg-bad/10 p-3 text-sm text-bad">{rtp.error}</p>}
                <div className="flex flex-wrap gap-2">
                  {rtp.state === "running" && <Button variant="danger" onClick={() => void cancelRuntimeInstall()}>Cancel install</Button>}
                  {(rtp.state === "failed" || rtp.state === "cancelled") && (
                    <Button variant="primary" icon={<RotateCw className="h-4 w-4" />} loading={starting} onClick={() => void installRuntimes(wantPython, wantTorch)}>Retry install</Button>
                  )}
                </div>
              </section>
            )}

            {allDone && (
              <section className="space-y-3">
                <p className="flex items-center gap-2 font-medium text-ok"><CheckCircle2 className="h-5 w-5" aria-hidden /> Everything you selected is installed.</p>
                <p className="text-sm">
                  Current voice: <strong>{currentVoice?.name ?? settings.tts.voice}</strong> ({currentVoice?.gender ?? "—"}), {settings.tts.engine === "silero" ? "Silero" : "Piper"}.
                </p>
                {status && status.state !== "ready" && <p role="status" className="rounded-lg border border-warn/40 bg-warn/10 p-3 text-sm text-warn">{status.detail}</p>}
                <div className="flex flex-wrap gap-2">
                  {currentVoice?.gender !== "female" && otherInstalled("female") && (
                    <Button onClick={() => { const v = otherInstalled("female"); if (v) update((s) => void (s.tts.voice = v.id)); }}>Use female voice</Button>
                  )}
                  {currentVoice?.gender !== "male" && otherInstalled("male") && (
                    <Button onClick={() => { const v = otherInstalled("male"); if (v) update((s) => void (s.tts.voice = v.id)); }}>Switch to a male voice</Button>
                  )}
                  <Button variant="primary" onClick={() => close(false)}>Finish</Button>
                  <Button variant="ghost" onClick={() => setForceSelect(true)}>Add more</Button>
                </div>
              </section>
            )}

            {!busy && !allDone && (
              <div className="flex flex-wrap gap-2 border-t border-line pt-4">
                <Button icon={<FolderOpen className="h-4 w-4" />} loading={picking} onClick={() => void pickFolder()}>Choose existing models folder…</Button>
                <Button variant="ghost" onClick={() => setForceSelect(true)}>Back</Button>
                <Button variant="ghost" onClick={() => close(false)}>Close</Button>
              </div>
            )}
            {folderErr && <p role="alert" className="text-sm text-bad">{folderErr}</p>}
          </div>
        )}
      </div>
    </div>
  );
}
