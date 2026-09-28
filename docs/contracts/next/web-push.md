# Web Push delivery (opt-in)

Web Push is an optional delivery channel for existing `inbox_events` and `inbox_deliveries`. It does not create or change a business event, task, approval, or Inbox read state. Migration `migrations/postgres/holding-write/0328_web_push.sql` adds per-company/user preferences, session-bound browser subscriptions, and retryable delivery jobs. Apply it to each holding-write database before enabling Push in an environment. The app and jobs roles receive access through that migration.

Configure these four values in **both web and jobs processes**:

| Variable | Value |
| --- | --- |
| `ERPX_PUSH_VAPID_PUBLIC_KEY` | URL-safe base64 public P-256 key (uncompressed, 65 bytes). |
| `ERPX_PUSH_VAPID_PRIVATE_KEY` | Matching URL-safe base64 private P-256 key (32 bytes); secret. |
| `ERPX_PUSH_VAPID_SUBJECT` | `mailto:` operations contact or an HTTPS URL. |
| `ERPX_PUSH_DATA_KEY` | Stable URL-safe base64 random 32-byte key for AES-GCM encryption of stored subscription endpoints and browser keys; secret. |

Generate the VAPID pair once with `bunx web-push generate-vapid-keys`; generate the data key separately from 32 cryptographically random bytes. Preserve both private keys across releases. Rotating VAPID requires users to subscribe again. Rotating the data key requires clearing existing `push_subscriptions` first; otherwise queued sends cannot decrypt them. Never put either private key in a client bundle or the repository. With any key absent, malformed, or mismatched, the Push API reports unavailable, scheduled delivery skips its tables, and the in-app Inbox continues to work.

An employee opens **Account & work role → Notification settings** on the Console origin. Only the explicit **Turn on for this browser** button asks browser permission and subscribes. The personal settings page is behind current work-context authorization. Categories default to Work, Mentions, and Important system updates; Calendar and Notices are off. Quiet hours use Asia/Bangkok time. The Settings route and service worker are origin-specific, so a Console subscription cannot be reused on the configured POS or PMS hosts. Front origin endpoints are allowed for separate opt-in, but the Front shells do not yet expose a personal Settings link. Push also depends on secure-context browser support; iOS availability depends on the browser's installed-app support.

The jobs process scans current Inbox deliveries after subscription, enqueues each `(delivery, subscription)` once, and checks unread state, category preference, active work assignment, unrevoked session, and quiet hours again before sending. Retries use a lease and exponential delay; HTTP 404/410 removes an expired subscription. Push providers are at-least-once, so a provider timeout may cause a retry, while the service worker uses the stable job ID as the notification tag to replace duplicates on a device. The lock-screen title/body are always generic: **ERPX — You have a work update.** The encrypted payload contains only that tag and a same-origin Inbox path. Inbox detail is fetched after authentication and rechecked against source access when opened. Account and cross-company role switches remove the active browser's subscription before changing context. Logout attempts cleanup and invalidates the session; session/assignment revocation also stops jobs. Push delivery errors never block the scheduled Inbox or other work.

The service worker does not cache business data or handle fetches. No browser permission prompt should be used as an automated acceptance test. Test the disabled UI and in-app Inbox without VAPID keys; test live Push manually on each supported origin/browser only after migration and configuration are approved.
