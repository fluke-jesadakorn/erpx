-- Matching must compare against immutable persisted journal facts, not only
-- caller-supplied candidate values. Existing rows without these facts are
-- intentionally rejected by the matching adapter until backfilled by a
-- reviewed migration job.
ALTER TABLE erpx_tenant.finance_posted_journals
    ADD COLUMN IF NOT EXISTS currency CHAR(3),
    ADD COLUMN IF NOT EXISTS total_minor BIGINT;
ALTER TABLE erpx_tenant.finance_posted_journals
    DROP CONSTRAINT IF EXISTS finance_posted_journals_currency_check,
    ADD CONSTRAINT finance_posted_journals_currency_check
        CHECK (currency IS NULL OR currency ~ '^[A-Z]{3}$'),
    DROP CONSTRAINT IF EXISTS finance_posted_journals_total_check,
    ADD CONSTRAINT finance_posted_journals_total_check
        CHECK (total_minor IS NULL OR total_minor > 0);

CREATE OR REPLACE FUNCTION erpx_tenant.reject_posted_journal_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.currency IS NULL OR NEW.total_minor IS NULL THEN
            RAISE EXCEPTION 'posted Finance journal match facts are required';
        END IF;
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'posted Finance journals are immutable';
END;
$$;

DROP TRIGGER IF EXISTS finance_posted_journals_immutable
    ON erpx_tenant.finance_posted_journals;
CREATE TRIGGER finance_posted_journals_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON erpx_tenant.finance_posted_journals
    FOR EACH ROW
    EXECUTE FUNCTION erpx_tenant.reject_posted_journal_mutation();
