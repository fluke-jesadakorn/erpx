-- Authoritative close-control results for one routed holding.  Close review
-- input may describe the controls that were observed, but it cannot create a
-- resolved result: every submitted flag must match one of these rows.
CREATE TABLE IF NOT EXISTS erpx_tenant.finance_close_control_evidence (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    period_id UUID NOT NULL,
    control_code TEXT NOT NULL CHECK (btrim(control_code) <> ''),
    source_type TEXT NOT NULL CHECK (btrim(source_type) <> ''),
    source_aggregate_id UUID NOT NULL,
    evidence_ref TEXT NOT NULL CHECK (btrim(evidence_ref) <> ''),
    resolved BOOLEAN NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, company_id, period_id, control_code)
);

CREATE INDEX IF NOT EXISTS finance_close_control_evidence_scope_idx
    ON erpx_tenant.finance_close_control_evidence
        (holding_ref, holding_id, company_id, period_id, control_code);

ALTER TABLE erpx_tenant.finance_close_control_evidence ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_close_control_evidence FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_close_control_evidence
    ON erpx_tenant.finance_close_control_evidence;
CREATE POLICY tenant_scope_finance_close_control_evidence
    ON erpx_tenant.finance_close_control_evidence
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

CREATE OR REPLACE FUNCTION erpx_tenant.reject_closed_finance_close_control_evidence_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF EXISTS (
            SELECT 1
            FROM erpx_tenant.finance_close_reviews review
            WHERE review.holding_ref = OLD.holding_ref
              AND review.company_id = OLD.company_id
              AND review.period_id = OLD.period_id
              AND review.status = 'closed'
        ) THEN
            RAISE EXCEPTION 'close control evidence is immutable after period close';
        END IF;
        RETURN OLD;
    END IF;
    IF EXISTS (
        SELECT 1
        FROM erpx_tenant.finance_close_reviews review
        WHERE (
            (review.holding_ref = OLD.holding_ref
             AND review.company_id = OLD.company_id
             AND review.period_id = OLD.period_id)
            OR (review.holding_ref = NEW.holding_ref
                AND review.company_id = NEW.company_id
                AND review.period_id = NEW.period_id)
        )
          AND review.status = 'closed'
    ) THEN
        RAISE EXCEPTION 'close control evidence is immutable after period close';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_close_control_evidence_closed_immutable
    ON erpx_tenant.finance_close_control_evidence;
CREATE TRIGGER finance_close_control_evidence_closed_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.finance_close_control_evidence
    FOR EACH ROW
    EXECUTE FUNCTION erpx_tenant.reject_closed_finance_close_control_evidence_mutation();

CREATE OR REPLACE FUNCTION erpx_tenant.reject_closed_finance_close_control_evidence_insert()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM erpx_tenant.finance_close_reviews review
        WHERE review.holding_ref = NEW.holding_ref
          AND review.company_id = NEW.company_id
          AND review.period_id = NEW.period_id
          AND review.status = 'closed'
    ) THEN
        RAISE EXCEPTION 'close control evidence is immutable after period close';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_close_control_evidence_closed_insert
    ON erpx_tenant.finance_close_control_evidence;
CREATE TRIGGER finance_close_control_evidence_closed_insert
    BEFORE INSERT ON erpx_tenant.finance_close_control_evidence
    FOR EACH ROW
    EXECUTE FUNCTION erpx_tenant.reject_closed_finance_close_control_evidence_insert();
