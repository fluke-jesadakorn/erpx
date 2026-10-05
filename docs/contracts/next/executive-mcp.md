# Executive remote MCP

This contract describes read-only access for a team-owned external agent. The agent brings its model and conversation UI; ERPX verifies a personal token and serves existing Executive reports and KPIs. The development endpoint is `https://console-dev-erpx.jesadakorn.com/api/mcp/executive`. Production rollout is separate; this change does not deploy or migrate production or staging.

## Create and revoke a connection

Open **Executive → External connections**. The right drawer lists your connections and offers a form when the selected work role has `executive.access` and satisfies existing executive eligibility: an active Level 1 or Level 2 engagement, or a Holding owner with `company.owner`. Emergency-session and public-beta access cannot create or use portable credentials.

Choose a name and Company, Branch or Department within the selected workspace. The selected assignment fixes the user, Holding, Company and role; the scope picker can narrow that workspace. Default lifetime is 30 days, selectable between 1 and 90 days, capped by assignment expiry. Copy the secret once into your agent's secret store. Only its SHA-256 hash persists in the control database. Closing the drawer clears the displayed secret.

The browser management endpoint is `/api/v1/holdings/{holdingSlug}/executive/connections`, including existing Branch/Department rewrites. GET lists your own metadata and allowed scopes. POST creates `{name, scopeId, expiresDays}` or revokes `{revokeId}`. Commands require a UUID `Idempotency-Key`, a normal browser session, work-context headers and matching Origin. A repeated creation key returns `CONNECTION_ALREADY_CREATED`; secrets cannot be replayed. If the response is lost, revoke the listed connection and create another. Revocation is idempotent and remains available to an active user who lost creation eligibility. Concurrent creation is serialized, with at most 20 unexpired, unrevoked connections per user and Company.

## Connect an agent

Send `Authorization: Bearer <token>` on every request. Tokens never belong in URLs, prompts or logs. The endpoint uses official TypeScript SDK v2, pinned at 2.3.0, with Streamable HTTP and a fresh server/principal per request. Current per-request MCP and stateless legacy requests are supported. There is no session ID, GET subscription stream or DELETE session operation. An Origin header, if present, must match the console origin.

The [team client example](../../../scripts/examples/executive-mcp-client.ts) discovers tools and lists dashboards and metrics:

```sh
# Inject ERPX_MCP_TOKEN from the agent's secret store first.
bun scripts/examples/executive-mcp-client.ts
```

Set `ERPX_MCP_URL` to override the development endpoint and `ERPX_MCP_METRIC` to optionally query one authorized metric. The example prints business results; use a private terminal.

```ts
const client = new Client({ name: "erpx-team-agent", version: "1.0.0" });
await client.connect(new StreamableHTTPClientTransport(new URL(endpoint), {
  authProvider: { token: async () => secret }
}));
const result = await client.callTool({
  name: "query_metric",
  arguments: {
    metric: "headcount",
    filters: { from: "2026-01-01", to: "2026-12-31", currency: "THB" },
    breakdown: "department", limit: 100
  }
});
```

Transport conventions follow the [official SDK web runtime guide](https://ts.sdk.modelcontextprotocol.io/v2/serving/web-standard-runtimes.html) and [per-request lifecycle](https://ts.sdk.modelcontextprotocol.io/v2/serving/sessions-state-scaling.html).

## Tools and output

| Tool | Arguments | Output |
| --- | --- | --- |
| `list_dashboards` | Optional `limit` (default 100, maximum 200), `cursor` | Accessible dashboard summaries, token scope and `nextCursor` |
| `get_dashboard` | `dashboardId` | Title, description, approved filters, widget metadata, report/share versions |
| `get_dashboard_data` | `dashboardId`, optional `filters`, `rowLimit` (default 100, maximum 200) | Metrics, previous-period comparisons, effective filters, revisions and share mode |
| `list_metrics` | Empty object | Existing catalogue, availability, acceptance reasons and supported breakdowns |
| `query_metric` | `metric`, `filters`, optional `breakdown`, `limit`, `cursor` | Authorized KPI and paginated grouped rows |

Schemas are strict. Tools do not accept SQL, Company, role, assignment or organization scope arguments. Chat history, recipient lists, publisher contexts, HTML/CSS widget source, browser documents and generation jobs are excluded. Source references describe datasets, fixed query IDs and revisions; source documents are not opened by this release.

Metric schema version 2 remains unchanged: exact totals and row values are decimal strings. Money uses minor units (`valueScale: 2`), percent uses hundredths of a percent, quantity uses thousandths and duration uses milliseconds. Preserve currency, formula, definition, scope boundary, status, source/query revisions, timestamps and source references. `valueAvailable: false` identifies unavailable or failed results: their existing contract total is a sentinel and must never be presented as measured zero. Partial, stale, empty and pending acceptance retain their original meaning. Catalogue metadata includes unavailable metrics with reasons; it does not grant permission to query them.

Recipient-mode shares use recipient source permissions. Distribution-mode shares read the approved widget/date projection through the publisher's existing authority. That projection may describe a publisher scope broader than the recipient token; use the metric's scope metadata. A share never grants additional KPI permissions. Requested shared dates can narrow the approved interval; requests outside that interval or currency retain the approved filters, as in Canvas.

Grouped rows include `{offset, returned, totalRows, truncated, nextCursor}`. Use the same metric, filters and breakdown with `nextCursor`; restart the analysis if the metric revision changes between pages. Dashboard data caps rows per metric and may trim them further to fit the size budget. Totals and provenance remain complete. Distribution recipients cannot query an otherwise forbidden metric to recover more groups.

## Authorization and limits

Every request resolves the active Holding, stored assignment/version and selected organization nodes, then loads current IAM and executive eligibility without a browser session. Revocation, expiry, suspended identities, directory freshness failure, removed eligibility, archived roles, revoked parent/directory assignments and assignment changes deny access. Assignment version changes invalidate existing tokens; current role permissions are loaded afresh. Authorization runs before business reads and again before releasing results. Report/share version and publisher authority checks remain in force; withdrawn shares or changed authority during a read discard the earlier result.

Requests allow 32 KiB and one JSON-RPC message. Structured business results allow 128 KiB; text and structured representations together have a bounded HTTP response. Trimmed rows are marked explicitly; oversized metadata returns `RESULT_TOO_LARGE`. PostgreSQL retains its existing 30-second statement timeout. The persisted rate limit allows 60 HTTP requests per minute per user and Company across all tokens, including discovery, with 429 and `Retry-After`.

Audit records store token ID, user, Company, assignment, Branch/Department, tool/action, start time, duration, success/denial/failure and a safe error code. Credentials, arguments and business payloads are excluded. Token issuance is audited. Failure to verify authority or persist the access audit fails closed. Missing/expired/revoked tokens return 401; IAM denials return 403; validation/tool failures use MCP error results; unavailable control/audit storage returns 503.

OAuth linking for ChatGPT/Claude, report editing/sharing, new KPIs and source-document access are outside this release.

## Verification

`tests/executive-mcp.test.ts` runs the official v2 client against the route handler backed by disposable PostgreSQL databases. It checks discovery, Canvas parity, exact large values, strict schemas, Company/Branch/Department isolation, recipient/distribution boundaries, live IAM and eligibility, withdrawals during reads, concurrency, limits, idempotency and storage failure. Run `bun run test -- tests/executive-mcp.test.ts tests/executive-canvas.test.ts`.

Browser and public-origin verification are separate from these integration tests. Development results do not certify production or staging rollout.
