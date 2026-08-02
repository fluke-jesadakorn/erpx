-- Three-layer SaaS control-plane bindings and short-lived entered-tenant
-- administration. This migration is additive: it performs no ownership
-- guessing, tenant activation, billing transition, suspension, or deletion.

CREATE TABLE IF NOT EXISTS erpx_control.customer_tenant_bindings (
    customer_id UUID PRIMARY KEY
        REFERENCES erpx_control.customers(id) ON DELETE RESTRICT,
    tenant_id TEXT NOT NULL UNIQUE
        REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE RESTRICT,
    root_holding_id UUID NOT NULL UNIQUE,
    holding_ref TEXT NOT NULL,
    binding_state TEXT NOT NULL DEFAULT 'configured'
        CHECK (binding_state IN ('configured', 'configuration_required')),
    bound_by TEXT NOT NULL,
    bound_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (length(btrim(tenant_id)) BETWEEN 1 AND 128),
    CHECK (length(btrim(holding_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(bound_by)) BETWEEN 1 AND 255),
    UNIQUE (customer_id, tenant_id),
    UNIQUE (customer_id, tenant_id, root_holding_id)
);

-- The client receives a random opaque UUID. Only its SHA-256 digest is stored,
-- so database reads cannot recover an active bearer value.
CREATE TABLE IF NOT EXISTS erpx_control.platform_tenant_admin_contexts (
    context_id_hash BYTEA PRIMARY KEY CHECK (octet_length(context_id_hash) = 32),
    principal_ref TEXT NOT NULL,
    device_id UUID NOT NULL,
    session_id UUID NOT NULL,
    tenant_id TEXT NOT NULL
        REFERENCES erpx_control.customer_tenant_bindings(tenant_id) ON DELETE RESTRICT,
    customer_id UUID NOT NULL,
    root_holding_id UUID NOT NULL,
    reason TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    revocation_reason TEXT,
    CHECK (expires_at = issued_at + INTERVAL '30 minutes'),
    CHECK (length(btrim(principal_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(reason)) BETWEEN 1 AND 512),
    CHECK ((revoked_at IS NULL) = (revocation_reason IS NULL)),
    FOREIGN KEY (customer_id, tenant_id, root_holding_id)
        REFERENCES erpx_control.customer_tenant_bindings
            (customer_id, tenant_id, root_holding_id)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS platform_tenant_admin_context_active_idx
    ON erpx_control.platform_tenant_admin_contexts
        (principal_ref, device_id, session_id, tenant_id, expires_at)
    WHERE revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS erpx_control.stripe_subscription_bindings (
    customer_id UUID NOT NULL,
    tenant_id TEXT NOT NULL,
    subscription_id UUID NOT NULL,
    stripe_customer_ref TEXT NOT NULL UNIQUE,
    stripe_subscription_ref TEXT UNIQUE,
    currency CHAR(3) NOT NULL,
    item_quantities JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (customer_id, subscription_id),
    FOREIGN KEY (customer_id, tenant_id)
        REFERENCES erpx_control.customer_tenant_bindings (customer_id, tenant_id)
        ON DELETE RESTRICT,
    CHECK (currency ~ '^[A-Z]{3}$'),
    CHECK (jsonb_typeof(item_quantities) = 'object'),
    CHECK (length(btrim(stripe_customer_ref)) BETWEEN 1 AND 255),
    CHECK (stripe_subscription_ref IS NULL OR length(btrim(stripe_subscription_ref)) BETWEEN 1 AND 255)
);

-- Checkout resolves these private provider identifiers from the published
-- catalog revision. They are never accepted from, or returned to, a UI.
CREATE TABLE IF NOT EXISTS erpx_control.stripe_catalog_price_bindings (
    catalog_revision INTEGER NOT NULL
        REFERENCES erpx_control.commercial_catalog_revisions(revision_number)
        ON DELETE RESTRICT,
    item_key TEXT NOT NULL,
    stripe_product_ref TEXT NOT NULL,
    stripe_price_ref TEXT NOT NULL UNIQUE,
    currency CHAR(3) NOT NULL,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    bound_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (catalog_revision, item_key),
    CHECK (catalog_revision > 0),
    CHECK (length(btrim(item_key)) BETWEEN 1 AND 255),
    CHECK (stripe_product_ref ~ '^prod_[A-Za-z0-9_]+$'),
    CHECK (stripe_price_ref ~ '^price_[A-Za-z0-9_]+$'),
    CHECK (currency ~ '^[A-Z]{3}$')
);

CREATE TABLE IF NOT EXISTS erpx_control.billing_grace_extensions (
    id UUID PRIMARY KEY,
    customer_id UUID NOT NULL,
    subscription_id UUID NOT NULL,
    delinquency_cycle_id UUID NOT NULL,
    previous_grace_until TIMESTAMPTZ NOT NULL,
    extended_until TIMESTAMPTZ NOT NULL,
    granted_at TIMESTAMPTZ NOT NULL,
    granted_by TEXT NOT NULL,
    reason TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    UNIQUE (customer_id, subscription_id, delinquency_cycle_id),
    FOREIGN KEY (customer_id, subscription_id)
        REFERENCES erpx_control.billing_lifecycles (customer_id, subscription_id)
        ON DELETE RESTRICT,
    CHECK (extended_until > previous_grace_until),
    CHECK (extended_until <= previous_grace_until + INTERVAL '7 days'),
    CHECK (length(btrim(granted_by)) BETWEEN 1 AND 255),
    CHECK (length(btrim(reason)) BETWEEN 1 AND 512)
);

CREATE TABLE IF NOT EXISTS erpx_control.tenant_platform_access_audit (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    customer_id UUID NOT NULL,
    root_holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    platform_actor_ref TEXT NOT NULL,
    module_key TEXT NOT NULL,
    action TEXT NOT NULL,
    reason TEXT NOT NULL,
    administrative_context_hash BYTEA NOT NULL
        CHECK (octet_length(administrative_context_hash) = 32),
    correlation_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (customer_id, tenant_id, root_holding_id)
        REFERENCES erpx_control.customer_tenant_bindings
            (customer_id, tenant_id, root_holding_id)
        ON DELETE RESTRICT,
    CHECK (branch_id IS NULL OR company_id IS NOT NULL),
    CHECK (length(btrim(platform_actor_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(module_key)) BETWEEN 1 AND 128),
    CHECK (length(btrim(action)) BETWEEN 1 AND 255),
    CHECK (length(btrim(reason)) BETWEEN 1 AND 512)
);

CREATE INDEX IF NOT EXISTS tenant_platform_access_audit_tenant_idx
    ON erpx_control.tenant_platform_access_audit
        (tenant_id, occurred_at DESC, id DESC);

-- The all-tenant registry uses a disposable control-plane projection. It holds
-- bounded operational/commercial summaries only and contains no tenant
-- business records or tenant-database routing secrets.
CREATE TABLE IF NOT EXISTS erpx_control.tenant_summary_projection (
    tenant_id TEXT PRIMARY KEY,
    customer_id UUID,
    root_holding_id UUID,
    customer_name TEXT NOT NULL DEFAULT '',
    tenant_state TEXT NOT NULL,
    configuration_state TEXT NOT NULL
        CHECK (configuration_state IN ('configured', 'configuration_required')),
    subscription_state TEXT NOT NULL DEFAULT 'unconfigured',
    plan_key TEXT NOT NULL DEFAULT '',
    enabled_modules TEXT[] NOT NULL DEFAULT '{}',
    purchased_seats INTEGER NOT NULL DEFAULT 0 CHECK (purchased_seats >= 0),
    used_seats INTEGER NOT NULL DEFAULT 0 CHECK (used_seats >= 0),
    company_count INTEGER NOT NULL DEFAULT 0 CHECK (company_count >= 0),
    branch_count INTEGER NOT NULL DEFAULT 0 CHECK (branch_count >= 0),
    renewal_at TIMESTAMPTZ,
    overdue BOOLEAN NOT NULL DEFAULT FALSE,
    projection_freshness TEXT NOT NULL DEFAULT 'unknown',
    storage_state TEXT NOT NULL DEFAULT 'unknown',
    backup_state TEXT NOT NULL DEFAULT 'unknown',
    integration_health TEXT NOT NULL DEFAULT 'unknown',
    health TEXT NOT NULL DEFAULT 'unknown',
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (used_seats <= purchased_seats OR configuration_state = 'configuration_required')
);

INSERT INTO erpx_control.tenant_summary_projection
    (tenant_id, tenant_state, configuration_state)
SELECT registry.tenant_id, registry.state,
       CASE WHEN binding.customer_id IS NULL THEN 'configuration_required' ELSE 'configured' END
FROM erpx_control.tenant_registry AS registry
LEFT JOIN erpx_control.customer_tenant_bindings AS binding
  ON binding.tenant_id = registry.tenant_id
ON CONFLICT (tenant_id) DO NOTHING;

CREATE OR REPLACE FUNCTION erpx_control.refresh_tenant_summary_identity()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    selected_tenant_id TEXT;
BEGIN
    selected_tenant_id := COALESCE(NEW.tenant_id, OLD.tenant_id);
    INSERT INTO erpx_control.tenant_summary_projection
        (tenant_id, customer_id, root_holding_id, customer_name,
         tenant_state, configuration_state, projected_at)
    SELECT registry.tenant_id,
           binding.customer_id,
           binding.root_holding_id,
           COALESCE(customer.trading_name, customer.legal_name, ''),
           registry.state,
           CASE WHEN binding.customer_id IS NULL
                THEN 'configuration_required' ELSE 'configured' END,
           now()
    FROM erpx_control.tenant_registry AS registry
    LEFT JOIN erpx_control.customer_tenant_bindings AS binding
      ON binding.tenant_id = registry.tenant_id
    LEFT JOIN erpx_control.customers AS customer
      ON customer.id = binding.customer_id
    WHERE registry.tenant_id = selected_tenant_id
    ON CONFLICT (tenant_id) DO UPDATE
    SET customer_id = EXCLUDED.customer_id,
        root_holding_id = EXCLUDED.root_holding_id,
        customer_name = EXCLUDED.customer_name,
        tenant_state = EXCLUDED.tenant_state,
        configuration_state = EXCLUDED.configuration_state,
        projected_at = EXCLUDED.projected_at;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS tenant_registry_summary_refresh
    ON erpx_control.tenant_registry;
CREATE TRIGGER tenant_registry_summary_refresh
AFTER INSERT OR UPDATE OF state ON erpx_control.tenant_registry
FOR EACH ROW EXECUTE FUNCTION erpx_control.refresh_tenant_summary_identity();

DROP TRIGGER IF EXISTS customer_tenant_binding_summary_refresh
    ON erpx_control.customer_tenant_bindings;
CREATE TRIGGER customer_tenant_binding_summary_refresh
AFTER INSERT OR UPDATE ON erpx_control.customer_tenant_bindings
FOR EACH ROW EXECUTE FUNCTION erpx_control.refresh_tenant_summary_identity();

CREATE INDEX IF NOT EXISTS tenant_summary_projection_filter_idx
    ON erpx_control.tenant_summary_projection
        (configuration_state, tenant_state, subscription_state, health, tenant_id);

CREATE INDEX IF NOT EXISTS tenant_summary_projection_search_idx
    ON erpx_control.tenant_summary_projection
        USING GIN (to_tsvector('simple', customer_name || ' ' || tenant_id));

CREATE TABLE IF NOT EXISTS erpx_control.tenant_invoice_summary_projection (
    tenant_id TEXT NOT NULL,
    invoice_id TEXT NOT NULL,
    state TEXT NOT NULL,
    currency CHAR(3) NOT NULL,
    amount_due_minor BIGINT NOT NULL CHECK (amount_due_minor >= 0),
    amount_paid_minor BIGINT NOT NULL CHECK (amount_paid_minor >= 0),
    due_at TIMESTAMPTZ,
    hosted_invoice_available BOOLEAN NOT NULL DEFAULT FALSE,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, invoice_id),
    CHECK (currency ~ '^[A-Z]{3}$')
);

CREATE INDEX IF NOT EXISTS tenant_invoice_summary_projection_page_idx
    ON erpx_control.tenant_invoice_summary_projection
        (tenant_id, projected_at DESC, invoice_id DESC);
