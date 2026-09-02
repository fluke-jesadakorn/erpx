# ADR-010: Shared Dioxus target strategy

> **Deprecated — Archived (Dioxus, see backup/rust-2026-09-02). No longer authoritative on this branch.**
> See `CUTOVER.md` (2026-09-02) and `ADR-020-nextjs-stack.md`.


**Status:** Accepted  
**Decision:** One Rust/Dioxus UI crate owns shared navigation, domain components, state variants, localization, validation, and accessibility semantics. Thin target adapters provide Web/gRPC-Web, macOS/Windows/Linux desktop transport/storage, and Android/iOS mobile platform capabilities. Sensitive session material uses platform-secure storage; no target stores tokens in ordinary local storage or logs.

**Rationale:** Shared workflows reduce authorization and UX drift while allowing camera/scanner, notifications, deep links, file access, and offline drafts to remain target-specific.

**Consequences:** Target capability interfaces must be mockable for tests. Offline mode is limited to explicitly approved drafts with conflict recovery; every screen presents scope, read-only/support/impersonation, projection freshness, denied, empty, loading, conflict, and offline states.

**Evidence required:** web/desktop/mobile build checks, target secure-storage tests, WCAG 2.2 AA audit, Thai/English coverage, deep-link/file/notification tests, and required role journeys on each target.