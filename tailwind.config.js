const c = (name) => `rgb(var(--${name}) / <alpha-value>)`;

/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        bg: c("bg"),
        surface: c("surface"),
        surface2: c("surface2"),
        line: c("line"),
        fg: c("fg"),
        muted: c("muted"),
        accent: c("accent"),
        "accent-fg": c("accent-fg"),
        ok: c("ok"),
        warn: c("warn"),
        bad: c("bad"),
      },
      boxShadow: { card: "0 1px 2px rgb(0 0 0 / 0.25), 0 8px 24px rgb(0 0 0 / 0.12)" },
      fontFamily: { sans: ["Inter", "Segoe UI", "system-ui", "-apple-system", "sans-serif"] },
    },
  },
  plugins: [],
};
