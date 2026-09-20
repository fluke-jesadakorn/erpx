# AI workspace acceptance — 2026-09-18

## Implemented and verified

- Acme Corporation's selected custom connection is **OpenCode Go · Muse Spark 1.3**, model `muse-spark-1.3-contributor`, Responses protocol at `https://opencode.ai/zen/go/v1`. Text and image verification passed through the actual Next.js application; activation and default selection persisted after reload. Credentials remain encrypted server-side. No credential values are included in this report.
- A real floating assistant conversation returned a Thai response with source links using the selected Go connection. This is live inference, not the separate generic-record development suggestion implementation.
- Page-mode conversations are isolated by canonical page/record/filter context, user, selected assignment, company, Holding, branch/department and authority revisions. Cross-page mode requires `ai.context.cross`; it is not enabled by a numeric seniority level. Users explicitly retain additional page contexts. Both current and retained sources remain subject to domain authorization.
- Supported record sources: employee requests, expenses, staff tasks, published handbook articles, and an intentionally small AI-settings projection. Other supported pages supply workflow guidance only; they do not imply that the assistant can read all underlying module data.
- Historical answers whose source revisions or access changed are hidden. Authority and source snapshots are checked again after generation. Assistant output never executes a business action.
- `Learn` now presents workflow guides and published handbook knowledge with search, publication revisions and links into the owning workflows. Editors manage knowledge in the existing versioned handbook. There is no automatic model fine-tuning or learning from unapproved conversations.
- Assistant reservations count toward the shared monthly AI allowance, including checks from request, expense and social preparation.

## Verification

- 5 assistant integration tests passed: canonical page isolation, user/page history separation, cross-context denial, inference idempotency, and revocation during inference.
- 48 request AI, expense AI and provider integration tests passed.
- TypeScript and targeted ESLint passed.
- Isolated production release build passed; no deployment or switch of the public origin was performed.
- Public authenticated browser inspection: Go configuration, actual Thai answer, source links, page-mode navigation isolation, assistant at 390×844, and Learn at desktop/mobile widths. The mobile training page had no document horizontal overflow.
- Local and public `/api/health` returned HTTP 200. Existing development process on 3100 remained running; production service on 3101 was left intact.

## Remaining scope at the earlier checkpoint

Executive core implementation and English presentation changes were subsequently delivered; see [Executive acceptance](executive-canvas-2026-09-18.md). The paragraph below records the earlier checkpoint, not the current Executive status.

The larger requested executive analytics workspace is **not implemented by this increment**: saved/versioned custom HTML/CSS reports, governed cross-module metrics, refresh runs and interactive drill-down need their own implementation and acceptance. Generic `records.ts` suggestions still use their existing development implementation. Page-specific handbook mappings, enterprise-wide retrieval and all-module record adapters are also outstanding. The architecture research compares 44 patterns and describes these proposals; it is not evidence that those features already exist.
