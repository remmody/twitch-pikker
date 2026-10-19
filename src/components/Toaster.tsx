import { CheckCircle2, Info, X, XCircle } from "lucide-react";
import { useApp } from "../stores/app";

const ICON = { ok: CheckCircle2, error: XCircle, info: Info } as const;
const TONE = { ok: "text-ok", error: "text-bad", info: "text-accent" } as const;

export function Toaster() {
  const toasts = useApp((s) => s.toasts);
  const dismiss = useApp((s) => s.dismissToast);
  return (
    <div aria-live="polite" className="pointer-events-none fixed bottom-4 right-4 z-50 flex w-96 max-w-[calc(100vw-2rem)] flex-col gap-2">
      {toasts.map((t) => {
        const Icon = ICON[t.kind];
        return (
          <div key={t.id} role={t.kind === "error" ? "alert" : "status"} className="pointer-events-auto flex items-start gap-3 rounded-xl border border-line bg-surface p-3 shadow-card">
            <Icon className={`mt-0.5 h-5 w-5 shrink-0 ${TONE[t.kind]}`} aria-hidden />
            <p className="flex-1 break-words text-sm">{t.text}</p>
            <button type="button" onClick={() => dismiss(t.id)} aria-label="Dismiss" className="rounded p-0.5 text-muted hover:text-fg">
              <X className="h-4 w-4" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
