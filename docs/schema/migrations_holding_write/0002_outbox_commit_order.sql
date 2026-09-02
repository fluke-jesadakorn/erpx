-- A BIGSERIAL value records allocation order, not commit order. Security
-- projections cursor each holding independently, so serialize sequence
-- allocation per transaction-bound holding before nextval and retain the
-- advisory lock until commit. The BEFORE INSERT trigger covers every direct
-- producer without relying on a shared Rust helper or a bypassable default.
CREATE OR REPLACE FUNCTION erpx_cqrs_write.lock_outbox_commit_order()
RETURNS VOID
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    bound_holding UUID := erpx_cqrs_write.current_holding_id();
BEGIN
    IF bound_holding IS NULL THEN
        RAISE EXCEPTION 'holding authority is required for outbox sequence allocation'
            USING ERRCODE = '42501';
    END IF;
    PERFORM pg_catalog.pg_advisory_xact_lock(
        1163022424,
        pg_catalog.hashtext(bound_holding::TEXT)
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
          AND database_role='holding_write' AND migration_set='holding-write'
    ) THEN
        RAISE EXCEPTION 'holding-write current manifest is not current';
    END IF;
END;
$$;
