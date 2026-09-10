-- PostgreSQL 18 baseline; fresh installations only. No demo data.
-- Source: final schema of control-read through 0001_control_read.sql


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


CREATE TABLE platform_health (
  component TEXT PRIMARY KEY,
  status TEXT NOT NULL,
  checked_at BIGINT NOT NULL,
  detail TEXT
);
