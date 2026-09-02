-- Phase 4 universal typed field suggestions. The original baseline and prior
-- forward migrations are immutable; this migration broadens the existing
-- Sales/Finance envelope authority without permitting untyped owner writes.

ALTER TABLE erpx_agent_write.suggestion_envelopes
    DROP CONSTRAINT suggestion_envelopes_owner_context_check,
    DROP CONSTRAINT suggestion_envelopes_capability_check,
    DROP CONSTRAINT suggestion_envelopes_suggestion_status_check;

ALTER TABLE erpx_agent_write.suggestion_envelopes
    ADD CONSTRAINT suggestion_envelopes_owner_context_check CHECK (
        owner_context IN (
            'identity-access','platform-administration','organization','commercial',
            'brand','customer-authority','customer-360','support','finance','sales',
            'point-of-sale','procurement','inventory','assets','maintenance','quality',
            'operations','people-payroll','legal','marketing','crm'
        )
    ),
    ADD CONSTRAINT suggestion_envelopes_capability_check CHECK (
        capability IN (
            'sales.quote.compose.v1','sales.revenue-arrangement.v1',
            'finance.accounting-plan.v1','finance.posting-review.v1',
            'record.field-patch.v2'
        )
    ),
    ADD CONSTRAINT suggestion_envelopes_suggestion_status_check CHECK (
        suggestion_status IN (
            'queued','running','ready','conflicted','applied','rejected',
            'superseded','failed'
        )
    ),
    ADD COLUMN envelope_document JSONB,
    ADD COLUMN field_patch_document JSONB,
    ADD COLUMN field_patch_conflicts JSONB NOT NULL DEFAULT '[]'::JSONB,
    ADD COLUMN human_review_document JSONB,
    ADD COLUMN applied_actor_kind TEXT,
    ADD COLUMN applied_decision_ref TEXT,
    ADD COLUMN applied_session_id UUID,
    ADD CONSTRAINT suggestion_envelopes_typed_patch_check CHECK (
        (capability <> 'record.field-patch.v2'
            AND field_patch_document IS NULL
            AND field_patch_conflicts = '[]'::JSONB
            AND human_review_document IS NULL)
        OR (capability = 'record.field-patch.v2'
            AND (suggestion_status IN ('queued','running','failed')
                OR field_patch_document IS NOT NULL))
    ),
    ADD CONSTRAINT suggestion_envelopes_human_apply_check CHECK (
        suggestion_status <> 'applied'
        OR (applied_by IS NOT NULL
            AND applied_actor_kind = 'holding_user'
            AND applied_decision_ref IS NOT NULL
            AND applied_session_id IS NOT NULL)
    ),
    ADD CONSTRAINT suggestion_envelopes_conflict_check CHECK (
        suggestion_status <> 'conflicted'
        OR jsonb_array_length(field_patch_conflicts) > 0
    );

CREATE TABLE erpx_agent_write.suggestion_command_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    scope_key TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_digest TEXT NOT NULL CHECK (request_digest ~ '^[0-9A-Fa-f]{64}$'),
    suggestion_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    result_envelope_document JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,scope_key,idempotency_key),
    UNIQUE (holding_id,command_id)
);

ALTER TABLE erpx_agent_write.suggestion_command_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_agent_write.suggestion_command_receipts FORCE ROW LEVEL SECURITY;
CREATE POLICY agent_write_holding_isolation
    ON erpx_agent_write.suggestion_command_receipts
    USING (holding_id = erpx_agent_write.current_holding_id())
    WITH CHECK (holding_id = erpx_agent_write.current_holding_id());

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='holding_write' AND migration_set='holding-write'
    ) THEN
        RAISE EXCEPTION 'holding-write current manifest is not current';
    END IF;
END;
$$;
