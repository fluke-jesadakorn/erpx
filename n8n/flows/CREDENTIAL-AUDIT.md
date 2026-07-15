# Flow 03 — Docs Hub — credential + flow audit

**Last updated:** 2026-06-22 (host-native stack, smart routing with 4 tools)

## Credentials

### httpHeaderAuth (LINE Bearer Auth)
- ID: `LNHDR8F94CE3AA25E4B8E823C70`
- Data: `{"name":"Authorization","value":"Bearer <LINE_CHANNEL_ACCESS_TOKEN>"}`
- **Required on:** `LINE Download from LINE`, `LINE Reply Success`, `LINE Reply Error`, `LINE: Reply Non-File`, `AI: Reply Text`, `AI: Send Reply`

### postgres (PG Folio - localhost:5432)
- ID: `regb87eec1e08a5cce3`
- Data: `{"host":"localhost","port":5432,"database":"folio_db","user":"contract","password":"<see .env>","ssl":"disable"}`
- **Required on:** `LINE Register Start`, `LINE Insert contract row`, `LINE Insert chunks`, `LINE Register Done`, `AI: List Contracts`, `AI: Get Stats`

### s3 (MinIO Folio - localhost:9000)
- ID: `minio-creds-folio`
- Data: `{"endpoint":"localhost:9000","region":"us-east-1","accessKeyId":"<see .env>","secretAccessKey":"<see .env>","forcePathStyle":true}`
- **Required on:** `MinIO Upload` (หลัง chunk+embed สำเร็จ)

## Smart routing flow

```
LINE: Is file? (Switch)
├── is_file → [Download → Extract → Chunk → Embed → Combine → Insert contract → Insert chunks → MinIO Upload → Reply Success]
└── not_file → AI Agent (Ollama qwen3.6:35b-a3b-q4_K_M) → Parse AI Response
                                                ↓
                                          AI Route Switch (4 outputs)
                                          ├── search → AI: Call Vector Search → Build Flex Card → AI: Send Reply
                                          ├── list → AI: List Contracts → AI: Format List → AI: Reply Text
                                          ├── stats → AI: Get Stats → AI: Format Stats → AI: Reply Text
                                          └── text → AI: Reply Text
                                                                       ↓
                                                                  Format Response
```

## AI tools (qwen3.6:35b-a3b-q4_K_M via Ollama /api/chat with system prompt)

| Tool | Args | When to use |
|------|------|-------------|
| `search_documents` | `query, limit=5` | "หา/ค้น/ค้นหา [keyword]" — semantic vector search |
| `list_documents` | `filter="", limit=10` | "กี่ฉบับ/list/ทั้งหมด/อันไหนบ้าง" — registry list |
| `get_stats` | (none) | "สรุป/ภาพรวม/สถิติ/สถานะ" — count + breakdown |
| (none) | — | greeting/chat/capabilities question |

> OpenRouter (`gemma-4-31b-it:free`) ถูกใช้ใน flow เก่า `02-line-search.json` (ใน `archive/`) แต่ไม่ใช้ใน flow ปัจจุบัน

## law.contracts schema requirements

```sql
-- Required columns (added via migration)
ALTER TABLE law.contracts ADD COLUMN IF NOT EXISTS doc_no TEXT;
ALTER TABLE law.contracts ADD COLUMN IF NOT EXISTS category TEXT;
ALTER TABLE law.contracts ADD COLUMN IF NOT EXISTS source TEXT;
ALTER TABLE law.contracts ADD COLUMN IF NOT EXISTS metadata JSONB;
ALTER TABLE law.contracts ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ;
CREATE UNIQUE INDEX IF NOT EXISTS idx_law_contracts_doc_no ON law.contracts (doc_no) WHERE doc_no IS NOT NULL;

-- Sequence function
CREATE OR REPLACE FUNCTION law.next_doc_seq() RETURNS TEXT AS $$ ... $$ LANGUAGE plpgsql;
```

ดู schema ปัจจุบันใน `folio/db/05_law/001_schema.sql`.

## Service management (host-native launchd)

```bash
# Restart n8n
launchctl kickstart -k gui/$(id -u)/com.folio.n8n

# Restart MinIO
launchctl kickstart -k gui/$(id -u)/com.folio.minio

# View n8n log
tail -f /Users/fluke/Desktop/Work/Contents/infra/logs/n8n.log

# Postgres auto-start via brew services (postgresql@18)
# Cloudflared manual — tunnel ID b8f4ccf5-67de-4bfa-b292-7641ad185006
#   ingress: n8n.jesadakorn.com → localhost:5678
#            ai.jesadakorn.com   → localhost:11434
```
