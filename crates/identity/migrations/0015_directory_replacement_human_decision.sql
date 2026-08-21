-- Directory identity replacement is a high-impact human decision. Preserve
-- the authenticated proposal session and an immutable, fully bound decision
-- record without inventing evidence for historical rows.
ALTER TABLE erpx_identity.directory_identity_replacement_approvals
    ADD COLUMN IF NOT EXISTS initiator_session_id UUID;

ALTER TABLE erpx_identity.directory_identity_replacement_approvals
    DROP CONSTRAINT IF EXISTS directory_replacement_initiator_session_check;
ALTER TABLE erpx_identity.directory_identity_replacement_approvals
    ADD CONSTRAINT directory_replacement_initiator_session_check CHECK (
        initiator_session_id IS NULL
        OR initiator_session_id <> '00000000-0000-0000-0000-000000000000'::UUID
    );

CREATE TABLE IF NOT EXISTS erpx_identity.directory_identity_replacement_decision_evidence (
    conflict_id UUID NOT NULL
        REFERENCES erpx_identity.directory_identity_conflicts(id) ON DELETE RESTRICT,
    expected_conflict_version BIGINT NOT NULL CHECK (expected_conflict_version > 0),
    decision_action TEXT NOT NULL CHECK (decision_action IN ('approve', 'reject')),
    initiator_ref TEXT NOT NULL,
    initiator_session_id UUID NOT NULL,
    decision_actor_ref TEXT NOT NULL,
    decision_actor_kind TEXT NOT NULL,
    decision_session_id UUID NOT NULL,
    authority_scope_key TEXT NOT NULL,
    decision_ref TEXT NOT NULL UNIQUE,
    decided_at TIMESTAMPTZ NOT NULL,
    correlation_id UUID NOT NULL,
    PRIMARY KEY (conflict_id, expected_conflict_version),
    CHECK (length(btrim(initiator_ref)) BETWEEN 1 AND 256),
    CHECK (length(btrim(decision_actor_ref)) BETWEEN 1 AND 256),
    CHECK (length(btrim(authority_scope_key)) BETWEEN 1 AND 256),
    CHECK (length(btrim(decision_ref)) BETWEEN 1 AND 256),
    CHECK (initiator_ref !~ '[[:cntrl:]]'),
    CHECK (decision_actor_ref !~ '[[:cntrl:]]'),
    CHECK (authority_scope_key !~ '[[:cntrl:]]'),
    CHECK (decision_ref !~ '[[:cntrl:]]'),
    CHECK (initiator_ref <> decision_actor_ref),
    CHECK (initiator_session_id <> decision_session_id),
    CHECK (initiator_session_id <> '00000000-0000-0000-0000-000000000000'::UUID),
    CHECK (decision_session_id <> '00000000-0000-0000-0000-000000000000'::UUID),
    CHECK (correlation_id <> '00000000-0000-0000-0000-000000000000'::UUID)
);

-- Re-running this unreleased additive migration against a database where the
-- table was already created must not invent actor kind for existing evidence.
-- `SET NOT NULL` therefore fails closed if such rows need explicit review.
ALTER TABLE erpx_identity.directory_identity_replacement_decision_evidence
    ADD COLUMN IF NOT EXISTS decision_actor_kind TEXT;
ALTER TABLE erpx_identity.directory_identity_replacement_decision_evidence
    DROP CONSTRAINT IF EXISTS directory_replacement_decision_actor_kind_check;
ALTER TABLE erpx_identity.directory_identity_replacement_decision_evidence
    ADD CONSTRAINT directory_replacement_decision_actor_kind_check
        CHECK (decision_actor_kind = 'human');
ALTER TABLE erpx_identity.directory_identity_replacement_decision_evidence
    ALTER COLUMN decision_actor_kind SET NOT NULL;

CREATE INDEX IF NOT EXISTS directory_replacement_decision_actor_idx
    ON erpx_identity.directory_identity_replacement_decision_evidence
        (decision_actor_ref, decided_at DESC, conflict_id);
