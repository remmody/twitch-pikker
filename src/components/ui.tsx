import { useId, useState, type ButtonHTMLAttributes, type ReactNode } from "react";
import { Loader2 } from "lucide-react";

const cx = (...c: (string | false | undefined)[]) => c.filter(Boolean).join(" ");

type Variant = "primary" | "secondary" | "ghost" | "danger";
const VARIANTS: Record<Variant, string> = {
  primary: "bg-accent text-accent-fg hover:brightness-110",
  secondary: "bg-surface2 text-fg border border-line hover:border-muted/50",
  ghost: "text-muted hover:text-fg hover:bg-surface2",
  danger: "bg-bad/15 text-bad border border-bad/40 hover:bg-bad/25",
};

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  loading?: boolean;
  icon?: ReactNode;
}

export function Button({ variant = "secondary", loading, icon, children, disabled, className, ...rest }: ButtonProps) {
  return (
    <button
      type="button"
      {...rest}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      className={cx(
        "inline-flex items-center justify-center gap-2 rounded-lg px-3.5 py-2 text-sm font-medium transition",
        "disabled:cursor-not-allowed disabled:opacity-50",
        VARIANTS[variant],
        className,
      )}
    >
      {loading ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden /> : icon}
      {children}
    </button>
  );
}

export function Card({ title, description, children, className }: { title?: string; description?: string; children: ReactNode; className?: string }) {
  return (
    <section className={cx("rounded-xl border border-line bg-surface p-5 shadow-card", className)}>
      {title && <h2 className="text-base font-semibold">{title}</h2>}
      {description && <p className="mt-1 text-sm text-muted">{description}</p>}
      <div className={cx((title || description) && "mt-4", "space-y-4")}>{children}</div>
    </section>
  );
}

export function Switch({ checked, onChange, label, hint, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: string; hint?: string; disabled?: boolean }) {
  return (
    <label className={cx("flex cursor-pointer items-center justify-between gap-4", disabled && "cursor-not-allowed opacity-50")}>
      <span>
        <span className="block text-sm font-medium">{label}</span>
        {hint && <span className="block text-xs text-muted">{hint}</span>}
      </span>
      <span className="relative inline-flex shrink-0">
        <input
          type="checkbox"
          role="switch"
          className="peer sr-only"
          checked={checked}
          disabled={disabled}
          onChange={(e) => onChange(e.target.checked)}
        />
        <span className="h-6 w-11 rounded-full bg-line transition peer-checked:bg-accent peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-accent" />
        <span className="pointer-events-none absolute left-0.5 top-0.5 h-5 w-5 rounded-full bg-white shadow transition peer-checked:translate-x-5" />
      </span>
    </label>
  );
}

export function Slider({ label, value, min, max, step = 1, onChange, format }: { label: string; value: number; min: number; max: number; step?: number; onChange: (v: number) => void; format?: (v: number) => string }) {
  const id = useId();
  return (
    <div>
      <div className="mb-1 flex items-baseline justify-between text-sm">
        <label htmlFor={id} className="font-medium">{label}</label>
        <span className="tabular-nums text-muted">{format ? format(value) : value}</span>
      </div>
      <input id={id} type="range" className="w-full" min={min} max={max} step={step} value={value} onChange={(e) => onChange(Number(e.target.value))} />
    </div>
  );
}

export function Field({ label, error, hint, children }: { label: string; error?: string | null; hint?: string; children: (props: { id: string; "aria-invalid": boolean; "aria-describedby"?: string }) => ReactNode }) {
  const id = useId();
  const msgId = `${id}-msg`;
  return (
    <div>
      <label htmlFor={id} className="mb-1 block text-sm font-medium">{label}</label>
      {children({ id, "aria-invalid": !!error, "aria-describedby": error || hint ? msgId : undefined })}
      {(error || hint) && (
        <p id={msgId} className={cx("mt-1 text-xs", error ? "text-bad" : "text-muted")} role={error ? "alert" : undefined}>
          {error ?? hint}
        </p>
      )}
    </div>
  );
}

export const inputCls =
  "w-full rounded-lg border border-line bg-surface2 px-3 py-2 text-sm placeholder:text-muted/70 aria-[invalid=true]:border-bad disabled:opacity-50";

export function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: { value: T; label: string }[]; onChange: (v: T) => void; label: string }) {
  return (
    <div role="radiogroup" aria-label={label} className="inline-flex rounded-lg border border-line bg-surface2 p-0.5">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          onClick={() => onChange(o.value)}
          className={cx(
            "rounded-md px-3 py-1.5 text-sm transition",
            value === o.value ? "bg-accent text-accent-fg font-medium" : "text-muted hover:text-fg",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Badge({ tone, children }: { tone: "ok" | "warn" | "bad" | "muted"; children: ReactNode }) {
  const t = { ok: "bg-ok/15 text-ok", warn: "bg-warn/15 text-warn", bad: "bg-bad/15 text-bad", muted: "bg-surface2 text-muted" }[tone];
  return <span className={cx("inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium", t)}>{children}</span>;
}

export function ProgressBar({ value, label }: { value: number; label: string }) {
  const pct = Math.max(0, Math.min(100, value));
  return (
    <div role="progressbar" aria-label={label} aria-valuenow={Math.round(pct)} aria-valuemin={0} aria-valuemax={100} className="h-2 w-full overflow-hidden rounded-full bg-line">
      <div className="h-full rounded-full bg-accent transition-[width] duration-200" style={{ width: `${pct}%` }} />
    </div>
  );
}

/** Wraps an async action: shows a spinner and blocks re-entry while it runs. */
export function useAsync<A extends unknown[]>(fn: (...a: A) => Promise<unknown>) {
  const [busy, setBusy] = useState(false);
  const run = async (...a: A) => {
    if (busy) return;
    setBusy(true);
    try {
      await fn(...a);
    } finally {
      setBusy(false);
    }
  };
  return [run, busy] as const;
}
