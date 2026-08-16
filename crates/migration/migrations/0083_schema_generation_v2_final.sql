-- Hard cutover: aggregate state is owned by context-specific typed relational
-- tables. Remove the obsolete shared JSON snapshot authority for databases
-- that applied an earlier generation-two foundation revision.
DROP TABLE IF EXISTS erpx_cqrs_write.aggregate_snapshots;
DROP TABLE IF EXISTS erpx_control.aggregate_snapshots;

-- Final control_write / control-write-v2 generation fence. The authoritative
-- erpx_meta.schema_manifest must still identify schema_generation 2.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_generation=2 AND manifest_version=1
          AND database_role='control_write' AND migration_set='control-write-v2'
    ) THEN
        RAISE EXCEPTION 'control-write generation-v2 manifest is not current';
    END IF;
END;
$$;
