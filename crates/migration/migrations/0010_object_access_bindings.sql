-- Authoritative binding for every signed object key. A presigned upload first
-- creates this row; downloads require the existing row. The namespace/key
-- uniqueness prevents an object from being rebound to another customer or
-- organization scope.
CREATE TABLE IF NOT EXISTS erpx_control.object_access_bindings (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    storage_namespace TEXT NOT NULL,
    object_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    created_by TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active', 'revoked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (branch_id IS NULL OR company_id IS NOT NULL),
    UNIQUE (storage_namespace, object_key)
);

CREATE INDEX IF NOT EXISTS object_access_bindings_customer_scope_idx
    ON erpx_control.object_access_bindings
        (customer_id, holding_id, company_id, branch_id, status);
