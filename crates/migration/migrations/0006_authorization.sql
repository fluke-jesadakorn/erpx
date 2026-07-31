CREATE TABLE IF NOT EXISTS erpx_control.sod_decisions (
    principal_ref TEXT NOT NULL,
    module_key TEXT NOT NULL,
    scope JSONB NOT NULL,
    passed BOOLEAN NOT NULL,
    reviewer_ref TEXT NOT NULL,
    reason TEXT NOT NULL,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    decision_version BIGINT NOT NULL CHECK (decision_version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (btrim(principal_ref) <> ''),
    CHECK (btrim(module_key) <> ''),
    CHECK (btrim(reviewer_ref) <> ''),
    CHECK (btrim(reason) <> ''),
    CHECK (valid_until > valid_from),
    PRIMARY KEY (principal_ref, module_key, scope)
);

CREATE INDEX IF NOT EXISTS sod_decisions_expiry_idx
    ON erpx_control.sod_decisions (valid_until, principal_ref, module_key);
