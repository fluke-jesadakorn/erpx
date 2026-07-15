BEGIN;

CREATE SCHEMA IF NOT EXISTS law;

CREATE TABLE IF NOT EXISTS law.contracts (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  line_user_id text,
  line_group_id text,
  line_message_id text,
  file_name text NOT NULL,
  file_type text,
  file_mime text,
  file_data bytea,
  storage_bucket text,
  storage_path text,
  size_bytes bigint,
  chunk_count integer NOT NULL DEFAULT 0,
  status text NOT NULL DEFAULT 'pending',
  error_message text,
  uploaded_at timestamptz NOT NULL DEFAULT now(),
  doc_no text,
  category text,
  source text,
  metadata jsonb,
  updated_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE law.contracts ADD COLUMN IF NOT EXISTS file_mime text;
ALTER TABLE law.contracts ADD COLUMN IF NOT EXISTS file_data bytea;

CREATE TABLE IF NOT EXISTS law.contract_chunks (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  contract_id uuid NOT NULL REFERENCES law.contracts(id) ON DELETE CASCADE,
  chunk_index integer NOT NULL,
  content text NOT NULL,
  token_count integer,
  embedding vector(1024),
  metadata jsonb,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (contract_id, chunk_index)
);

CREATE TABLE IF NOT EXISTS law.contract_pages (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  contract_id uuid NOT NULL REFERENCES law.contracts(id) ON DELETE CASCADE,
  page_index integer NOT NULL,
  image_data bytea NOT NULL,
  image_mime text NOT NULL DEFAULT 'image/jpeg',
  bytes integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (contract_id, page_index)
);

CREATE INDEX IF NOT EXISTS idx_law_contracts_line_user ON law.contracts (line_user_id);
CREATE INDEX IF NOT EXISTS idx_law_contracts_status ON law.contracts (status);
CREATE UNIQUE INDEX IF NOT EXISTS idx_law_contracts_doc_no ON law.contracts (doc_no) WHERE doc_no IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_law_chunks_contract ON law.contract_chunks (contract_id);
CREATE INDEX IF NOT EXISTS idx_law_pages_contract ON law.contract_pages (contract_id);

CREATE OR REPLACE FUNCTION law.next_doc_seq() RETURNS text AS $$
DECLARE
  today text := to_char(now() AT TIME ZONE 'Asia/Bangkok', 'YYYYMMDD');
  seq integer;
BEGIN
  PERFORM pg_advisory_xact_lock(hashtext('law.doc_no_seq'));
  SELECT COALESCE(MAX(CAST(substring(doc_no FROM 'DOC-[0-9]{8}-([0-9]+)') AS integer)), 0) + 1
    INTO seq
    FROM law.contracts
   WHERE doc_no LIKE 'DOC-' || today || '-%';
  RETURN 'DOC-' || today || '-' || lpad(seq::text, 4, '0');
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION law.touch_updated_at() RETURNS trigger AS $$
BEGIN
  NEW.updated_at := now();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_law_contracts_touch ON law.contracts;
CREATE TRIGGER trg_law_contracts_touch
  BEFORE UPDATE ON law.contracts
  FOR EACH ROW EXECUTE FUNCTION law.touch_updated_at();

COMMIT;
