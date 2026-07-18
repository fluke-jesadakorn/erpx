# Executive Crystal design system

Folio uses one authoritative token layer in `app/globals.tokens.css` and one implementation layer in `app/globals.css`. Dark mode is the signature theme; `data-theme="light"` is a fully designed counterpart. Legacy `paper`, `ink`, and semantic aliases remain supported while routes migrate.

## Foundations

- Typography: Geist for UI and display, Geist Mono for identifiers and numbers, and Noto Sans Thai for Thai shaping.
- Glass: `panel`, `panel-elevated`, `panel-floating`, and `panel-interactive` are the four shared elevation recipes. Use `glass-toolbar`, `glass-input`, and `glass-chip` for their named roles.
- Layout: `PageLayout` supports `standard`, `wide`, and `full`. Finance tables and multi-column workspaces should use `wide`; immersive chat and dense matrices may use `full`.
- Color: use semantic tokens such as `accent`, `positive`, `caution`, `critical`, `info`, and `neutral`. Never use color without an icon or text label for status.
- Motion: transitions stay between 140–220 ms and hover movement stays within 1–2 px. Reduced-motion disables decorative animation.

## Component rules

Use primitives from `components/ui` before styling a new surface. `Panel`, `Button`, `Input`, `Textarea`, `Select`, `Tabs`, `Badge`, `Status`, `Kpi`, `Alert`, `Table`, `Modal`, `Toast`, `Tooltip`, `Skeleton`, and `Empty` carry theme, focus, fallback, and density behavior.

Overlays render through the centralized portal layers. Modal content must have an accessible title, initial focus must be predictable, and destructive actions must remain visually and textually explicit. Toasts are mounted once in the root layout.

Tables should keep labels compact, values tabular, and long translated content allowed to wrap. Use sticky headers only when the table is inside a bounded scrolling workspace.

## Accessibility

- Maintain WCAG AA contrast on translucent and opaque fallback surfaces.
- Preserve `:focus-visible`; do not replace it with hover-only feedback.
- Keep interactive targets at least 36 px on desktop and 44 px for primary mobile actions.
- Support English, Thai, and German without fixed text widths. Thai content uses loose line breaking and overflow wrapping.
- Labels, icons, loading, empty, error, locked, disabled, rejected, and completed states must remain explicit.

The admin-gated `/design-system` route demonstrates the production primitives and should be checked in both themes after token changes.
