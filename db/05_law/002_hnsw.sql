CREATE INDEX IF NOT EXISTS idx_law_chunks_embedding
  ON law.contract_chunks USING hnsw (embedding vector_cosine_ops);
