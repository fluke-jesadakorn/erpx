-- Journal posting records the exact normalized Chart of Accounts rows used by
-- each immutable journal. This is a write-side fact, never a read projection,
-- and is inserted in the same transaction as the journal and outbox event.
CREATE TABLE IF NOT EXISTS erpx_tenant.finance_posted_journal_account_usage (
    holding_ref TEXT NOT NULL
        CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    journal_id UUID NOT NULL
        REFERENCES erpx_tenant.finance_posted_journals (journal_id)
        ON DELETE RESTRICT,
    chart_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    account_id UUID NOT NULL,
    chart_version BIGINT NOT NULL CHECK (chart_version > 0),
    line_count BIGINT NOT NULL CHECK (line_count > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, journal_id, account_id),
    FOREIGN KEY (holding_ref, chart_id, account_id)
        REFERENCES erpx_tenant.finance_chart_accounts
            (holding_ref, chart_id, account_id)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS finance_journal_account_usage_account_idx
    ON erpx_tenant.finance_posted_journal_account_usage
        (holding_ref, chart_id, company_id, account_id, created_at);

ALTER TABLE erpx_tenant.finance_posted_journal_account_usage ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_posted_journal_account_usage FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_journal_account_usage
    ON erpx_tenant.finance_posted_journal_account_usage;
CREATE POLICY tenant_scope_finance_journal_account_usage
    ON erpx_tenant.finance_posted_journal_account_usage
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

CREATE OR REPLACE FUNCTION erpx_tenant.reject_finance_journal_account_usage_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION 'Finance journal account usage facts are immutable';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_journal_account_usage_immutable
    ON erpx_tenant.finance_posted_journal_account_usage;
CREATE TRIGGER finance_journal_account_usage_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.finance_posted_journal_account_usage
    FOR EACH ROW
    EXECUTE FUNCTION erpx_tenant.reject_finance_journal_account_usage_mutation();

-- Extend the existing structural guard to use the normalized, durable usage
-- fact. Keep the original payload check for journals created before this
-- additive migration so historical rows remain protected during upgrade.
CREATE OR REPLACE FUNCTION erpx_tenant.protect_finance_account_identity_and_usage()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Finance account identities are immutable';
    END IF;
    IF NEW.holding_ref <> OLD.holding_ref
       OR NEW.chart_id <> OLD.chart_id
       OR NEW.holding_id <> OLD.holding_id
       OR NEW.company_id <> OLD.company_id
       OR NEW.account_id <> OLD.account_id THEN
        RAISE EXCEPTION 'Finance account scope and identity are immutable';
    END IF;
    IF (
        NEW.account_code IS DISTINCT FROM OLD.account_code
        OR NEW.account_name IS DISTINCT FROM OLD.account_name
        OR NEW.category IS DISTINCT FROM OLD.category
        OR NEW.normal_balance IS DISTINCT FROM OLD.normal_balance
        OR NEW.parent_account_id IS DISTINCT FROM OLD.parent_account_id
    ) AND (
        EXISTS (
            SELECT 1
              FROM erpx_tenant.finance_posted_journal_account_usage usage_fact
             WHERE usage_fact.holding_ref = OLD.holding_ref
               AND usage_fact.chart_id = OLD.chart_id
               AND usage_fact.account_id = OLD.account_id
        )
        OR EXISTS (
            SELECT 1
              FROM erpx_tenant.finance_posted_journals journal
              CROSS JOIN LATERAL jsonb_array_elements(
                  COALESCE(journal.payload #> '{entry,lines}', '[]'::jsonb)
              ) AS line
             WHERE journal.holding_ref = OLD.holding_ref
               AND journal.company_id = OLD.company_id
               AND line ->> 'account_id' = OLD.account_id::text
        )
    ) THEN
        RAISE EXCEPTION 'posted/used Finance accounts are structurally immutable';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_chart_account_identity_and_usage
    ON erpx_tenant.finance_chart_accounts;
CREATE TRIGGER finance_chart_account_identity_and_usage
    BEFORE UPDATE OR DELETE ON erpx_tenant.finance_chart_accounts
    FOR EACH ROW
    EXECUTE FUNCTION erpx_tenant.protect_finance_account_identity_and_usage();
