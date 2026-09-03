# Designed — Pencil Canvas

> Edit only via **Pencil MCP** (see `AGENTS.md`). Do not `grep` or `Read` any `.pen` directly. After canvas work, press **⌘S in Pen** (MCP edits live in the app model; disk flush + `git commit` need the save).

- **Canvas:** `MainDesign.pen` — 15M, v2.17, 894 top frames / 32,990 nodes (272 `Page ·` + 204 `Action ·` + 205 `Decision ·` + 230 `Outcome ·`), 1120px columns at `x = 600 / 1820 / 3040 / 4260`.
- **Images:** `images/` — 27 PNGs (16M total, `generated-17866*.png`, 450–819K each). List:
  - `generated-1786639650115.png`, `generated-1786639680333.png`, `generated-1786639697356.png`, `generated-1786639702211.png`, `generated-1786639714816.png`, `generated-1786640441134.png`, `generated-1786640446031.png`, `generated-1786640454887.png`, `generated-1786640461277.png`, `generated-1786640566514.png`, `generated-1786641405266.png`, `generated-1786641429880.png`, `generated-1786641434376.png`, `generated-1786641548026.png`, `generated-1786641548027.png`, `generated-1786642210503.png`, `generated-1786642219976.png`, `generated-1786642237969.png`, `generated-1786642256774.png`, `generated-1786642262853.png`, `generated-1786642303910.png`, `generated-1786642317392.png`, `generated-1786642772661.png`, `generated-1786642789860.png`, `generated-1786642809317.png`, `generated-1786642869485.png`, `generated-1786642928563.png`
- **Routes (representative):** `/work-queue`, `/work` + `/sales` (`/sales`, `/sales/:id` — sales quotes/orders, see `docs/plan/02-capabilities/03-finance-sales-pos/sales.md` + `docs/plan/03-cross-cutting/@-and-ai/sales.md` + `docs/plan/MAP.md`), `/billing/*`, `/platform/*`, `/platform/holdings`, holding detail, plus 31 business record pages derived from `docs/plan/02-capabilities/` contexts — see `Designed/AGENTS.md` for URL hierarchy and naming (`Page · (/url)`) + `docs/TOC.md` Sales Quick Jump.
- **Link to spec:** business spine is in `docs/plan/README.md` + `docs/plan/02-capabilities/` + `docs/plan/03-cross-cutting/@-and-ai/`; each page header shows its canonical URL and owner — match it to the Plan spine.
- **Guardrails:** `AGENTS.md` — Pencil-only access, no filesystem grep, English-only route/decision/outcome names, 160px indentation, 1120px frame width.
- **Recovery:** full canvas is on `backup/rust-2026-09-02` (`rust-archive-2026-09-02`, `f97b74d6`) — this branch keeps the same file.

## ERPX-System.pen — fresh shadcn system (in progress, 2026-09-03)

Parallel rebuild from `@docs/` (31 contexts, `docs/plan/MAP.md`), shadcn base + Tailwind v4 + Lucide, responsive **1440 desktop + 390 mobile** (device themes `desktop/mobile`, light/dark). `MainDesign.pen` untouched.

- **Tokens (29):** `--background/foreground/card/popover/primary(#6758E5 violet)/primary-foreground/secondary/muted/muted-foreground/accent/destructive/border/input/ring/success/warning/info/radius(10)/font-sans(Inter)/font-mono/font-thai/space-xs–xl/page-gutter(28/16)/page-gap(18/14)` → Tailwind: `:root{--color-background:…}` + `@layer base .font-sans{font-family:Inter…}` (see Tailwind guide; `src/app/globals.css` is the target).
- **Components (21, Lucide):** 5 Buttons · 4 Badges · Input Field · Metric Card · Nav Item · Sidebar · Navbar · Card · Table Cell/Row (spec masters) · Reference Chip (`@ctx/id`, mono) · Scope Badge · Mobile Status/Tab Bar.
- **Pages (5, verified unless noted):** `Page · Dashboard (/dashboard)` 1440 ✅ screenshot · Dashboard Mobile ✅ screenshot · `Journals (/finance/journals)` + `Journal Detail (/finance/journals/:id)` + `Quotes (/sales/quotes)` — structure-verified, **render pending app restart** (see below).
- **Patterns:** 1-level `ref` nesting max; bespoke table rows/cells (no instance-slot ops); `placeholder` cleared per frame; verify with depth-0 listings (`c.depth===0` = tops).
- **Deviations from `AGENTS.md`:** 1440/390 grid (not 1120) for responsive; Action/Decision/Outcome trees deferred to key flows (sales-approve, finance-close, pos-refund) after 31 list+detail pairs land.
- **Queue (docs refs):** finance mobile → sales detail+mobile → pos → crm/support → remaining 27 contexts (`docs/plan/02-capabilities/*`, deep deltas in `03-cross-cutting/@-and-ai/*`, routes `docs/contracts/next/route-table.md`).
- **Known env issue:** Pen crashed 2026-09-03 (bridge timeouts, file confusion); post-restart renderer paints old nodes but not new ones — restart Pen + ⌘S, then re-export a new page before resuming. Checkpoint `d2b030e7` holds tokens+components; pages await next commit after save.
