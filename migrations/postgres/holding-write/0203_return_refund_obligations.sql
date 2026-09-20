-- Preserve old returns without inventing historical approvals or refund balances.
ALTER TABLE commerce_returns ADD COLUMN credited_minor BIGINT CHECK(credited_minor>=0);
ALTER TABLE commerce_returns ADD COLUMN refund_minor BIGINT CHECK(refund_minor>=0);
ALTER TABLE commerce_returns ADD CONSTRAINT return_adjustment_totals CHECK
 (credited_minor IS NULL AND refund_minor IS NULL OR credited_minor IS NOT NULL AND refund_minor IS NOT NULL AND credited_minor+refund_minor=total_minor);

CREATE TABLE finance_refund_obligations (
 id TEXT PRIMARY KEY,
 company_id TEXT NOT NULL,
 branch_id TEXT NOT NULL,
 department_id TEXT,
 return_id TEXT NOT NULL UNIQUE REFERENCES commerce_returns(id),
 invoice_id TEXT NOT NULL REFERENCES finance_invoices(id),
 party_id TEXT NOT NULL REFERENCES finance_parties(id),
 direction TEXT NOT NULL CHECK(direction IN ('outgoing','incoming')),
 currency TEXT NOT NULL,
 chart_id TEXT NOT NULL REFERENCES finance_charts(id),
 balance_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 amount_minor BIGINT NOT NULL CHECK(amount_minor>0),
 outstanding_minor BIGINT NOT NULL CHECK(outstanding_minor>=0 AND outstanding_minor<=amount_minor),
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','settled')),
 beneficiary_name TEXT,
 beneficiary_account TEXT,
 approval_reason TEXT,
 created_by TEXT NOT NULL,
 approved_by TEXT,
 approved_at BIGINT,
 version BIGINT NOT NULL DEFAULT 1,
 created_at BIGINT NOT NULL,
 updated_at BIGINT NOT NULL,
 CHECK(status='pending' OR (approved_by IS NOT NULL AND approved_by<>created_by AND approval_reason IS NOT NULL)),
 CHECK((status='settled')=(outstanding_minor=0))
);
CREATE INDEX refund_obligation_queue ON finance_refund_obligations(company_id,branch_id,status,updated_at);
CREATE TABLE finance_refund_instructions (
 id TEXT PRIMARY KEY,
 obligation_id TEXT NOT NULL REFERENCES finance_refund_obligations(id),
 company_id TEXT NOT NULL,
 amount_minor BIGINT NOT NULL CHECK(amount_minor>0),
 planned_date TEXT NOT NULL,
 bank_account TEXT NOT NULL,
 cash_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 status TEXT NOT NULL DEFAULT 'prepared' CHECK(status IN ('prepared','exported','unknown','failed','confirmed','cancelled')),
 export_hash TEXT,
 export_command_id TEXT,
 exported_at BIGINT,
 bank_reference TEXT,
 evidence TEXT,
 payment_id TEXT UNIQUE REFERENCES finance_payments(id),
 bank_transaction_id TEXT UNIQUE REFERENCES finance_bank_transactions(id),
 prepared_by TEXT NOT NULL,
 resolved_by TEXT,
 version BIGINT NOT NULL DEFAULT 1,
 created_at BIGINT NOT NULL,
 updated_at BIGINT NOT NULL,
 CHECK(status!='confirmed' OR (payment_id IS NOT NULL AND bank_transaction_id IS NOT NULL AND bank_reference IS NOT NULL)),
 CHECK(status NOT IN ('exported','unknown','failed','confirmed') OR export_hash IS NOT NULL)
);
CREATE INDEX refund_instruction_queue ON finance_refund_instructions(obligation_id,status);
-- Existing installations have already run 0900, so install invalidation here too.
DO $$ DECLARE t text; BEGIN
 FOREACH t IN ARRAY ARRAY['finance_refund_obligations','finance_refund_instructions'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
