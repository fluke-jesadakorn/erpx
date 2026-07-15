# DEPRECATED — n8n flows

The orchestration workflow previously described here has been replaced by:

- `folio/ocr/` — Fastify OCR service (port 3004)
- `folio/app/src/app/api/upload/` — upload proxy
- `folio/app/src/app/api/ai/invoke/` — AI router (called via `x-ai-key` header from the OCR service)

The workflow JSON (`flows/finance-agent-bot.json`) is no longer shipped with this repo. If you need to revive the LINE-OA flow, copy the steps below from the legacy description and recreate them in n8n:

1. LINE webhook → GET media (vision) or text-only
2. Forward image as base64 to `app/api/ai/invoke` with `sectionKey: 'staff:ocr'` and `task: 'vision'`
3. Parse the JSON response
4. Persist to the `expenses` + `expense_items` tables
5. Send LINE Flex confirmation card back to the user
