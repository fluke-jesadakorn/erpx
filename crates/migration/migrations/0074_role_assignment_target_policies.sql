-- Explicit authorization targets for local role assignments. Every existing
-- assignment is backfilled as unrestricted to preserve production behavior;
-- new authorization code denies assignments with no policy row.

CREATE TABLE IF NOT EXISTS erpx_control.role_assignment_target_policies (
    assignment_id UUID PRIMARY KEY
        REFERENCES erpx_control.role_assignments(id) ON DELETE RESTRICT,
    policy_kind TEXT NOT NULL
        CHECK (policy_kind IN ('unrestricted', 'tenant_allowlist')),
    environment_gate TEXT,
    configured_by TEXT NOT NULL,
    configured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (length(btrim(configured_by)) BETWEEN 1 AND 255),
    CHECK (
        (policy_kind = 'unrestricted' AND environment_gate IS NULL)
        OR
        (policy_kind = 'tenant_allowlist'
         AND environment_gate = 'local-ucs-lab')
    )
);

CREATE TABLE IF NOT EXISTS erpx_control.role_assignment_tenant_targets (
    assignment_id UUID NOT NULL
        REFERENCES erpx_control.role_assignment_target_policies(assignment_id)
        ON DELETE CASCADE,
    tenant_id TEXT NOT NULL,
    configured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (assignment_id, tenant_id),
    CHECK (length(btrim(tenant_id)) BETWEEN 1 AND 128),
    CHECK (tenant_id !~ '[[:cntrl:]]')
);

CREATE INDEX IF NOT EXISTS role_assignment_tenant_targets_tenant_idx
    ON erpx_control.role_assignment_tenant_targets (tenant_id, assignment_id);

INSERT INTO erpx_control.role_assignment_target_policies
    (assignment_id, policy_kind, environment_gate, configured_by)
SELECT id, 'unrestricted', NULL, 'migration:0074'
FROM erpx_control.role_assignments
ON CONFLICT (assignment_id) DO NOTHING;

-- The existing audit table is the authoritative, payload-free source for the
-- tenant control history cursor.
CREATE INDEX IF NOT EXISTS tenant_platform_access_audit_history_cursor_idx
    ON erpx_control.tenant_platform_access_audit
        (tenant_id, occurred_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS control_outbox_tenant_history_cursor_idx
    ON erpx_control.outbox_events
        ((tenant_scope->>'tenant_id'), occurred_at DESC, id DESC)
    WHERE tenant_scope ? 'tenant_id';
