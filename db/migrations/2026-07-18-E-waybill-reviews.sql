CREATE TABLE IF NOT EXISTS folio.waybill_reviews (
  waybill_id text NOT NULL,
  stage text NOT NULL CHECK (stage IN ('hod','am')),
  hint text NOT NULL,
  generated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (waybill_id, stage)
);

CREATE INDEX IF NOT EXISTS waybill_reviews_generated_at_idx
  ON folio.waybill_reviews(generated_at DESC);