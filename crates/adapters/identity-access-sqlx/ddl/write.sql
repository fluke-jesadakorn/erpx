CREATE SCHEMA IF NOT EXISTS erpx_identity_access_write;

CREATE TABLE erpx_identity_access_write.role_code_guards (
    role_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    role_code TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, role_code)
);

CREATE TABLE erpx_identity_access_write.assignment_guards (
    assignment_id UUID PRIMARY KEY,
    principal_id UUID NOT NULL,
    role_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    risk_tier TEXT NOT NULL CHECK (risk_tier IN (
        'platform_root', 'administrator', 'high', 'standard', 'contractor',
        'support_diagnostic', 'support_mutation', 'emergency'
    )),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    principal_version BIGINT NOT NULL CHECK (principal_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (valid_until > valid_from)
);

CREATE UNIQUE INDEX identity_access_one_live_assignment
    ON erpx_identity_access_write.assignment_guards
        (principal_id, role_id, scope_key)
    WHERE revoked_at IS NULL;

CREATE INDEX identity_access_assignment_validity
    ON erpx_identity_access_write.assignment_guards
        (principal_id, valid_from, valid_until)
    WHERE revoked_at IS NULL;
