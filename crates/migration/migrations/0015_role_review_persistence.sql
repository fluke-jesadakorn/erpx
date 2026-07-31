-- G2-005 durable role-review projections.
--
-- These tables are derived review state only. The authoritative role
-- assignment aggregate remains erpx_control.role_assignments. There are no
-- credential, token, secret, or generic raw-payload columns in this schema.
-- Rollback: stop review persistence, verify no worker claim is in flight, then
-- drop the tables in reverse dependency order in a separately approved
-- migration. Never delete role_assignments as part of this rollback.

CREATE TABLE IF NOT EXISTS erpx_control.role_review_snapshots (
    snapshot_key TEXT PRIMARY KEY,
    scope_key TEXT NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('platform', 'holding', 'company', 'branch', 'subtree')),
    holding_id UUID,
    company_id UUID,
    branch_id UUID,
    scope_company_ids UUID[] NOT NULL DEFAULT '{}',
    scope_branch_ids UUID[] NOT NULL DEFAULT '{}',
    evaluated_at TIMESTAMPTZ NOT NULL,
    expiry_horizon_until TIMESTAMPTZ NOT NULL,
    snapshot_version BIGINT NOT NULL CHECK (snapshot_version > 0),
    content_hash TEXT NOT NULL CHECK (content_hash ~ '^[0-9a-f]{64}$'),
    item_count INTEGER NOT NULL CHECK (item_count >= 0 AND item_count <= 1000),
    actor_ref TEXT NOT NULL CHECK (length(btrim(actor_ref)) > 0),
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT role_review_snapshots_scope_shape CHECK (
        (scope_kind = 'platform'
            AND holding_id IS NULL AND company_id IS NULL AND branch_id IS NULL
            AND cardinality(scope_company_ids) = 0
            AND cardinality(scope_branch_ids) = 0)
        OR (scope_kind = 'holding'
            AND holding_id IS NOT NULL AND company_id IS NULL AND branch_id IS NULL
            AND cardinality(scope_company_ids) = 0
            AND cardinality(scope_branch_ids) = 0)
        OR (scope_kind = 'company'
            AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NULL
            AND cardinality(scope_company_ids) = 1
            AND cardinality(scope_branch_ids) = 0)
        OR (scope_kind = 'branch'
            AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NOT NULL
            AND cardinality(scope_company_ids) = 1
            AND cardinality(scope_branch_ids) = 1)
        OR (scope_kind = 'subtree'
            AND holding_id IS NOT NULL AND company_id IS NULL AND branch_id IS NULL
            AND cardinality(scope_company_ids) >= 1
            AND cardinality(scope_branch_ids) >= 0)
    ),
    CONSTRAINT role_review_snapshots_horizon_after_evaluation
        CHECK (expiry_horizon_until > evaluated_at)
);

CREATE INDEX IF NOT EXISTS role_review_snapshots_scope_idx
    ON erpx_control.role_review_snapshots (scope_kind, holding_id, company_id, branch_id, evaluated_at DESC);

CREATE INDEX IF NOT EXISTS role_review_snapshots_updated_idx
    ON erpx_control.role_review_snapshots (updated_at DESC, snapshot_key);

CREATE TABLE IF NOT EXISTS erpx_control.role_review_snapshot_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (length(btrim(idempotency_key)) > 0),
    snapshot_key TEXT NOT NULL REFERENCES erpx_control.role_review_snapshots(snapshot_key) ON DELETE CASCADE,
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    snapshot_version BIGINT NOT NULL CHECK (snapshot_version > 0),
    item_count INTEGER NOT NULL CHECK (item_count >= 0 AND item_count <= 1000),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS role_review_snapshot_idempotency_snapshot_idx
    ON erpx_control.role_review_snapshot_idempotency (snapshot_key, recorded_at DESC);

CREATE TABLE IF NOT EXISTS erpx_control.role_review_items (
    item_key TEXT PRIMARY KEY,
    snapshot_key TEXT NOT NULL REFERENCES erpx_control.role_review_snapshots(snapshot_key) ON DELETE CASCADE,
    assignment_id UUID NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('platform', 'holding', 'company', 'branch', 'subtree')),
    holding_id UUID,
    company_id UUID,
    branch_id UUID,
    scope_company_ids UUID[] NOT NULL DEFAULT '{}',
    scope_branch_ids UUID[] NOT NULL DEFAULT '{}',
    principal_ref TEXT NOT NULL CHECK (length(btrim(principal_ref)) > 0),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    parent_assignment_id UUID,
    assignment_version INTEGER NOT NULL CHECK (assignment_version > 0),
    classifications TEXT[] NOT NULL CHECK (cardinality(classifications) >= 1),
    notification_kind TEXT CHECK (notification_kind IS NULL OR notification_kind IN ('expiring_soon', 'expired')),
    notification_key TEXT,
    role_key TEXT NOT NULL CHECK (length(btrim(role_key)) > 0),
    permissions TEXT[] NOT NULL,
    functional_domain TEXT NOT NULL CHECK (length(btrim(functional_domain)) > 0),
    authority_level SMALLINT NOT NULL CHECK (authority_level >= 0),
    approval_limit_minor BIGINT NOT NULL CHECK (approval_limit_minor >= 0),
    timezone TEXT NOT NULL CHECK (length(btrim(timezone)) > 0),
    risk_tier TEXT NOT NULL CHECK (risk_tier IN ('platform_root', 'administrator', 'high', 'standard', 'contractor', 'support_diagnostic', 'support_mutation', 'emergency')),
    CONSTRAINT role_review_items_window_order CHECK (valid_until > valid_from),
    CONSTRAINT role_review_items_notification_pair CHECK (
        (notification_kind IS NULL AND notification_key IS NULL)
        OR (notification_kind IS NOT NULL AND notification_key IS NOT NULL)
    ),
    UNIQUE (snapshot_key, assignment_id)
);

CREATE INDEX IF NOT EXISTS role_review_items_snapshot_idx
    ON erpx_control.role_review_items (snapshot_key, assignment_id);

CREATE TABLE IF NOT EXISTS erpx_control.role_expiry_notifications (
    notification_key TEXT PRIMARY KEY,
    snapshot_key TEXT NOT NULL REFERENCES erpx_control.role_review_snapshots(snapshot_key) ON DELETE CASCADE,
    assignment_id UUID NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('platform', 'holding', 'company', 'branch', 'subtree')),
    holding_id UUID,
    company_id UUID,
    branch_id UUID,
    scope_company_ids UUID[] NOT NULL DEFAULT '{}',
    scope_branch_ids UUID[] NOT NULL DEFAULT '{}',
    principal_ref TEXT NOT NULL CHECK (length(btrim(principal_ref)) > 0),
    valid_until TIMESTAMPTZ NOT NULL,
    notification_kind TEXT NOT NULL CHECK (notification_kind IN ('expiring_soon', 'expired')),
    status TEXT NOT NULL CHECK (status IN ('pending', 'claimed', 'reconciled', 'superseded')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    claim_key TEXT,
    claimed_by UUID,
    claimed_at TIMESTAMPTZ,
    lease_until TIMESTAMPTZ,
    reconciled_at TIMESTAMPTZ,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    CONSTRAINT role_expiry_notifications_claim_shape CHECK (
        (status = 'pending' AND claimed_by IS NULL AND claimed_at IS NULL AND lease_until IS NULL)
        OR (status IN ('claimed', 'reconciled', 'superseded'))
    ),
    UNIQUE (snapshot_key, assignment_id, notification_kind)
);

CREATE INDEX IF NOT EXISTS role_expiry_notifications_claim_idx
    ON erpx_control.role_expiry_notifications (scope_kind, holding_id, company_id, branch_id, valid_until, notification_key)
    WHERE status IN ('pending', 'claimed') AND reconciled_at IS NULL;

CREATE INDEX IF NOT EXISTS role_expiry_notifications_claim_key_idx
    ON erpx_control.role_expiry_notifications (claim_key, notification_key)
    WHERE claim_key IS NOT NULL;

CREATE TABLE IF NOT EXISTS erpx_control.role_review_notification_claims (
    claim_key TEXT PRIMARY KEY CHECK (length(btrim(claim_key)) > 0),
    scope_key TEXT NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('platform', 'holding', 'company', 'branch', 'subtree')),
    holding_id UUID,
    company_id UUID,
    branch_id UUID,
    scope_company_ids UUID[] NOT NULL DEFAULT '{}',
    scope_branch_ids UUID[] NOT NULL DEFAULT '{}',
    claimant_ref TEXT NOT NULL CHECK (length(btrim(claimant_ref)) > 0),
    requested_limit INTEGER NOT NULL CHECK (requested_limit > 0 AND requested_limit <= 100),
    notification_keys TEXT[] NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL,
    lease_until TIMESTAMPTZ NOT NULL CHECK (lease_until > claimed_at),
    reconciled_count INTEGER NOT NULL DEFAULT 0 CHECK (reconciled_count >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS role_review_notification_claims_scope_idx
    ON erpx_control.role_review_notification_claims (scope_kind, holding_id, company_id, branch_id, claimed_at DESC);

CREATE TABLE IF NOT EXISTS erpx_control.role_recovery_review_receipts (
    request_id UUID NOT NULL,
    review_version BIGINT NOT NULL CHECK (review_version > 0),
    scope_key TEXT NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('platform', 'holding', 'company', 'branch', 'subtree')),
    holding_id UUID,
    company_id UUID,
    branch_id UUID,
    scope_company_ids UUID[] NOT NULL DEFAULT '{}',
    scope_branch_ids UUID[] NOT NULL DEFAULT '{}',
    target_kind TEXT NOT NULL CHECK (target_kind IN ('platform_root', 'holding_owner')),
    target_holding_id UUID,
    principal_ref TEXT NOT NULL CHECK (length(btrim(principal_ref)) > 0),
    permissions TEXT[] NOT NULL CHECK (cardinality(permissions) >= 1),
    functional_domain TEXT NOT NULL CHECK (length(btrim(functional_domain)) > 0),
    authority_level SMALLINT NOT NULL CHECK (authority_level >= 0),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    timezone TEXT NOT NULL CHECK (length(btrim(timezone)) > 0),
    reason_hash TEXT NOT NULL CHECK (reason_hash ~ '^[0-9a-f]{64}$'),
    ticket_reference_hash TEXT CHECK (ticket_reference_hash IS NULL OR ticket_reference_hash ~ '^[0-9a-f]{64}$'),
    requester_ref TEXT NOT NULL CHECK (length(btrim(requester_ref)) > 0),
    approver_ref TEXT NOT NULL CHECK (length(btrim(approver_ref)) > 0),
    reviewer_ref TEXT,
    decision TEXT NOT NULL CHECK (decision IN ('approved', 'rejected')),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    idempotency_key TEXT NOT NULL UNIQUE CHECK (length(btrim(idempotency_key)) > 0),
    actor_ref TEXT NOT NULL CHECK (length(btrim(actor_ref)) > 0),
    correlation_id UUID NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (request_id, review_version),
    CONSTRAINT role_recovery_receipt_target_shape CHECK (
        (target_kind = 'platform_root' AND target_holding_id IS NULL)
        OR (target_kind = 'holding_owner' AND target_holding_id IS NOT NULL)
    ),
    CONSTRAINT role_recovery_receipt_window_order CHECK (valid_until > valid_from)
);

CREATE INDEX IF NOT EXISTS role_recovery_review_receipts_scope_idx
    ON erpx_control.role_recovery_review_receipts (scope_kind, holding_id, company_id, branch_id, recorded_at DESC);

CREATE INDEX IF NOT EXISTS role_recovery_review_receipts_request_idx
    ON erpx_control.role_recovery_review_receipts (request_id, review_version DESC);
