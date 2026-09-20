-- Additive: existing payments and approvals are not backfilled or reposted.
CREATE TABLE finance_supplier_batches (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, currency TEXT NOT NULL,
 bank_account_id TEXT NOT NULL REFERENCES finance_accounts(id), planned_date TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','exported','cancelled')),
 created_by TEXT NOT NULL, approved_by TEXT, approval_reason TEXT,
 exported_by TEXT, export_command_id TEXT, export_hash TEXT,
 version BIGINT NOT NULL DEFAULT 1, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 CHECK(status NOT IN ('approved','exported') OR (approved_by IS NOT NULL AND approved_by<>created_by AND approval_reason IS NOT NULL)),
 CHECK(status!='exported' OR (exported_by IS NOT NULL AND exported_by<>approved_by AND exported_by<>created_by AND export_hash IS NOT NULL))
);
CREATE TABLE finance_supplier_instructions (
 id TEXT PRIMARY KEY, batch_id TEXT NOT NULL REFERENCES finance_supplier_batches(id),
 invoice_id TEXT NOT NULL REFERENCES finance_invoices(id), invoice_version BIGINT NOT NULL,
 amount_minor BIGINT NOT NULL CHECK(amount_minor>0), beneficiary_name TEXT NOT NULL, beneficiary_account TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'reserved' CHECK(status IN ('reserved','exported','unknown','failed','confirmed','cancelled')),
 evidence TEXT, resolved_by TEXT, bank_reference TEXT,
 payment_id TEXT UNIQUE REFERENCES finance_payments(id), bank_transaction_id TEXT UNIQUE REFERENCES finance_bank_transactions(id),
 version BIGINT NOT NULL DEFAULT 1, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 UNIQUE(batch_id,invoice_id),
 CHECK(status!='confirmed' OR (payment_id IS NOT NULL AND bank_transaction_id IS NOT NULL AND bank_reference IS NOT NULL))
);
CREATE INDEX supplier_batch_queue ON finance_supplier_batches(company_id,status,updated_at);
CREATE INDEX supplier_instruction_reservations ON finance_supplier_instructions(invoice_id,status);
DO $$ DECLARE t text; BEGIN
 FOREACH t IN ARRAY ARRAY['finance_supplier_batches','finance_supplier_instructions'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
