-- Marketing Campaign Budget command guards. Marketing owns planning,
-- allocation, and spend reservations; Procurement/Finance remain owners of
-- commitments, invoices, payments, and posted accounting amounts.

CREATE SCHEMA IF NOT EXISTS erpx_marketing_write;

CREATE TABLE erpx_marketing_write.campaign_budget_guards (
    budget_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    campaign_id UUID NOT NULL,
    budget_code TEXT NOT NULL,
    budget_status TEXT NOT NULL CHECK (budget_status IN (
        'draft', 'review', 'approved', 'running', 'completed', 'cancelled'
    )),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    requested_minor BIGINT NOT NULL CHECK (requested_minor > 0),
    approved_minor BIGINT CHECK (approved_minor > 0),
    reserved_minor BIGINT NOT NULL CHECK (reserved_minor >= 0),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, budget_code),
    CHECK (approved_minor IS NULL OR reserved_minor <= approved_minor)
);

CREATE INDEX marketing_budget_scope_campaign_status
    ON erpx_marketing_write.campaign_budget_guards
        (scope_key, campaign_id, budget_status, updated_at DESC, budget_id DESC);

CREATE TABLE erpx_marketing_write.campaign_spend_request_guards (
    request_id UUID PRIMARY KEY,
    budget_id UUID NOT NULL,
    allocation_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    expected_outcome TEXT NOT NULL,
    spend_status TEXT NOT NULL CHECK (spend_status IN ('active', 'released')),
    requested_at TIMESTAMPTZ NOT NULL,
    released_at TIMESTAMPTZ,
    budget_aggregate_version BIGINT NOT NULL CHECK (budget_aggregate_version > 0),
    FOREIGN KEY (budget_id)
        REFERENCES erpx_marketing_write.campaign_budget_guards (budget_id)
        ON DELETE RESTRICT,
    CHECK (
        (spend_status = 'active' AND released_at IS NULL)
        OR (spend_status = 'released' AND released_at >= requested_at)
    )
);

CREATE INDEX marketing_spend_request_budget_status
    ON erpx_marketing_write.campaign_spend_request_guards
        (budget_id, spend_status, requested_at, request_id);
