# Inbox messaging

Inbox contains Chats, Teams, and the existing Updates notification view. Messaging uses the holding write database and the existing private local file store. Company membership comes from current assignments and real control-plane user accounts, not HR directory entries alone.

## HTTP contract

All endpoints are relative to `/api/v1/holdings/[holdingSlug]/messaging`, require an authenticated workspace, and return `{ data }` or the existing `{ error: { code, message } }` envelope. Responses are private and uncached. Mutations check Origin. POST mutations other than `/read` require an `Idempotency-Key`; reuse a key only with the exact original payload.

| Method / path                                    | Input or result                                                                                                   |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------- |
| GET `/`                                          | Teams and accessible rooms, each room's unread count and latest message preview                                   |
| GET `/people?q=…&team=…`                         | Up to 100 matching active company accounts; optional team restricts recipients                                    |
| POST `/rooms`                                    | `{ kind: "direct" \| "group", name, members: userId[] }`; returns room ID                                         |
| POST `/teams`                                    | `{ name, members }`; returns team ID and General `roomId`                                                         |
| POST `/teams/:id/channels`                       | `{ name, private: boolean, members }`; returns room ID                                                            |
| GET `/teams/:id/members`, `/rooms/:id/members`   | Current metadata, version, and member account names                                                               |
| POST `/teams/:id`, `/rooms/:id`                  | `{ name, members, expectedVersion }`; owner only; direct rooms cannot be edited                                   |
| GET `/rooms/:id/messages?before=N` or `?after=N` | `{ room, messages, hasMore }`; 50 messages, ascending sequence in responses; without a cursor returns latest page |
| POST `/rooms/:id/messages`                       | `{ body, files: fileId[] }`; returns message ID                                                                   |
| POST `/rooms/:id/read`                           | `{ seq }`; advances the user's read position monotonically, bounded by the room's last sequence                   |
| PUT `/rooms/:id/files/:uuid`                     | Binary body, Content-Type, percent-encoded `X-File-Name`; retry same UUID with identical bytes and metadata       |
| GET `/files/:uuid`                               | Authorized file content; images inline, documents downloaded                                                      |

Text is plain text, at most 5,000 characters. Messages must contain text or a file. Accept JPEG, PNG, WebP, PDF, CSV; at most ten files per message and 10 MiB per file. File reads use current room access, and unsent uploads are visible only to their author. Attachment binding occurs in the message transaction; files cannot be reused in another message or taken from another sender. Messaging remains the specialized CSV exception to the generic image/PDF policy, and every new file must pass the central private quarantine and malware-scan lifecycle before it can bind to a message.

## Staff Inbox events and notices (2026-09-26)

The Updates tab lists visible staff Inbox deliveries separately from Messages and Teams conversations. Each logical event has a stable key, source reference, category and safe preview; each recipient has independent read and archive state. Source access is checked for every page and unread count. A notification cannot grant source access or acknowledge a business action. The legacy `staff_notifications` API remains writable during cutover and its rows are backfilled and bridged into the new delivery model with read state preserved.

`GET .../inbox/notifications` supports unread/all, category, search and cursor paging. `GET .../inbox/summary` returns visible unread counts. `POST .../inbox/notifications/:id/state` accepts read and archive changes. `POST .../inbox/notices` sends immutable plain-text notices with an idempotency key to named active same-Company accounts or an authorized Company, Branch or Department audience. The sender sees accepted delivery count, never recipients’ read state. Broadcast fanout is bounded and retried by the scheduled worker. `GET .../inbox/notices/:id` shows a sender only its accepted and completed delivery counts. Existing manager and owner roles receive `inbox.notice.broadcast` within their current assignment scope.

All staff-facing producers should publish only meaningful assignments, decisions, returns, deadlines, escalations, mentions, meeting activity, access effects and participant-facing status changes. Ordinary field edits do not produce notifications. External email and push delivery are outside this contract.

## Access and consistency

- Direct rooms have a unique company/pair identity. Message retries use command receipts and database transactions; room sequences serialize simultaneous sends.
- Group creators manage membership. Team creators manage the team and channels. Owners always remain members. New members can read previous messages.
- Standard channels inherit team membership. Private channels require both team and channel membership. Removing a team member clears private-channel membership, so rejoining the team does not silently restore private access.
- Company administrators have no implicit chat access. Revoking company assignments removes access even when stored room membership remains.
- Clients poll visible conversations and the overview every two seconds, pause hidden tabs, and resume on visibility changes. Reading older messages does not automatically mark new messages read.
- No threaded replies, message editing/deletion, team deletion, owner transfer, self-service leave, typing indicators, calls, or external notifications in this version.

## Development verification

> Recorded observations apply to their stated date/environment. Current coverage and remaining acceptance are in the [verification inventory](../../status/capability-delivery-matrix.md); old gaps do not override current owner behavior.

Apply `0004_messaging.sql` through the existing holding-write migration runner, then grant application/jobs roles access to the new tables using the existing provisioning pattern. Do not edit older applied migrations. The migration was applied to the two local `erpx_dev_*_write` databases for the public dev server, not production databases.

Run `bun run test -- tests/messaging.test.ts tests/staff-migration.test.ts`, `bun run typecheck`, and `bun run lint`. For browser acceptance, run `bun --env-file=.env.local scripts/seed-messaging-e2e.ts`, then `ERPX_E2E_BASE_URL=https://console-dev-erpx.jesadakorn.com bunx playwright test tests/e2e/messaging.spec.ts`. The seed is restricted to local development databases, creates a separate synthetic company, and stores its generated password in ignored `.scratch/messaging/fixture.json` with mode 0600. Screenshots are saved alongside that fixture. Browser login checks reject redirects.

## Public dev routing correction (2026-09-09)

Public browser verification exposed two pre-existing Cloudflare redirect rules applying to every hostname in the zone. Windows agents were redirected to `gg.gg/Updates12` and Android agents to `momen.tm/xvBoRu`. Both rules now additionally require `http.host ne "erpx.jesadakorn.com"`; their behavior on other hostnames is unchanged. The tunnel continues to point at development port 3100.

Ruleset `5d2cf118423741d28cbc56e97bc38057`, rules `02150c8e57404873b031b44944e7d678` and `bc7a8870bb47450980f980a2bb36872a`. Original expressions were `(http.user_agent contains "Windows")` and `(http.user_agent contains "Android")`, respectively. Restoring those expressions would again redirect ERPX users and is not needed to roll back the messaging feature.
