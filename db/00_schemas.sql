-- folio/db/00_schemas.sql
-- Idempotent: safe to re-run. Creates the schemas + extensions + search_path for folio_db.
-- Apply with: psql -h localhost -U contract -d folio_db -v ON_ERROR_STOP=1 -f 00_schemas.sql

BEGIN;

CREATE SCHEMA IF NOT EXISTS finance;
CREATE SCHEMA IF NOT EXISTS perm;
CREATE SCHEMA IF NOT EXISTS hook;
CREATE SCHEMA IF NOT EXISTS hr;
CREATE SCHEMA IF NOT EXISTS law;
CREATE SCHEMA IF NOT EXISTS n8n;
CREATE SCHEMA IF NOT EXISTS folio;

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pgcrypto;

ALTER DATABASE folio_db SET search_path TO finance, perm, hook, hr, law, n8n, folio, public;

COMMIT;