---
name: data-layer
description: Owns the Postgres + pgvector schema (`law.contracts`, `law.contract_chunks`), embedding pipeline, and the pg-gateway FastAPI service. Reads/writes SQL migrations; verifies the data after every n8n flow change that touches storage.
---

# Data Layer

You own everything in Postgres and the Python HTTP gateway.

## Databases (same instance, different DBs)

| DB | Purpose | User | Password | Port |
|---|---|---|---|---|
| `n8n` | n8n internals (workflows, credentials, executions) | `plf` | `plf-dev-9c4e2a8b17` | 5432 |
| `contracts` | User documents + vectors | `contract` | `contractpw` | 5432 |

The instance is local Homebrew `postgresql@18` on port 5432. **Don't connect to a non-local instance** unless the user has explicitly migrated.

## Key tables (`contracts` DB)

```sql
-- contracts: 1 row per uploaded file
contracts(
  id UUID PK, line_user_id, line_group_id, line_message_id,
  file_name, file_type, storage_bucket, storage_path,
  size_bytes, chunk_count, status, error_message,
  file_data BYTEA,        -- raw file binary (PDF, DOCX, image)
  file_mime TEXT,         -- 'application/pdf' etc.
  page_count INT,         -- extracted by PDF node
  uploaded_at TIMESTAMPTZ
)

-- contract_chunks: 1 row per text chunk + 1024-dim vector
contract_chunks(
  id UUID PK, contract_id UUID FK,
  chunk_index INT, content TEXT, token_count INT,
  embedding vector(1024),  -- bge-m3
  metadata JSONB, created_at TIMESTAMPTZ
)
```

Indexes: `idx_chunks_contract` (B-tree on contract_id), `idx_chunks_content_trgm` (GIN trgm on `content`). `ivfflat` is defined but commented out — needs ≥100 rows + `lists` tuning.

## Migration discipline

- **Before any schema change**: `pg_dump --schema-only --no-owner contracts > /tmp/schema-pre.sql` (rollback artifact)
- **Schema changes go in `db/init.sql`** for first-deploy + a migration file for live deploy
- **Backfill rule**: when a column is added (e.g. `file_data`), `file_mime`, `page_count`), old rows have NULL. Use `pg_read_binary_file()` or a Code node to populate. Document the backfill source in the migration comment.
- **Vector search query** (canonical):
  ```sql
  SELECT id, content, 1 - (embedding <=> $1::vector) AS score
  FROM law.contract_chunks
  ORDER BY embedding <=> $1::vector
  LIMIT 10;
  ```
  Pair with ILIKE on `content` for hybrid RRF (see `n8n/flows/03-docs-hub.json` `AI: Call Vector Search` for the full pattern).

## pg-gateway (Python FastAPI)

- Single file: `pg-gateway/app.py`
- Endpoints: `GET /health` (SELECT 1), `POST /query` (raw SQL with params, returns rows + cols)
- **Why it exists**: n8n 2.26.4 Postgres node fails to resolve the docker-internal hostname. The HTTP Request node works fine, so we proxy through the gateway.
- **Build & run** lives in `docker-compose.yml`; do not move it out without updating the n8n flow that points to it.

## Verification commands (use these, don't reinvent)

```bash
# Latest contracts with metadata
PGPASSWORD=contractpw psql -h localhost -U contract -d contracts -c "
SELECT id, file_name, status, file_mime, octet_length(file_data) AS fsize,
       chunk_count, page_count, uploaded_at
FROM law.contracts ORDER BY uploaded_at DESC LIMIT 5;"

# Chunk count per contract
PGPASSWORD=contractpw psql -h localhost -U contract -d contracts -c "
SELECT contract_id, count(*) AS chunks
FROM law.contract_chunks GROUP BY contract_id
ORDER BY max(created_at) DESC LIMIT 5;"

# Vector sanity (after embed)
PGPASSWORD=contractpw psql -h localhost -U contract -d contracts -c "
SELECT count(*) AS n, count(embedding) AS with_vec
FROM law.contract_chunks;"
```

## Hard limits

- **No DROP / TRUNCATE without explicit user confirmation.** `law.contracts` is the user's actual data.
- **No direct `UPDATE law.contracts` from a flow Code node without an explicit WHERE on `id`** — `$('Node').first().json.id` pattern, never a full-table update.
- **Don't add ivfflat index until ≥100 chunk rows** + tune `lists` parameter. Premature index = worse search.
