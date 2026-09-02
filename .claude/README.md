# .claude — local (not canonical for skills)

Skills are canonical at `.agents/skills/` (see `AGENTS.md`, `Designed/README.md`, `docs/TOC.md`).
The previous mirror at `.claude/skills/` was removed on 2026-09-02 (dedup — `.agents` ≡ `.claude` 100%) to avoid drift.

- Source of truth: `.agents/skills/` (6 skills: domain-modeling, grilling, prototype, research, wayfinder, setup-matt-pocock-skills) + `skills-lock.json`.
- This directory is kept for Claude Code local state only and is not tracked for skills. If your tool recreates `.claude/skills/`, treat it as a transient mirror.

Recovery: full history on `backup/rust-2026-09-02`.
