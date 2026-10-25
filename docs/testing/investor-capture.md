# Investor capture and report

Run `bun run investor:capture` with pinned Bun 1.2.17 and Node 24.20.0. It selects nine existing journeys across Connected ERP, Hotel/POS and AI/Collaboration, with 24 desktop and six mobile checkpoints. Existing assertions remain mandatory. The normal E2E command does not enable investor captures.

The command uses the existing isolated PostgreSQL browser runner on loopback port 3104. It refuses an occupied port and retains private case evidence before cleaning its owned fixtures. Development, private production, staging and Tunnel configuration are not changed. The runner loads only the restricted test administrator setting through the same mechanism as the normal test command.

The offline bundle is written under `.scratch/dev/investor/<run-id>/`: `index.html`, `report.html`, `brand.svg`, `images/` and `manifest.json`. Open `index.html` directly in a browser or open it in Codex. No remote assets, login or application server are required. Print either HTML page to save a PDF.

Run `bun run investor:report --from=<run-directory>` to regenerate without repeating business workflows. Missing, altered or duplicate images and failed/skipped cases make the presentation incomplete and the command exit unsuccessfully. Only images attached to exact passing cases with successful cleanup are included. Raw reports, logs, credentials and session files remain outside the presentation bundle.

Inspect every captured PNG. An optional `review.json` in the bundle records the matching `runtimeFingerprint` and `verificationFingerprint`, `reviewedImages` (PNG basenames), `strengths` (English observations), and `findings` (`severity`: `fixed` or `observation`, `title`, `detail`, `checkpoints`: checkpoint IDs). Regenerate afterward. A review for another revision is refused. Workflow success alone is not a completed visual assessment.

Fix confirmed bugs or presentation-blocking UI in the owning product component and strengthen the existing case's regression assertions. Capture the complete curated set again after tracked changes so the final bundle uses one verified source revision. Preserve existing worktree changes and use an isolated release build if runtime validation needs a build.

All captures show real product UI with fictional isolated Company data. The AI journey uses a deterministic adapter, and payments/bank imports use synthetic evidence. Preserve these limits in the report; do not describe the captures as live provider or production acceptance. Do not modify the DOM to fabricate a presentation state.

Investor fixtures attach the repository's existing generated menu photographs through the normal POS image service, including file scanning and authenticated thumbnails. Selected POS assertions require all six menu thumbnails to decode before capture. Ordinary browser runs do not enable this extra fixture preparation.

Selection and captions live in `scripts/testing/investor-selection.ts`. Checkpoints use the opt-in helper `tests/helpers/investor-capture.ts`, which retains PNGs through `captureCaseScreenshot`, checks viewport fit and attaches route/time/viewport metadata. The report collector copies only curated passing attachments into the offline bundle and records image checksums.
