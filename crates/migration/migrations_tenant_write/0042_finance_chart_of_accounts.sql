-- Normalized, authoritative Chart of Accounts source state. This is additive
-- to the early finance_charts JSON compatibility table; new integrations use
-- these rows and the complete shared outbox envelope.

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_chart_of_accounts (
    holding_ref TEXT NOT NULL
        CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    chart_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    chart_name TEXT NOT NULL
        CHECK (btrim(chart_name) <> '' AND octet_length(chart_name) <= 255),
    version BIGINT NOT NULL CHECK (version > 0),
    last_command_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL
        CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 512),
    actor_ref TEXT NOT NULL
        CHECK (btrim(actor_ref) <> '' AND octet_length(actor_ref) <= 255),
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, chart_id),
    UNIQUE (holding_ref, holding_id, company_id),
    UNIQUE (holding_ref, chart_id, holding_id, company_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_chart_accounts (
    holding_ref TEXT NOT NULL
        CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    chart_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    account_id UUID NOT NULL,
    account_code TEXT NOT NULL
        CHECK (
            btrim(account_code) <> ''
            AND octet_length(account_code) <= 64
            AND account_code ~ '^[A-Z0-9._-]+$'
        ),
    account_name TEXT NOT NULL
        CHECK (btrim(account_name) <> '' AND octet_length(account_name) <= 255),
    category TEXT NOT NULL
        CHECK (category IN ('asset', 'liability', 'equity', 'revenue', 'expense')),
    normal_balance TEXT NOT NULL
        CHECK (normal_balance IN ('debit', 'credit')),
    parent_account_id UUID,
    status TEXT NOT NULL CHECK (status IN ('active', 'inactive')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, chart_id, account_id),
    UNIQUE (holding_ref, company_id, account_code),
    FOREIGN KEY (holding_ref, chart_id, holding_id, company_id)
        REFERENCES erpx_tenant.finance_chart_of_accounts
            (holding_ref, chart_id, holding_id, company_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (holding_ref, chart_id, parent_account_id)
        REFERENCES erpx_tenant.finance_chart_accounts
            (holding_ref, chart_id, account_id)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_chart_command_idempotency (
    holding_ref TEXT NOT NULL
        CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    idempotency_key TEXT NOT NULL
        CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 512),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    chart_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, idempotency_key),
    FOREIGN KEY (holding_ref, chart_id)
        REFERENCES erpx_tenant.finance_chart_of_accounts (holding_ref, chart_id)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS finance_chart_accounts_hierarchy_idx
    ON erpx_tenant.finance_chart_accounts
        (holding_ref, chart_id, parent_account_id, account_code);
CREATE INDEX IF NOT EXISTS finance_chart_idempotency_aggregate_idx
    ON erpx_tenant.finance_chart_command_idempotency
        (holding_ref, chart_id, aggregate_version);

ALTER TABLE erpx_tenant.finance_chart_of_accounts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_chart_of_accounts FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_chart_accounts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_chart_accounts FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_chart_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_chart_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_chart_of_accounts
    ON erpx_tenant.finance_chart_of_accounts;
CREATE POLICY tenant_scope_finance_chart_of_accounts
    ON erpx_tenant.finance_chart_of_accounts
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_finance_chart_accounts
    ON erpx_tenant.finance_chart_accounts;
CREATE POLICY tenant_scope_finance_chart_accounts
    ON erpx_tenant.finance_chart_accounts
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_finance_chart_idempotency
    ON erpx_tenant.finance_chart_command_idempotency;
CREATE POLICY tenant_scope_finance_chart_idempotency
    ON erpx_tenant.finance_chart_command_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

CREATE OR REPLACE FUNCTION erpx_tenant.protect_finance_chart_identity()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Finance chart identities are immutable';
    END IF;
    IF NEW.holding_ref <> OLD.holding_ref
       OR NEW.chart_id <> OLD.chart_id
       OR NEW.holding_id <> OLD.holding_id
       OR NEW.company_id <> OLD.company_id THEN
        RAISE EXCEPTION 'Finance chart scope and identity are immutable';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_chart_identity_immutable
    ON erpx_tenant.finance_chart_of_accounts;
CREATE TRIGGER finance_chart_identity_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.finance_chart_of_accounts
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.protect_finance_chart_identity();

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
    ) AND EXISTS (
        SELECT 1
          FROM erpx_tenant.finance_posted_journals journal
          CROSS JOIN LATERAL jsonb_array_elements(
              COALESCE(journal.payload #> '{entry,lines}', '[]'::jsonb)
          ) AS line
         WHERE journal.holding_ref = OLD.holding_ref
           AND journal.company_id = OLD.company_id
           AND line ->> 'account_id' = OLD.account_id::text
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

CREATE OR REPLACE FUNCTION erpx_tenant.reject_finance_chart_cycle()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    cycle_found BOOLEAN;
BEGIN
    WITH RECURSIVE ancestry AS (
        SELECT account_id,
               parent_account_id,
               ARRAY[account_id] AS path,
               FALSE AS cycle
          FROM erpx_tenant.finance_chart_accounts
         WHERE holding_ref = NEW.holding_ref
           AND chart_id = NEW.chart_id
        UNION ALL
        SELECT parent.account_id,
               parent.parent_account_id,
               ancestry.path || parent.account_id,
               parent.account_id = ANY(ancestry.path)
          FROM ancestry
          JOIN erpx_tenant.finance_chart_accounts parent
            ON parent.holding_ref = NEW.holding_ref
           AND parent.chart_id = NEW.chart_id
           AND parent.account_id = ancestry.parent_account_id
         WHERE NOT ancestry.cycle
    )
    SELECT EXISTS (SELECT 1 FROM ancestry WHERE cycle) INTO cycle_found;
    IF cycle_found THEN
        RAISE EXCEPTION 'Finance account hierarchy contains a cycle';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_chart_hierarchy_acyclic
    ON erpx_tenant.finance_chart_accounts;
CREATE CONSTRAINT TRIGGER finance_chart_hierarchy_acyclic
    AFTER INSERT OR UPDATE OF parent_account_id
    ON erpx_tenant.finance_chart_accounts
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_finance_chart_cycle();
