CREATE TABLE finance_batch_review_items (
  command_id TEXT NOT NULL REFERENCES command_receipts(command_id),
  journal_id TEXT NOT NULL REFERENCES finance_journals(id),
  company_id TEXT NOT NULL,
  reviewed_version BIGINT NOT NULL,
  reviewed_by TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  evidence_digest TEXT NOT NULL,
  reason TEXT NOT NULL,
  reviewed_at BIGINT NOT NULL,
  PRIMARY KEY(command_id,journal_id)
);

CREATE TRIGGER finance_batch_review_immutable BEFORE UPDATE OR DELETE ON finance_batch_review_items
  FOR EACH ROW EXECUTE FUNCTION protect_financial_event();

CREATE FUNCTION protect_reviewed_accounting_profile() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.status IN ('approved','rejected') THEN
    RAISE EXCEPTION 'ACCOUNTING_PROFILE_IMMUTABLE' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END;
$$;
CREATE TRIGGER finance_accounting_profile_immutable BEFORE UPDATE OR DELETE ON finance_accounting_profiles
  FOR EACH ROW EXECUTE FUNCTION protect_reviewed_accounting_profile();

CREATE FUNCTION protect_posted_line_evidence() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF EXISTS(SELECT 1 FROM finance_journals WHERE id=OLD.journal_id AND status IN ('posted','reversed')) THEN
    RAISE EXCEPTION 'POSTED_JOURNAL_LINES_IMMUTABLE' USING ERRCODE='23514';
  END IF;
  IF TG_OP='DELETE' THEN RETURN OLD; END IF;
  RETURN NEW;
END;
$$;
CREATE TRIGGER finance_posted_line_evidence BEFORE UPDATE OR DELETE ON finance_journal_lines
  FOR EACH ROW EXECUTE FUNCTION protect_posted_line_evidence();

DO $$
DECLARE app_role TEXT := regexp_replace(current_user,'_migration$','_app');
BEGIN
  IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
    EXECUTE format('GRANT SELECT,INSERT ON finance_batch_review_items TO %I',app_role);
  END IF;
END;
$$;

INSERT INTO work_cache_revisions(table_name) VALUES ('finance_batch_review_items') ON CONFLICT DO NOTHING;
