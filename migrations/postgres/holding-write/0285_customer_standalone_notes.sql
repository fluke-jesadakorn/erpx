-- Customer 360 notes may be recorded before a transaction exists. The customer
-- remains the immutable owner; an order is an optional downstream source.
ALTER TABLE customer_contact_notes ALTER COLUMN order_id DROP NOT NULL;
CREATE INDEX customer_contact_notes_customer_scope
  ON customer_contact_notes(company_id,customer_id,created_at DESC,id DESC);
