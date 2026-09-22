ALTER TABLE field_service_billing_releases
  ADD COLUMN IF NOT EXISTS warranty_end_snapshot TEXT,
  ADD COLUMN IF NOT EXISTS pricing_date TEXT;
