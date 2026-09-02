-- Phase 3: truthful Agent assignment command receipts for rolling readers.
-- Legacy receipts remain readable with nullable metadata and are rehydrated
-- from the authoritative outbox event by the adapter.
ALTER TABLE erpx_agent_write.agent_command_receipts
    ADD COLUMN command_id UUID,
    ADD COLUMN correlation_id UUID,
    ADD COLUMN event_id UUID,
    ADD COLUMN global_sequence BIGINT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema='erpx_agent_write'
          AND table_name='agent_assignments'
          AND column_name='holding_scope'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema='erpx_agent_write'
          AND table_name='agent_assignments'
          AND column_name='active_scope'
    ) THEN
        ALTER TABLE erpx_agent_write.agent_assignments
            RENAME COLUMN holding_scope TO active_scope;
    END IF;
END $$;

ALTER TABLE erpx_agent_write.agent_assignments
    ADD COLUMN human_assignment_decision JSONB;

ALTER TABLE erpx_agent_write.agent_command_receipts
    ADD CONSTRAINT agent_command_receipt_phase3_metadata
    CHECK (
        (command_id IS NULL AND correlation_id IS NULL AND event_id IS NULL AND global_sequence IS NULL)
        OR
        (command_id IS NOT NULL AND correlation_id IS NOT NULL AND event_id IS NOT NULL AND global_sequence > 0)
    );

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
