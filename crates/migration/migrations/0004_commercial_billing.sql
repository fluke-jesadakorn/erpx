CREATE TABLE IF NOT EXISTS erpx_control.commercial_catalog_revisions (
    id UUID PRIMARY KEY,
    revision_number INTEGER NOT NULL UNIQUE CHECK (revision_number > 0),
    published_at TIMESTAMPTZ NOT NULL,
    published_by TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_control.grandfathered_price_migrations (
    id UUID PRIMARY KEY,
    customer_id UUID NOT NULL,
    subscription_id UUID NOT NULL,
    item_key TEXT NOT NULL,
    from_revision INTEGER NOT NULL CHECK (from_revision > 0),
    to_revision INTEGER NOT NULL CHECK (to_revision > from_revision),
    effective_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (customer_id, subscription_id, item_key, from_revision, to_revision)
);

CREATE TABLE IF NOT EXISTS erpx_control.billing_lifecycles (
    customer_id UUID NOT NULL,
    subscription_id UUID NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('trialing', 'active', 'past_due', 'grace', 'read_only', 'canceled')),
    grace_started_at TIMESTAMPTZ,
    grace_until TIMESTAMPTZ,
    last_transition_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (customer_id, subscription_id)
);

CREATE TABLE IF NOT EXISTS erpx_control.entitlement_allocation_books (
    customer_id UUID NOT NULL,
    module_key TEXT NOT NULL,
    active BOOLEAN NOT NULL DEFAULT FALSE,
    active_until TIMESTAMPTZ,
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (customer_id, module_key)
);

CREATE TABLE IF NOT EXISTS erpx_control.stripe_event_inbox (
    event_id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    stripe_created_at TIMESTAMPTZ,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    payload JSONB NOT NULL,
    raw_body BYTEA NOT NULL,
    status TEXT NOT NULL DEFAULT 'received'
        CHECK (status IN ('received', 'processing', 'processed', 'retryable', 'dead_letter')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    claimed_by UUID,
    claimed_at TIMESTAMPTZ,
    claim_expires_at TIMESTAMPTZ,
    processed_at TIMESTAMPTZ,
    dead_lettered_at TIMESTAMPTZ,
    CHECK (
        status <> 'processing'
        OR (claimed_by IS NOT NULL AND claimed_at IS NOT NULL AND claim_expires_at IS NOT NULL)
    ),
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS stripe_event_inbox_ready_idx
    ON erpx_control.stripe_event_inbox (available_at, received_at, event_id)
    WHERE status IN ('received', 'retryable');

CREATE INDEX IF NOT EXISTS stripe_event_inbox_claim_expiry_idx
    ON erpx_control.stripe_event_inbox (claim_expires_at, received_at, event_id)
    WHERE status = 'processing';

CREATE INDEX IF NOT EXISTS billing_lifecycles_state_idx
    ON erpx_control.billing_lifecycles (state, updated_at);
