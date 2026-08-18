-- The read database is disposable, but refuse to silently hide a legacy n8n
-- projection or projection failure/receipt trail during an in-place upgrade.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM erpx_platform_administration_read.connector_projection
        WHERE lower(connector_kind) = 'n8n' OR lower(state::text) LIKE '%n8n%'
    ) OR EXISTS (
        SELECT 1 FROM erpx_cqrs_read.projection_failures
        WHERE projection_name = 'platform_administration.connectors'
          AND lower(last_error) LIKE '%n8n%'
    ) THEN
        RAISE EXCEPTION 'n8n connector projection evidence exists; retirement requires explicit reconciliation';
    END IF;
END;
$$;

ALTER TABLE erpx_platform_administration_read.connector_projection
    ADD CONSTRAINT platform_connector_projection_n8n_retired CHECK (
        lower(connector_kind) <> 'n8n' AND lower(state::text) NOT LIKE '%n8n%'
    );

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_generation=2 AND manifest_version=1
          AND database_role='control_read' AND migration_set='control-read-v2'
    ) THEN
        RAISE EXCEPTION 'control-read generation-v2 manifest is not current';
    END IF;
END;
$$;
