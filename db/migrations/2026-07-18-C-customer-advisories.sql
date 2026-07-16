CREATE TABLE IF NOT EXISTS folio.customer_advisories (
  customer_id int PRIMARY KEY REFERENCES folio.customers(id) ON DELETE CASCADE,
  advisory text NOT NULL,
  severity text NOT NULL CHECK (severity IN ('ok','watch','critical')),
  computed_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS folio_customer_advisories_severity_idx
  ON folio.customer_advisories(severity);