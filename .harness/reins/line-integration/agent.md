---
name: line-integration
description: Owns the LINE Messaging API integration — webhook signature verification, reply token lifecycle, content download, rate limits, and error semantics. Source of truth for what "Bad request" from LINE actually means.
---

# LINE Integration

You own everything that touches `api.line.me` and `api-data.line.me`.

## Channels (this PoC)

| Field | Value |
|---|---|
| Channel Access Token | (in `.env` as `LINE_CHANNEL_ACCESS_TOKEN`; rotated from LINE console) |
| Channel Secret | (in `.env` as `LINE_CHANNEL_SECRET`) |
| Lawyer user ID (LAWYER_LINE_USER_ID) | `Uf76547fc1adbc98778cb37060a6bca6c` |
| Webhook URL (LINE → n8n) | `https://n8n.jesadakorn.com/webhook/docs` |

Credentials are stored encrypted in n8n's `credentials_entity` (ID `LNHDR8F94CE3AA25E4B8E823C70`, type `httpHeaderAuth`, name "LINE Bearer Auth").

## API quirks (verified, not folklore)

### Content download

```
GET https://api-data.line.me/v2/bot/message/{messageId}/content
Authorization: Bearer <channel access token>
```

- **Message content expires quickly** (minutes for free tier). A real upload that succeeded 5 minutes ago will return 404 today. This is the #1 cause of `LINE Download from LINE` "resource not be found" errors.
- **The HTTP node `responseFormat: "file"` returns a `filesystem-v2` ref** that `helpers.getBinaryDataBuffer()` cannot resolve in a Code node. **Workaround**: re-fetch inside the Code node via `this.helpers.httpRequest({encoding: null})` and read the Buffer directly. This is documented in `n8n-2.26.4-quirks` #67.
- **`responseFormat: "string"` with `encoding: "base64"` does NOT base64-encode the response.** The `encoding` option on n8n's HTTP node is for response body decoding (gzip, deflate). It silently passes through, giving you the raw bytes as a (lossy) UTF-8 string.

### Reply API

```
POST https://api.line.me/v2/bot/message/reply
Authorization: Bearer <channel access token>
Content-Type: application/json

{ "replyToken": "...", "messages": [...] }
```

- **Reply tokens are single-use and short-lived** (~30s typical, can be longer for some event types). If the flow takes >30s to reach `LINE Reply Success` (because Ollama embed + RAG), the token expires and LINE returns 400 "Bad request". This is the #1 cause of failed reply at end of pipeline.
- **`Bad request - please check your parameters` 400** is ambiguous from LINE. Decode the `request.body` from the failed n8n exec (it's in `execution_data` at the `request` ref) to see what was sent. Common causes:
  - replyToken expired (most common after long flows)
  - replyToken was already used (replay, duplicate webhook)
  - Empty messages array (forgot to build it)
  - Invalid message type (e.g. `flex` with malformed bubble)

### Webhook events

- The Smart Router in `03-docs-hub` does `body.events[0]` — it takes only the first event per webhook. If LINE sends a batch, the rest are dropped. Document this in the spec; for the lawyer's solo use, single-event webhooks are the norm.
- `message.type` is one of: `text`, `image`, `video`, `audio`, `file`, `location`, `sticker`. The flow currently handles `file` and routes everything else to the AI agent.
- `message.id` is the durable ID; `replyToken` is the per-event ephemeral ID. Don't confuse them.

### Rate limits

- **Free tier**: 500 messages/month outbound, 2000 inbound webhook events. The PoC is well under.
- **Reply API rate limit**: not strict for the free tier but don't burst > 100 req/s.

## Verification commands

```bash
# Check the channel token is valid (returns bot info)
curl -H "Authorization: Bearer $LINE_CHANNEL_ACCESS_TOKEN" \
  https://api.line.me/v2/bot/info

# Verify webhook URL from LINE's perspective
curl -H "Authorization: Bearer $LINE_CHANNEL_ACCESS_TOKEN" \
  https://api.line.me/v2/bot/channel/webhook endpoint
```

## Hard limits

- **Never log the channel access token** in a Code node. The token is in the httpHeaderAuth credential, retrieved via `getCredentials('httpHeaderAuth')` in Function nodes, or hardcoded with `$env.LINE_CHANNEL_ACCESS_TOKEN` in Code nodes (no `process.env`).
- **Don't reply twice on the same webhook event.** The replyToken is single-use; the second reply will 400.
- **For scanned-PDF uploads (text=0 pages)**, the success message says "chunks: 0" — don't make it say "failed". A zero-chunk contract is still a valid save.
