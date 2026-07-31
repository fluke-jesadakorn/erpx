CREATE SCHEMA IF NOT EXISTS erpx_control;

-- Rebuildable projection storage only. Aggregate snapshots, outbox events,
-- role assignments, identity, and command receipts exist on the write side.
CREATE TABLE IF NOT EXISTS erpx_control.customers (
    id UUID PRIMARY KEY,
    account_code TEXT NOT NULL UNIQUE,
    trading_name TEXT NOT NULL,
    legal_name TEXT,
    country TEXT NOT NULL,
    status TEXT NOT NULL,
    billing_state TEXT NOT NULL,
    search_document TSVECTOR GENERATED ALWAYS AS (
        to_tsvector('simple', coalesce(account_code, '') || ' ' || coalesce(trading_name, '') || ' ' || coalesce(legal_name, ''))
    ) STORED,
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS customers_search_document_idx
    ON erpx_control.customers USING GIN (search_document);
