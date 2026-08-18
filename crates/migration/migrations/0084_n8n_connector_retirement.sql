-- Retire the n8n connector kind without rewriting durable authority or CQRS
-- evidence. Any legacy row aborts the migration so an operator must reconcile
-- it explicitly before this generation can advance.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM erpx_platform_administration_write.connector_guards
        WHERE lower(connector_kind_type) = 'n8n'
           OR lower(coalesce(connector_kind_other, '')) = 'n8n'
    ) OR EXISTS (
        SELECT 1 FROM erpx_cqrs_write.command_receipts
        WHERE context_name = 'platform_administration'
          AND lower(coalesce(receipt::text, '')) LIKE '%n8n%'
    ) OR EXISTS (
        SELECT 1 FROM erpx_cqrs_write.audit_log
        WHERE context_name = 'platform_administration'
          AND lower(details::text) LIKE '%n8n%'
    ) OR EXISTS (
        SELECT 1 FROM erpx_cqrs_write.outbox_events
        WHERE context_name = 'platform_administration'
          AND lower(payload::text) LIKE '%n8n%'
    ) THEN
        RAISE EXCEPTION 'n8n connector evidence exists; retirement requires explicit reconciliation';
    END IF;
END;
$$;

ALTER TABLE erpx_platform_administration_write.connector_guards
    DROP CONSTRAINT connector_guards_connector_kind_type_check,
    ADD CONSTRAINT connector_guards_connector_kind_type_check CHECK (
        connector_kind_type IN ('stripe','minio','ollama','other')
    ),
    ADD CONSTRAINT platform_connector_other_kind_retirement CHECK (
        connector_kind_other IS NULL OR lower(connector_kind_other) <> 'n8n'
    );

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
