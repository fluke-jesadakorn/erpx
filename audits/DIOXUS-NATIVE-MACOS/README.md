# Dioxus Native macOS evidence

This directory retains release evidence for the Web/macOS renderer cutover. The
tooling only reports measurements that were captured on the named hardware and
revision; a template or an absent file is never treated as measured evidence.

Create `evidence.json` from `evidence.template.json`, replace every
`not_captured` status with either `passed`, `failed`, or `blocked`, and attach
artifacts below `artifacts/<release-revision>/`. Screenshots must cover English
and Thai, light and dark themes, and the sign-in, shell, dialog/drawer, error,
retry, and 10,000-row virtualized-table states.

The evidence record contains:

- cold-start and input-to-paint durations in milliseconds;
- p95 frame duration and observed refresh rate;
- working-set memory and the separately captured WebView baseline;
- keyboard, focus restoration, and VoiceOver/AccessKit results;
- artifact paths and SHA-256 digests for screenshots and raw measurements.

Never store credentials, access or refresh tokens, cookies, authorization
headers, Keychain payloads, private URLs, customer data, or developer home
paths. Use repository-relative artifact paths and redact user/account labels in
screenshots and accessibility transcripts.

`cargo xtask verify-platforms --macos` performs the architecture scan, native
build, and bounded launch smoke. Manual performance and accessibility evidence
remains explicitly uncaptured until `evidence.json` exists; the command does
not infer performance or VoiceOver success from a successful launch.

`webview-baseline.json` retains the partial baseline captured from revision
`47b0f55`. It contains observed memory data, while cold start, frame time, and
input-to-paint remain `null` because they were not reliably instrumented.
