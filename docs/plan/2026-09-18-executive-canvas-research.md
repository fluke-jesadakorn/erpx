# Executive AI live report canvas: research findings

Research date: 2026-09-18. Status: implementation-planning input; this document does not claim the feature exists.

Scope: private saved reports, generated widgets added from chat, live authorized data, and named sharing with executives or employees.
Source facts below describe browser standards and official documentation. Recommendations are proposed ERPX design choices, not guarantees supplied by those standards.

## 1. Separate generated presentation from trusted data access

| Source fact | ERPX recommendation and tradeoff |
| --- | --- |
| Without `allow-same-origin`, a sandboxed iframe has an opaque origin. Combining `allow-scripts` and `allow-same-origin` for same-origin content can undermine the sandbox. [MDN iframe](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/iframe) | Render generated content outside the application DOM, preferably from a dedicated artifact origin, with the smallest sandbox permissions. Arbitrary generated JS must never receive session cookies, bearer tokens, provider keys, or database access. |
| `srcdoc` uses the embedding document's URL as its base for relative URLs. [MDN iframe](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/iframe) | Treat `srcdoc` as a rendering mechanism, not a data or network security boundary. An isolated host with application-controlled response headers makes the policy easier to enforce and test. |
| CSP's weakest permitted resource destination can become an exfiltration channel. [W3C CSP security considerations](https://w3c.github.io/webappsec-csp/#security-exfiltration) | Start with `default-src 'none'`; narrowly allow required scripts/styles and controlled assets. Generated HTML/CSS also needs controls because images, fonts, and CSS URLs can make requests. The host, not the generated document, owns the effective policy. |
| `connect-src` covers interfaces including fetch, XHR, WebSocket, EventSource, and sendBeacon. It is not a universal outbound-network switch. [MDN connect-src](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/connect-src) | Use `connect-src 'none'` inside widgets. Also constrain image/font/media/frame/object sources, forms, popups, and navigation. Do not describe this directive alone as complete network isolation. |
| The sandbox navigation flag does not forbid a frame from navigating itself. [WHATWG sandboxing](https://html.spec.whatwg.org/multipage/browsers.html#sandboxing) | Test self-navigation explicitly. Combine a dedicated origin, parent frame restrictions, restrictive child CSP, and bridge revocation on navigation; do not assume `sandbox="allow-scripts"` proves no egress. |

**Recommended data path:** viewer session -> application gateway -> authorized semantic query -> bounded result -> widget bridge -> isolated renderer.

- The gateway resolves tenant, viewer, report access, row/member permissions, allowed metric bindings, filter types, result limits, and query budgets on the server.
- Widgets request approved binding IDs with validated filter changes. They cannot supply arbitrary SQL, URLs, headers, actions, or a substitute user identity.
- Prefer metric definitions with explicit grain, joins, date basis, currency, and aggregation rules. A language model proposes a binding; trusted application code validates and executes it.
- Distinguish rendering freedom from data-access authority: free-form charts and local interactions do not require free-form database access.

The authorization model above is a recommendation. As a primary implementation example, Cube separates authentication from authorization and applies row/member policies using security context; this is a design reference, not a requirement to adopt Cube. [Cube access control](https://docs.cube.dev/docs/data-modeling/access-control)

## 2. Give each widget a narrow, revocable bridge

**Source fact:** cross-document messages require sender validation and payload validation; a window can navigate after a message is sent. Exact target origins should be used where available. [MDN postMessage](https://developer.mozilla.org/en-US/docs/Web/API/Window/postMessage)

**Recommendations:**

- Bind communication to the expected frame instance and current document lifecycle. Validate source window, origin where meaningful, protocol version, message type, payload schema, and size.
- Opaque sandbox origins commonly serialize as `null`; that value is not an identity check. Use a carefully bounded handshake and a dedicated MessagePort, and revoke the channel when the frame reloads, navigates, or is removed.
- Keep any wildcard handshake free of business data. Do not continuously broadcast datasets through wildcard window messages.
- Permit only narrow operations such as requesting a binding refresh, reporting size, or requesting a parent-controlled record link. Share, export, permissions, and save controls remain in the trusted application.
- Apply response limits, rate limits, and lifecycle cancellation. Browser isolation alone does not establish reliable CPU/memory quotas for arbitrary generated JS.

Message ports support an object-capability communication model, but the application must define which authority each port grants. [WHATWG ports and capabilities](https://html.spec.whatwg.org/multipage/web-messaging.html#ports-as-the-basis-of-an-object-capability-model-on-the-web)

## 3. Save report definitions and enforce recipient permissions

The following are design recommendations derived from the separation of authentication and authorization above. They describe recipient-access sharing. The [product plan](2026-09-18-executive-live-canvas.md) additionally proposes explicitly authorized distribution contracts for sharing bounded live outputs with employees; such a contract must grant only those outputs, never inherit the creator's general credentials or authority.

- New reports are private. Store owner, tenant, title, layout, widget versions, data bindings, and named viewer/editor grants separately from chat history.
- Sharing a report grants access to its definition, not the creator's business-data privileges. Resolve each recipient's current permissions on open, refresh, export, and live delivery.
- Apply the same rule to executives and employees; a job title alone must not imply unrestricted access.
- Cache results by tenant, effective authorization scope, binding version, and filters. Invalidate or deny stale scopes after permission changes.
- Generated text, HTML, labels, hidden elements, and saved snapshots may already contain creator-visible facts. Rebinding live data cannot remove those facts automatically. Separate reusable presentation from data; authorize embedded content before sharing or regenerate it for the recipient.
- Show `dataAsOf`, refresh time, metric definition/version, applied filters, source drill-down, and query/result ID. A connected stream does not prove the underlying data is current.
- Preserve narrative provenance: an AI explanation should identify the result revision it describes and become visibly stale when that result changes.

## 4. Dragging must have equivalent non-drag interactions

**Source fact:** WCAG 2.2 SC 2.5.7 requires a single-pointer alternative to dragging unless an exception applies. Keyboard support alone does not satisfy that criterion. [W3C dragging movements](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements.html)

**Recommendations:**

- Every chat widget offers an explicit **Add to report** button and a destination chooser, alongside drag-to-canvas.
- Each canvas widget has click/tap controls for **Move before/after**, **Move to section**, resize presets, and remove; keyboard users can operate the same controls.
- The parent application owns drag handles and position controls so iframe content cannot interfere with report operations.
- Preserve focus after movement, announce the completed action, and keep reading order consistent with the saved layout. Provide meaningful iframe titles and text/table alternatives for important charts.
- Offer **Pause live updates** and **Refresh now**; retain focus and selection during refresh. Automatically updating parallel content can require a pause/stop/hide or frequency control. [W3C pause, stop, hide](https://www.w3.org/WAI/WCAG22/Understanding/pause-stop-hide.html)

## 5. SSE reconnects transport; the application restores correctness

| Source fact | ERPX implication |
| --- | --- |
| Native EventSource accepts a URL and `withCredentials`; its constructor provides no custom-header option. [WHATWG EventSource](https://html.spec.whatwg.org/multipage/server-sent-events.html#the-eventsource-interface) | Prefer an application-owned same-origin stream using the existing session. Do not put bearer tokens in URLs. If custom authorization headers require fetch streaming, implement parsing, reconnect, and cancellation deliberately. |
| EventSource reconnects after recoverable disconnection; `id` feeds `Last-Event-ID`, and `retry` adjusts the reconnect delay. `.close()` stops it; HTTP 204 can stop reconnecting. [WHATWG SSE](https://html.spec.whatwg.org/multipage/server-sent-events.html) | Recheck authentication and permissions on connect/reconnect. A cursor is untrusted input, not authorization. Persist replay history or explicitly reset to a fresh authorized snapshot; the protocol does not create that history. |
| SSE is one-way. HTTP/1 connections are limited per browser/domain; HTTP/2 stream capacity is negotiated. Comments can keep idle streams alive. [MDN using SSE](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events) | Multiplex report invalidations through an app-owned connection rather than opening one per widget. Send heartbeats, verify proxy flushing, cancel server work on disconnect, and test the deployed Tunnel path. |

Additional recommendations:

- Stream authorized invalidation IDs/revisions, then fetch bounded results through the gateway. This adds a request but keeps query policy centralized and lets bursts coalesce.
- Prevent the snapshot/subscription race with a cursor-bound snapshot and replay, or a documented reset/refetch protocol. Ignore duplicate or older revisions.
- Revalidate access during long sessions and stop delivery after revocation. Reconnect checks alone leave already-open streams with stale authorization.
- Show **Live**, **Reconnecting**, **Stale**, and **Paused** states separately from the latest successful data timestamp. Refreshing numbers should not regenerate the language-model output or rearrange widgets automatically.
- Define retries, backoff, replay retention, reset behavior, and per-viewer query budgets before calling the experience live.

## 6. Tradeoffs and validation before implementation completion

| Choice | Benefit | Cost or constraint |
| --- | --- | --- |
| Generated HTML/CSS with an application-owned interactive runtime | Smaller execution surface; consistent accessibility and refresh behavior | Less freedom than arbitrary JS; requires a widget interaction contract |
| Arbitrary generated JS in an isolated renderer | Broad custom interactions and visual expression | More security and resource-abuse testing; still no credentials or unrestricted fetching |
| SSE invalidation plus authorized refetch | Centralized query control, coalescing, smaller stream payloads | Extra request and explicit snapshot/replay coordination |
| Named sharing evaluated as the recipient | Existing business permissions remain meaningful | Some recipients see partial or unavailable widgets; explain the missing access clearly |

Recommended acceptance checks: unauthorized recipient and cross-tenant access; revoked access on an open stream; hidden sensitive text in shared artifacts; forged bridge messages and frame navigation; attempted resource/network exfiltration; duplicate, missed, and expired SSE cursors; reconnect through the public proxy; keyboard and touch-only add/reorder; pause and manual refresh; stale narrative provenance; and browser responsiveness under a misbehaving widget.

No feature implementation, browser security validation, or accessibility conformance is asserted by this research note.
