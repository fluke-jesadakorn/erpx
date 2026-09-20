-- Some early 0041 installations stamped imported balances with the migration year.
-- Recover the source year, while preserving the original per-code source records.
ALTER TABLE leave_balance_entries DISABLE TRIGGER leave_balance_entries_immutable;
UPDATE leave_balance_entries l SET entitlement_year=b.year,effective_date=b.year::text||'-01-01'
 FROM leave_legacy_balance_provenance b
 WHERE l.company_id=b.company_id AND l.engagement_id=b.engagement_id
 AND l.command_key='legacy-balance-'||b.id AND l.kind='opening'
 AND (l.entitlement_year!=b.year OR l.effective_date!=b.year::text||'-01-01');
ALTER TABLE leave_balance_entries ENABLE TRIGGER leave_balance_entries_immutable;
UPDATE iam_permissions SET definition=definition||'{"risk":"privileged"}'::jsonb WHERE id='leave.policy.manage';
-- Every new ledger or segment reference must remain within its Company.
CREATE FUNCTION leave_request_reference_integrity() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF NEW.request_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM leave_requests r WHERE r.id=NEW.request_id AND r.company_id=NEW.company_id) THEN RAISE EXCEPTION 'LEAVE_COMPANY_MISMATCH'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER leave_ledger_request_company BEFORE INSERT ON leave_balance_entries FOR EACH ROW EXECUTE FUNCTION leave_request_reference_integrity();
CREATE TRIGGER leave_segment_request_company BEFORE INSERT OR UPDATE ON leave_day_segments FOR EACH ROW EXECUTE FUNCTION leave_request_reference_integrity();
