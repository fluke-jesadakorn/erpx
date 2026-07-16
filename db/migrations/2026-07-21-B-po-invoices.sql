CREATE TABLE IF NOT EXISTS folio.po_invoices (
  id bigserial PRIMARY KEY,
  vendor_name text,
  vendor_id int REFERENCES folio.customers(id) ON DELETE SET NULL,
  invoice_no text,
  invoice_date date,
  file_path text NOT NULL,
  mime_type text,
  file_size bigint,
  status text NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending','extracted','drafted','rejected','failed')),
  draft_pr_id int REFERENCES folio.purchase_requisitions(id) ON DELETE SET NULL,
  draft_po_id int REFERENCES folio.purchase_orders(id) ON DELETE SET NULL,
  extracted jsonb,
  error text,
  uploaded_by int REFERENCES folio.users(id) ON DELETE SET NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS folio_po_invoices_status_idx
  ON folio.po_invoices(status, created_at DESC);

CREATE INDEX IF NOT EXISTS folio_po_invoices_vendor_idx
  ON folio.po_invoices(vendor_id);