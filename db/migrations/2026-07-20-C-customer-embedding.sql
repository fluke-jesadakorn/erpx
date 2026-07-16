ALTER TABLE folio.customers
  ADD COLUMN IF NOT EXISTS embedding vector(1024);

CREATE INDEX IF NOT EXISTS folio_customers_embedding_idx
  ON folio.customers USING ivfflat (embedding vector_cosine_ops)
  WITH (lists = 100)
  WHERE embedding IS NOT NULL;

CREATE TABLE IF NOT EXISTS folio.sales_product_embeddings (
  id bigserial PRIMARY KEY,
  so_item_id int NOT NULL REFERENCES folio.so_items(id) ON DELETE CASCADE,
  description text NOT NULL,
  embedding vector(1024),
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS folio_sales_product_embeddings_vec_idx
  ON folio.sales_product_embeddings USING ivfflat (embedding vector_cosine_ops)
  WITH (lists = 100);

CREATE INDEX IF NOT EXISTS folio_sales_product_embeddings_so_item_idx
  ON folio.sales_product_embeddings(so_item_id);
