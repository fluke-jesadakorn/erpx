CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS folio.vendor_embeddings (
  id bigserial PRIMARY KEY,
  expense_id int NOT NULL REFERENCES folio.expenses(id) ON DELETE CASCADE,
  submitter_id int REFERENCES folio.users(id) ON DELETE SET NULL,
  vendor_name text,
  description text,
  amount_thb numeric(14,2),
  transaction_date date,
  embedding vector(1024),
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS folio_vendor_embeddings_vec_idx
  ON folio.vendor_embeddings USING ivfflat (embedding vector_cosine_ops)
  WITH (lists = 100);

CREATE INDEX IF NOT EXISTS folio_vendor_embeddings_submitter_idx
  ON folio.vendor_embeddings(submitter_id);

CREATE INDEX IF NOT EXISTS folio_vendor_embeddings_date_idx
  ON folio.vendor_embeddings(transaction_date);