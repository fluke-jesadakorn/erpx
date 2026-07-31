-- Authoritative control-plane bindings between customer accounts and the
-- organization scopes that may address their tenant data. A NULL company or
-- branch is a deliberately explicit wildcard at that hierarchy level.
CREATE TABLE IF NOT EXISTS erpx_control.customer_scope_bindings (
    customer_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (branch_id IS NULL OR company_id IS NOT NULL),
    UNIQUE (customer_id, holding_id, company_id, branch_id)
);

CREATE INDEX IF NOT EXISTS customer_scope_bindings_lookup_idx
    ON erpx_control.customer_scope_bindings
        (customer_id, holding_id, company_id, branch_id);

CREATE UNIQUE INDEX IF NOT EXISTS customer_scope_bindings_null_safe_uq
    ON erpx_control.customer_scope_bindings
        (customer_id, holding_id, coalesce(company_id, '00000000-0000-0000-0000-000000000000'::uuid),
         coalesce(branch_id, '00000000-0000-0000-0000-000000000000'::uuid));
