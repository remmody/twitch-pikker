import { useEffect } from "react";
import { Bell, MessageSquare, Mic, Settings as SettingsIcon, type LucideIcon } from "lucide-react";
import { useApp } from "./stores/app";
import { useUi, type Tab } from "./stores/ui";
import { Header } from "./components/Header";
import { Toaster } from "./components/Toaster";
import { ChatView } from "./features/chat/ChatView";
import { VoiceView } from "./features/tts/VoiceView";
import { SetupWizard } from "./features/tts/SetupWizard";
import { NotificationsView } from "./features/notifications/NotificationsView";
import { SettingsView } from "./features/settings/SettingsView";

const TABS: { id: Tab; label: string; icon: LucideIcon }[] = [
  { id: "chat", label: "Chat", icon: MessageSquare },
  { id: "voice", label: "Voice", icon: Mic },
  { id: "notifications", label: "Alerts", icon: Bell },
  { id: "settings", label: "Settings", icon: SettingsIcon },
];

/** Applies the saved theme (and follows the OS when set to "system"). */
function useTheme() {
  const theme = useApp((s) => s.settings?.ui.theme ?? "dark");
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => document.documentElement.classList.toggle("dark", theme === "dark" || (theme === "system" && mq.matches));
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [theme]);
}

export default function App() {
  const init = useApp((s) => s.init);
  const ready = useApp((s) => s.ready);
  const tab = useUi((s) => s.tab);
  const setUi = useUi((s) => s.set);
  useTheme();

  useEffect(() => {
    void init();
  }, [init]);

  return (
    <div className="flex h-full">
      <nav aria-label="Sections" className="flex w-[84px] shrink-0 flex-col items-center gap-1 border-r border-line bg-surface py-4">
        {TABS.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            type="button"
            aria-current={tab === id ? "page" : undefined}
            onClick={() => setUi({ tab: id })}
            className={`flex w-16 flex-col items-center gap-1 rounded-xl px-1 py-2.5 text-xs font-medium transition ${tab === id ? "bg-accent/15 text-accent" : "text-muted hover:bg-surface2 hover:text-fg"}`}
          >
            <Icon className="h-5 w-5" aria-hidden />
            {label}
          </button>
        ))}
      </nav>

      <div className="flex min-w-0 flex-1 flex-col">
        <Header />
        <main className="min-h-0 flex-1 overflow-y-auto">
          {!ready ? (
            <div className="grid h-full place-items-center text-muted" role="status">Loading…</div>
          ) : tab === "chat" ? (
            <ChatView />
          ) : tab === "voice" ? (
            <VoiceView />
          ) : tab === "notifications" ? (
            <NotificationsView />
          ) : (
            <SettingsView />
          )}
        </main>
      </div>

      <SetupWizard />
      <Toaster />
    </div>
  );
}
