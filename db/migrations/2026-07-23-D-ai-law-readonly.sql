-- Grant folio_readonly_agent SELECT on law.* schema tables so AI ask_sql can answer
-- contract / document questions without exposing raw file_data, image_data, or embeddings.
-- hr.* SELECT was already granted in 2026-07-20-D-readonly-agent.sql.

GRANT USAGE ON SCHEMA law TO folio_readonly_agent;
GRANT SELECT ON ALL TABLES IN SCHEMA law TO folio_readonly_agent;
ALTER DEFAULT PRIVILEGES IN SCHEMA law GRANT SELECT ON TABLES TO folio_readonly_agent;

-- Schema digest intentionally allows column-level SELECT but excludes:
--   law.contracts.file_data      (bytea — large binary, never needed for analytics)
--   law.contract_pages.image_data (bytea — large binary, render via presigned URL separately)
--   law.contract_chunks.embedding (vector(1024) — opaque, no semantic search via ask_sql yet)