-- Company-scoped Finance period-close reviews. The domain aggregate uses the
-- accounting period as its identity; one holding can therefore have one
-- review per period. Journal facts remain owned by Finance posting.
CREATE TABLE IF NOT EXISTS erpx_tenant.finance_close_reviews (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    period_id UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('open', 'review', 'closed')),
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    opened_by TEXT NOT NULL CHECK (btrim(opened_by) <> ''),
    reviewed_by TEXT,
    closed_by TEXT,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, period_id),
    UNIQUE (holding_ref, idempotency_key)
);

CREATE INDEX IF NOT EXISTS finance_close_reviews_company_idx
    ON erpx_tenant.finance_close_reviews
        (holding_ref, holding_id, company_id, status, updated_at DESC);

ALTER TABLE erpx_tenant.finance_close_reviews ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_close_reviews FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_close_reviews
    ON erpx_tenant.finance_close_reviews;
CREATE POLICY tenant_scope_finance_close_reviews
    ON erpx_tenant.finance_close_reviews
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

CREATE OR REPLACE FUNCTION erpx_tenant.reject_closed_finance_close_review_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status = 'closed' THEN
        RAISE EXCEPTION 'closed Finance close reviews are immutable';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_close_reviews_closed_immutable
    ON erpx_tenant.finance_close_reviews;
CREATE TRIGGER finance_close_reviews_closed_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.finance_close_reviews
    FOR EACH ROW
    EXECUTE FUNCTION erpx_tenant.reject_closed_finance_close_review_mutation();
