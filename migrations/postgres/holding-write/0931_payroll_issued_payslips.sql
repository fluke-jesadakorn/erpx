-- Retained employee payslips are issued facts of a successful payroll posting.
-- Existing posted history is issued explicitly by the owner; no silent rewrite.
CREATE TABLE hr_issued_payslips (
 id text PRIMARY KEY,
 company_id text NOT NULL,
 run_id text NOT NULL REFERENCES hr_payroll_runs(id),
 run_revision bigint NOT NULL CHECK(run_revision>0),
 run_version bigint NOT NULL CHECK(run_version>0),
 item_id text NOT NULL REFERENCES hr_payroll_items(id),
 engagement_id text NOT NULL REFERENCES hr_engagements(id),
 person_id text NOT NULL REFERENCES hr_people(id),
 recipient_user_id text,
 pay_date text NOT NULL,
 currency text NOT NULL,
 gross_minor bigint NOT NULL,
 deductions_minor bigint NOT NULL,
 net_minor bigint NOT NULL,
 source_digest text NOT NULL CHECK(source_digest ~ '^[a-f0-9]{64}$'),
 snapshot_json text NOT NULL CHECK(erpx_json_valid(snapshot_json)),
 export_html text NOT NULL,
 export_sha256 text NOT NULL CHECK(export_sha256 ~ '^[a-f0-9]{64}$'),
 issued_by text NOT NULL,
 command_id text NOT NULL,
 issued_at bigint NOT NULL,
 UNIQUE(run_id,item_id),
 CHECK(gross_minor-deductions_minor=net_minor)
);
CREATE INDEX hr_issued_payslips_own ON hr_issued_payslips(company_id,recipient_user_id,pay_date DESC,id);
CREATE FUNCTION hr_issued_payslip_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 RAISE EXCEPTION 'Issued payroll payslip source and bytes are immutable' USING ERRCODE='23514';
END; $$;
CREATE TRIGGER hr_issued_payslip_immutable BEFORE UPDATE OR DELETE ON hr_issued_payslips FOR EACH ROW EXECUTE FUNCTION hr_issued_payslip_immutable();
CREATE FUNCTION hr_issued_payslip_posted_source() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM hr_payroll_runs r JOIN hr_payroll_items i ON i.run_id=r.id JOIN hr_engagements e ON e.id=i.engagement_id
   JOIN hr_payroll_postings p ON p.run_id=r.id JOIN finance_journals j ON j.id=p.journal_id
   WHERE r.id=NEW.run_id AND r.company_id=NEW.company_id AND r.status='posted' AND r.revision=NEW.run_revision
   AND i.id=NEW.item_id AND i.status='posted' AND i.engagement_id=NEW.engagement_id AND e.person_id=NEW.person_id
   AND i.gross_minor=NEW.gross_minor AND i.deductions_minor=NEW.deductions_minor AND i.net_minor=NEW.net_minor
   AND i.currency=NEW.currency AND r.pay_date=NEW.pay_date AND r.input_digest IS NOT NULL AND j.status='posted') THEN
  RAISE EXCEPTION 'Payslip requires the exact independently posted payroll item and journal' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END; $$;
CREATE CONSTRAINT TRIGGER hr_issued_payslip_posted_source AFTER INSERT ON hr_issued_payslips DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION hr_issued_payslip_posted_source();
