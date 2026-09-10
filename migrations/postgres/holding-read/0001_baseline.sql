-- PostgreSQL 18 baseline; fresh installations only. No demo data.
-- Source: final schema of holding-read through 0002_workspace_scopes.sql


-- Shared PostgreSQL functions used by the application-owned SQL contract.
CREATE FUNCTION erpx_json_valid(document text) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$ SELECT document IS JSON $$;
CREATE FUNCTION erpx_json_extract(document text, path text) RETURNS text LANGUAGE sql IMMUTABLE AS $$
 SELECT jsonb_path_query_first(document::jsonb,path::jsonpath) #>> '{}' $$;
CREATE FUNCTION erpx_json_each(document text, path text DEFAULT '$') RETURNS TABLE(key text,value text) LANGUAGE sql IMMUTABLE AS $$
 SELECT (ordinality-1)::text, element #>> '{}' FROM jsonb_array_elements(COALESCE(jsonb_path_query_first(document::jsonb,path::jsonpath),'[]'::jsonb)) WITH ORDINALITY AS a(element,ordinality) $$;
CREATE FUNCTION erpx_json(document text) RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$ SELECT document::jsonb $$;
CREATE FUNCTION erpx_json(document jsonb) RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$ SELECT document $$;
CREATE FUNCTION erpx_json_array_append(state jsonb, element anyelement) RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$ SELECT state || jsonb_build_array(element) $$;
CREATE AGGREGATE erpx_json_array_agg(anyelement) (SFUNC=erpx_json_array_append,STYPE=jsonb,INITCOND='[]');


CREATE TABLE work_items (
  id TEXT PRIMARY KEY,
  source_type TEXT NOT NULL,
  source_id TEXT NOT NULL,
  source_version BIGINT NOT NULL,
  title TEXT NOT NULL,
  subtitle TEXT NOT NULL,
  queue TEXT NOT NULL CHECK(queue IN ('todo','waiting','completed')),
  assigned_role TEXT,
  assigned_user_id TEXT,
  company_id TEXT NOT NULL,
  branch_id TEXT,
  due_at BIGINT,
  updated_at BIGINT NOT NULL, department_id TEXT,
  UNIQUE(source_type, source_id)
);

CREATE TABLE projection_receipts (
  event_id TEXT PRIMARY KEY,
  aggregate_id TEXT NOT NULL,
  aggregate_version BIGINT NOT NULL,
  applied_at BIGINT NOT NULL
);

CREATE TABLE projection_checkpoints (
  projector TEXT PRIMARY KEY,
  last_event_id TEXT,
  last_event_at BIGINT,
  updated_at BIGINT NOT NULL
);

CREATE INDEX work_queue_idx ON work_items(queue, assigned_user_id, updated_at DESC);
