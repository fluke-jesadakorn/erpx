# n8n Setup

> **Historical context (2026-07-16 flatten).** Most of the n8n flows this doc
> describes are now deprecated — the Law RAG and HR agent ingest paths live
> in-process inside folio's Next.js server (`lib/hook/`, `lib/law/`,
> `lib/hr/agent.ts`). The flows under `folio/n8n/flows/` are kept only as a
> reference of the old externalization. New ingest should be added to the
> Next.js route handlers in `app/api/hook/{line,line-hr,line-law,generic}/`,
> not as new n8n flows.
>
> The setup steps below still apply for those who run the old flows or who
> want to flip them back on.

n8n รัน host-native ผ่าน launchd (`com.folio.n8n`) ที่ `http://localhost:5678` และ exposed ผ่าน Cloudflare tunnel `https://n8n.jesadakorn.com`.

## 1. เข้า n8n UI

```bash
open https://n8n.jesadakorn.com
```

ครั้งแรก n8n จะถามสร้าง owner account (email + password) — ทำครั้งเดียว

## 2. Environment Variables

n8n อ่านจาก `/Users/fluke/Desktop/Work/Contents/infra/.env` (host-native runtime env, **gitignored**).

Template reference: `folio/.env.example`.

```
# LINE
LINE_CHANNEL_ACCESS_TOKEN=<your token>
LINE_CHANNEL_SECRET=<your secret>

# Ollama (host native localhost:11434; หรือใช้ tunnel https://ai.jesadakorn.com)
OLLAMA_URL=http://localhost:11434
OLLAMA_EMBED_MODEL=bge-m3
OLLAMA_AGENT_MODEL=qwen3.6:35b-a3b-q4_K_M
EMBED_DIM=1024

# Postgres (brew postgresql@18 on localhost:5432)
POSTGRES_USER=contract
POSTGRES_PASSWORD=contractpw
POSTGRES_DB=contracts
POSTGRES_PORT=5432

# n8n DB (separate from contracts DB)
N8N_DB_NAME=folio_db
N8N_DB_USER=n8n_user
N8N_DB_PASSWORD=n8n_pw

# MinIO (host native localhost:9000)
MINIO_ROOT_USER=minioadmin
MINIO_ROOT_PASSWORD=minioadmin
MINIO_BUCKET=folio-storage

# n8n public URL (cloudflared tunnel)
N8N_PUBLIC_URL=https://n8n.jesadakorn.com
N8N_EDITOR_BASE_URL=https://n8n.jesadakorn.com
```

> OpenRouter config (`OPENROUTER_*`) ยังอยู่ใน env ตัวอย่าง แต่ active flows (03-docs-hub, 04-docs-admin) ใช้ Ollama ทั้ง embed และ chat agent — OpenRouter เก็บไว้สำหรับ flow เก่าใน `n8n/flows/archive/` เท่านั้น

## 3. Architecture ปัจจุบัน (Smart Router + AI Tools)

```
LINE webhook (contract-rag-line)
  ├─ file? → Download from LINE → Extract text → Chunk (Code node JS) →
  │          Embed (Ollama bge-m3 /api/embeddings) → Insert contract_chunks (PG) →
  │          Upload MinIO → Reply Success
  └─ text? → AI Agent (Ollama qwen3.6:35b-a3b-q4_K_M /api/chat with tools) →
              AI Route Switch (4 outputs)
              ├─ search_documents → vector search (PG pgvector) → Build Flex Card → Reply
              ├─ list_documents    → SELECT contracts → Format list → Reply
              ├─ get_stats         → aggregate stats → Format → Reply
              └─ text              → Reply text ตรง
```

## 4. Credentials ที่ต้องตั้งใน n8n UI

| Name | Type | Host | Port | User | Database | Password |
|---|---|---|---|---|---|---|
| PG Contracts - localhost:5432 | postgres | localhost | 5432 | contract | contracts | contractpw |
| LINE Bearer Auth | httpHeaderAuth | — | — | — | — | `Bearer <LINE_CHANNEL_ACCESS_TOKEN>` |
| MinIO Contracts | s3 | localhost:9000 | — | minioadmin | — | minioadmin (forcePathStyle=true) |

> ดู credential IDs ปัจจุบันใน `n8n/flows/CREDENTIAL-AUDIT.md`

## 5. Import Flows

ไปที่ **Workflows → Import from File**:
1. Import `n8n/flows/03-docs-hub.json` — LINE bot + smart router (main flow)
2. Import `n8n/flows/04-docs-admin.json` — Admin UI + file preview
3. **Activate** ทั้งคู่

Webhook paths:
- `contract-rag-line` — LINE webhook
- `docs-admin-ui`, `admin-stats`, `admin-file` — admin endpoints

## 6. ตั้ง Webhook URL ใน LINE

ดู `docs/LINE-SETUP.md`

URL: `https://n8n.jesadakorn.com/webhook/contract-rag-line`

## 7. ทดสอบ

```bash
# Smoke test admin stats
curl -s https://n8n.jesadakorn.com/webhook/admin-stats
# 期待: {"ok":true,"action":"stats","data":{...}}

# Test LINE webhook (mock event — ไม่ตอบกลับเพราะ replyToken test)
curl -X POST https://n8n.jesadakorn.com/webhook/contract-rag-line \
  -H "Content-Type: application/json" \
  -d '{"destination":"U","events":[{"type":"message","replyToken":"test","message":{"type":"text","id":"1","text":"สวัสดี"},"source":{"type":"user","userId":"U"},"timestamp":1718554800000}]}'

# 1. ส่งไฟล์ PDF เข้า LINE bot จริง
# 2. ดู execution ใน n8n UI
# 3. พิมพ์คำถามเกี่ยวกับสัญญา
```

## 8. ตรวจข้อมูลใน Postgres

```bash
PGPASSWORD=contractpw psql -h localhost -U contract -d contracts -c \
  "SELECT id, file_name, status, chunk_count FROM contracts ORDER BY uploaded_at DESC LIMIT 5;"

PGPASSWORD=contractpw psql -h localhost -U contract -d contracts -c \
  "SELECT count(*) FROM contract_chunks;"

PGPASSWORD=n8n_pw psql -h localhost -U n8n_user -d folio_db -c \
  "SELECT name, active FROM workflow_entity;"
```

## 9. จัดการ services (launchd)

```bash
# ตรวจสถานะ
launchctl list | grep folio

# Restart n8n
launchctl kickstart -k gui/$(id -u)/com.folio.n8n

# Restart MinIO
launchctl kickstart -k gui/$(id -u)/com.folio.minio

# ดู log
tail -f /Users/fluke/Desktop/Work/Contents/infra/logs/n8n.log
```

## Troubleshooting

| อาการ | สาเหตุ | แก้ |
|---|---|---|
| `401 Unauthorized` จาก LINE | token หมดอายุ | rotate LINE token ใน LINE console + update n8n credential `LINE Bearer Auth` |
| `model 'qwen3.6:35b-a3b-q4_K_M' not found` (404) | ยังไม่ pull model | `ollama pull qwen3.6:35b-a3b-q4_K_M` (หรือดูใน `infra/scripts/`) |
| `vector extension not found` | Postgres ไม่มี pgvector | `brew install pgvector` (ต้องใช้ Postgres 18) |
| Webhook 404 หลัง restart | flow ไม่ active | เปิด n8n UI → activate flow `03-docs-hub` |
| `EACCES ~/Desktop` ใน n8n.log | launchd ไม่มี Full Disk Access | System Settings > Privacy & Security → เพิ่ม `/Users/fluke/.nvm/versions/node/v22.23.0/bin/node` |
| n8n ไม่ start หลัง reboot | launchd agent ไม่ load | `launchctl load ~/Library/LaunchAgents/com.folio.n8n.plist` |

## หมายเหตุ

- **Host-native stack** ไม่ใช้ docker แล้ว — rollback path อยู่ใน `AGENTS.md` section "Rollback"
- **Postgres 18 บน Mac** ใช้ brew `postgresql@18` + `pgvector` — ทั้ง `contracts` (owner: contract) และ `folio_db` (owner: n8n_user) อยู่ใน instance เดียวกัน
- **node 22 (nvm)** จำเป็นเพราะ n8n 2.26.8 ใช้ isolated-vm native build (node 26 ไม่รองรับ)
- **Ollama** ทำหน้าที่ทั้ง embedding (bge-m3, 1024 dim) และ chat agent (qwen3.6:35b-a3b-q4_K_M) — ไม่มี data leaves host นอกจากผ่าน Cloudflare tunnel
