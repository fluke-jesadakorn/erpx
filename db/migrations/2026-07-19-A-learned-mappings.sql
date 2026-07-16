CREATE TABLE IF NOT EXISTS folio.learned_mappings (
  vendor_name_norm text NOT NULL,
  account_code text NOT NULL REFERENCES chart_of_accounts(code),
  hits int NOT NULL DEFAULT 1,
  last_used_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (vendor_name_norm, account_code)
);

CREATE INDEX IF NOT EXISTS folio_learned_mappings_vendor_norm_idx
  ON folio.learned_mappings(vendor_name_norm);