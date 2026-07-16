CREATE TABLE IF NOT EXISTS folio.vision_chain (
  section_key text PRIMARY KEY,
  models text[] NOT NULL,
  updated_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO folio.vision_chain (section_key, models) VALUES
  ('staff:ocr', ARRAY['qwen3-vl:4b','qwen3.6:35b-a3b-q4_K_M','qwen3.6:35b-mlx'])
ON CONFLICT (section_key) DO UPDATE
  SET models = EXCLUDED.models, updated_at = now();
