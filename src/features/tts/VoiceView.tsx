import { useMemo, useState } from "react";
import { FolderOpen, Play, Search, Wand2 } from "lucide-react";
import { api } from "../../lib/tauri";
import { useApp } from "../../stores/app";
import { Badge, Button, Card, Field, inputCls, Segmented, Slider, Switch, useAsync } from "../../components/ui";
import type { Engine, Gender } from "../../lib/types";

type GenderFilter = "all" | Gender;

export function OfflineIndicator() {
  const st = useApp((s) => s.ttsStatus);
  if (!st) return <Badge tone="muted">Checking…</Badge>;
  const map = {
    ready: { tone: "ok", label: "Offline TTS: Ready" },
    model_missing: { tone: "bad", label: "Offline TTS: Model missing" },
    runtime_missing: { tone: "warn", label: "Offline TTS: Runtime missing" },
  } as const;
  const m = map[st.state];
  return <span title={st.detail}><Badge tone={m.tone}>{m.label}</Badge></span>;
}

export function VoiceView() {
  const settings = useApp((s) => s.settings);
  const update = useApp((s) => s.update);
  const voicesByEngine = useApp((s) => s.voices);
  const status = useApp((s) => s.ttsStatus);
  const toast = useApp((s) => s.toast);
  const openWizard = useApp((s) => s.openWizard);
  const refreshModels = useApp((s) => s.refreshModels);
  const ttsError = useApp((s) => s.ttsError);
  const [query, setQuery] = useState("");
  const [gender, setGender] = useState<GenderFilter>("all");
  const [sample, setSample] = useState("Привет! Это проверка голоса.");
  const [previewing, setPreviewing] = useState<string | null>(null);
  const [folderErr, setFolderErr] = useState<string | null>(null);

  const [speak, speaking] = useAsync(async (engine: Engine, voice: string, text: string) => {
    try {
      await api.testTts(text, engine, voice);
    } catch (e) {
      toast("error", e instanceof Error ? e.message : String(e));
    }
  });
  const [pickFolder, picking] = useAsync(async () => {
    try {
      const path = await api.pickTtsModel();
      if (!path) return;
      const v = await api.validateTtsModel(path);
      if (!v.ok) {
        setFolderErr(v.problems.join(" "));
        return;
      }
      setFolderErr(null);
      update((s) => void (s.tts.models_dir = path));
      setTimeout(() => void refreshModels(), 600); // after the debounced save lands
      toast("ok", `Using models from ${path}`);
    } catch (e) {
      toast("error", e instanceof Error ? e.message : String(e));
    }
  });

  const engine = settings?.tts.engine ?? "silero";
  const all = voicesByEngine[engine] ?? [];
  const voices = useMemo(
    () =>
      all.filter(
        (v) => (gender === "all" || v.gender === gender) && `${v.name} ${v.id}`.toLowerCase().includes(query.trim().toLowerCase()),
      ),
    [all, gender, query],
  );
  if (!settings) return null;
  const t = settings.tts;

  const chooseEngine = (e: Engine) =>
    update((s) => {
      s.tts.engine = e;
      const list = voicesByEngine[e] ?? [];
      const pick = list.find((v) => v.installed && v.gender === "female") ?? list.find((v) => v.installed) ?? list[0];
      if (pick) s.tts.voice = pick.id;
    });

  const preview = async (id: string) => {
    setPreviewing(id);
    await speak(engine, id, sample);
    setPreviewing(null);
  };

  return (
    <div className="mx-auto max-w-4xl space-y-6 p-6">
      <Card title="Offline voices" description="Speech is synthesized on this computer. No chat text leaves it.">
        <div className="flex flex-wrap items-center gap-3">
          <Segmented<Engine> label="TTS engine" value={engine} onChange={chooseEngine} options={[{ value: "silero", label: "Silero" }, { value: "piper", label: "Piper" }]} />
          <OfflineIndicator />
          {status && status.state !== "ready" && <span className="text-sm text-muted">{status.detail}</span>}
        </div>
        {ttsError && <p role="alert" className="rounded-lg border border-bad/40 bg-bad/10 p-3 text-sm text-bad">{ttsError}</p>}
        <div className="flex flex-wrap gap-2">
          <Button variant="primary" icon={<Wand2 className="h-4 w-4" />} onClick={() => openWizard(true)}>Set up offline voices…</Button>
          <Button icon={<FolderOpen className="h-4 w-4" />} onClick={() => void pickFolder()} loading={picking}>Choose models folder…</Button>
          <Button variant="ghost" onClick={() => void api.openModelsFolder().catch((e: Error) => toast("error", e.message))}>Open models folder</Button>
        </div>
        {folderErr && <p role="alert" className="text-sm text-bad">{folderErr}</p>}
        {t.models_dir && <p className="text-xs text-muted">Custom folder: {t.models_dir}</p>}
      </Card>

      <Card title="Voice">
        <div className="flex flex-wrap items-center gap-3">
          <div className="relative min-w-52 flex-1">
            <Search className="pointer-events-none absolute left-3 top-2.5 h-4 w-4 text-muted" aria-hidden />
            <input aria-label="Search voices" className={`${inputCls} pl-9`} placeholder="Search voices" value={query} onChange={(e) => setQuery(e.target.value)} />
          </div>
          <Segmented<GenderFilter> label="Gender" value={gender} onChange={setGender} options={[{ value: "all", label: "All" }, { value: "female", label: "Female" }, { value: "male", label: "Male" }]} />
        </div>

        <ul className="divide-y divide-line overflow-hidden rounded-lg border border-line" role="radiogroup" aria-label="Voices">
          {voices.length === 0 && <li className="p-4 text-sm text-muted">No voices match.</li>}
          {voices.map((v) => {
            const selected = t.voice === v.id;
            return (
              <li key={v.id} className={`flex flex-wrap items-center gap-3 p-3 ${selected ? "bg-accent/10" : ""}`}>
                <button
                  type="button"
                  role="radio"
                  aria-checked={selected}
                  onClick={() => update((s) => void (s.tts.voice = v.id))}
                  className="flex min-w-0 flex-1 items-center gap-3 text-left"
                >
                  <span className={`grid h-5 w-5 shrink-0 place-items-center rounded-full border-2 ${selected ? "border-accent" : "border-line"}`}>
                    {selected && <span className="h-2.5 w-2.5 rounded-full bg-accent" />}
                  </span>
                  <span className="min-w-0">
                    <span className="block truncate font-medium">{v.name}</span>
                    <span className="block truncate text-xs text-muted">{v.id}</span>
                  </span>
                </button>
                <Badge tone="muted">{v.language === "ru" ? "Russian" : v.language}</Badge>
                <Badge tone="muted">{v.gender === "female" ? "Female" : v.gender === "male" ? "Male" : "Unknown"}</Badge>
                <Badge tone={v.installed ? "ok" : "bad"}>{v.installed ? "Installed" : "Missing"}</Badge>
                <Button disabled={!v.installed || speaking} loading={previewing === v.id && speaking} onClick={() => void preview(v.id)} icon={<Play className="h-4 w-4" />} aria-label={`Preview ${v.name}`}>
                  Preview
                </Button>
              </li>
            );
          })}
        </ul>
      </Card>

      <Card title="Speech">
        <div className="grid gap-x-8 gap-y-5 md:grid-cols-2">
          <Slider label="Speed" value={t.speed} min={0.7} max={1.5} step={0.05} format={(v) => `${v.toFixed(2)}×`} onChange={(v) => update((s) => void (s.tts.speed = v))} />
          <Slider label="TTS volume" value={t.volume} min={0} max={100} format={(v) => `${v}%`} onChange={(v) => update((s) => void (s.tts.volume = v))} />
          <Field label="Maximum spoken message length" hint="Characters; longer messages are cut." error={t.max_message_chars < 1 || t.max_message_chars > 1000 ? "Enter a number from 1 to 1000." : null}>
            {(p) => (
              <input {...p} type="number" min={1} max={1000} className={inputCls} value={t.max_message_chars}
                onChange={(e) => update((s) => void (s.tts.max_message_chars = Math.max(1, Math.min(1000, Number(e.target.value) || 1))))} />
            )}
          </Field>
          <Field label="Username template" hint="Use {username} where the name goes. Leave empty to say just the message." error={t.announce_username && t.username_template.trim() !== "" && !t.username_template.includes("{username}") ? "Template has no {username}; every message would start with the same words." : null}>
            {(p) => (
              <input {...p} className={inputCls} disabled={!t.announce_username} value={t.username_template}
                onChange={(e) => update((s) => void (s.tts.username_template = e.target.value))} />
            )}
          </Field>
        </div>
        <div className="grid gap-4 md:grid-cols-3">
          <Switch label="Announce username" checked={t.announce_username} onChange={(v) => update((s) => void (s.tts.announce_username = v))} />
          <Switch label="Read emotes" hint="Off: emotes are skipped" checked={t.read_emotes} onChange={(v) => update((s) => void (s.tts.read_emotes = v))} />
          <Switch label="Read links" hint="Off: says “link”" checked={t.read_links} onChange={(v) => update((s) => void (s.tts.read_links = v))} />
        </div>
        <div className="flex flex-wrap items-end gap-3 border-t border-line pt-4">
          <div className="min-w-64 flex-1">
            <Field label="Test text">{(p) => <input {...p} maxLength={300} className={inputCls} value={sample} onChange={(e) => setSample(e.target.value)} />}</Field>
          </div>
          <Button variant="primary" icon={<Play className="h-4 w-4" />} loading={speaking && previewing === null} disabled={speaking || !sample.trim()} onClick={() => void speak(engine, t.voice, sample)}>
            Test voice
          </Button>
        </div>
      </Card>
    </div>
  );
}
