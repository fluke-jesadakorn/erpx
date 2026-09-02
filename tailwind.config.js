/** @type {import('tailwindcss').Config} */
module.exports = {
  darkMode: ['selector', '[data-theme="dark"]'],
  content: [
    "./apps/console/src/**/*.{rs,html,css}",
    "./crates/ui-kit/src/**/*.{rs,css}",
    "./crates/console-shell/src/**/*.{rs,css}",
    "./crates/ui-domains/*/src/**/*.{rs,css}",
    "./crates/ui-core/src/**/*.{rs,css}",
  ],
  theme: {
    extend: {
      colors: {
        erpx: {
          bg: "var(--erpx-bg)",
          surface: "var(--erpx-surface)",
          raised: "var(--erpx-surface-raised)",
          border: "var(--erpx-border)",
          "border-strong": "var(--erpx-border-strong)",
          "border-subtle": "var(--erpx-border-subtle)",
          text: "var(--erpx-text)",
          muted: "var(--erpx-text-muted)",
          primary: "var(--erpx-primary)",
          "on-primary": "var(--erpx-on-primary)",
          accent: "var(--erpx-accent)",
          link: "var(--erpx-link)",
          focus: "var(--erpx-focus)",
          success: "var(--erpx-success)",
          warning: "var(--erpx-warning)",
          danger: "var(--erpx-danger)",
          security: "var(--erpx-security)",
        },
      },
      borderRadius: {
        sm: "var(--erpx-radius-sm)",
        md: "var(--erpx-radius-md)",
        lg: "var(--erpx-radius-lg)",
        xl: "var(--erpx-radius-xl)",
        pill: "var(--erpx-radius-pill)",
      },
      boxShadow: {
        sm: "var(--erpx-shadow-sm)",
        md: "var(--erpx-shadow-md)",
        lg: "var(--erpx-shadow-lg)",
      },
      fontFamily: {
        sans: ["Inter","Geist Sans","Noto Sans Thai","system-ui","sans-serif"],
        mono: ["Geist Mono","JetBrains Mono","ui-monospace","monospace"],
      },
      fontSize: {
        xs: "11px",
        sm: "12px",
        md: "14px",
        lg: "17px",
        xl: "28px",
      },
      spacing: {
        18: "4.5rem",
      },
      maxWidth: {
        content: "var(--erpx-content-max)",
      },
      zIndex: {
        nav: "20",
        topbar: "18",
        overlay: "60",
        toast: "70",
      },
      transitionDuration: {
        control: "140ms",
        surface: "220ms",
      },
    },
  },
  plugins: [],
  safelist: [
    "bg-erpx-bg","bg-erpx-surface","bg-erpx-raised","border-erpx-border","text-erpx-text","text-erpx-muted",
    "bg-erpx-primary","text-erpx-on-primary","border-erpx-primary","bg-erpx-primary/10",
  ],
};
