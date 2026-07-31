CREATE TABLE IF NOT EXISTS erpx_control.tenant_owner_seeds (
    tenant_id TEXT PRIMARY KEY
        REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE RESTRICT,
    holding_ref TEXT NOT NULL,
    holding_id UUID NOT NULL,
    principal_ref TEXT NOT NULL,
    approver_ref TEXT NOT NULL,
    role_assignment_id UUID NOT NULL UNIQUE,
    state TEXT NOT NULL CHECK (state IN ('pending', 'seeded')),
    recovery_issued_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (btrim(holding_ref) <> ''),
    CHECK (btrim(principal_ref) <> ''),
    CHECK (btrim(approver_ref) <> ''),
    CHECK (principal_ref <> approver_ref)
);

CREATE UNIQUE INDEX IF NOT EXISTS tenant_owner_seeds_principal_scope_idx
    ON erpx_control.tenant_owner_seeds (principal_ref, holding_id);

CREATE INDEX IF NOT EXISTS tenant_owner_seeds_state_idx
    ON erpx_control.tenant_owner_seeds (state, updated_at, tenant_id);
