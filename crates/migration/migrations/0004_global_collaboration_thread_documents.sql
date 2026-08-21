-- Forward-only snapshot needed by the audited Platform Collaboration gateway.
-- The normalized child tables remain authoritative evidence; this document is
-- a typed aggregate snapshot used only to enforce optimistic mutation rules
-- without inventing a holding-local thread root.

ALTER TABLE erpx_global_collaboration.threads
    ADD COLUMN thread_document JSONB;

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
