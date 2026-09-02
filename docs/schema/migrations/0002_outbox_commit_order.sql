-- A BIGSERIAL value records allocation order, not commit order. A durable
-- projection cursor must never observe N+1 while N is still able to commit.
-- Control-write has one outbox per database, so serialize sequence allocation
-- with one transaction-scoped advisory lock held until commit.
CREATE OR REPLACE FUNCTION erpx_cqrs_write.lock_outbox_commit_order()
RETURNS VOID
LANGUAGE plpgsql
VOLATILE
AS $$
BEGIN
    PERFORM pg_catalog.pg_advisory_xact_lock(
        1163022424,
        0
    );
END;
$$;

CREATE OR REPLACE FUNCTION erpx_cqrs_write.assign_outbox_global_sequence()
RETURNS TRIGGER
LANGUAGE plpgsql
VOLATILE
AS $$
BEGIN
    IF NEW.global_sequence IS NOT NULL THEN
        RAISE EXCEPTION 'global_sequence is database assigned'
            USING ERRCODE = '22023';
    END IF;
    PERFORM erpx_cqrs_write.lock_outbox_commit_order();
    NEW.global_sequence := pg_catalog.nextval(
        'erpx_cqrs_write.outbox_events_global_sequence_seq'::REGCLASS
    );
    RETURN NEW;
END;
$$;

ALTER TABLE erpx_cqrs_write.outbox_events
    ALTER COLUMN global_sequence
    DROP DEFAULT;

DROP TRIGGER IF EXISTS cqrs_outbox_assign_global_sequence
    ON erpx_cqrs_write.outbox_events;
CREATE TRIGGER cqrs_outbox_assign_global_sequence
BEFORE INSERT ON erpx_cqrs_write.outbox_events
FOR EACH ROW
EXECUTE FUNCTION erpx_cqrs_write.assign_outbox_global_sequence();

-- Every migration set must finish by proving that it is updating the expected
-- physical database role. This is an assertion, not a mutable authority row.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='control_write' AND migration_set='control-write'
    ) THEN
        RAISE EXCEPTION 'control-write current manifest is not current';
    END IF;
END;
$$;
