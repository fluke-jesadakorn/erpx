# DEPRECATED — n8n flows

The orchestration workflow previously described here has been replaced by:

- `lib/slips/` — in-process OCR and slip storage
- `app/api/slips/` — upload and file routes
- `lib/ai/` — in-process AI router

The workflow JSON (`flows/finance-agent-bot.json`) is no longer shipped with this repo. If you need to revive the LINE-OA flow, copy the steps below from the legacy description and recreate them in n8n:

1. LINE webhook → GET media (vision) or text-only
2. Forward image as base64 to `app/api/ai/invoke` with `sectionKey: 'staff:ocr'` and `task: 'vision'`
3. Parse the JSON response
4. Persist to the `expenses` + `expense_items` tables
5. Send LINE Flex confirmation card back to the user
