-- Owner-side typed field patch application. Each supported owner keeps an
-- immutable record-field subaggregate in its own schema. The Agent envelope
-- and owner application are committed by one holding-local SQL transaction.

CREATE FUNCTION erpx_agent_write.reject_owner_field_patch_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'owner field patch applications are append-only';
END;
$$;

CREATE TABLE erpx_sales_write.record_field_patch_applications (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    application_id UUID NOT NULL, command_id UUID NOT NULL, suggestion_id UUID NOT NULL,
    record_id UUID NOT NULL, record_type TEXT NOT NULL CHECK (record_type='sales.quote'),
    source_record_version BIGINT NOT NULL CHECK (source_record_version>0),
    schema_version BIGINT NOT NULL CHECK (schema_version>0),
    owner_revision BIGINT NOT NULL CHECK (owner_revision>0), previous_application_id UUID,
    field_values JSONB NOT NULL CHECK (jsonb_typeof(field_values)='object'),
    patch_document JSONB NOT NULL, human_review_document JSONB NOT NULL,
    request_digest TEXT NOT NULL CHECK (request_digest~'^[0-9A-Fa-f]{64}$'),
    human_apply_permission TEXT NOT NULL,
    actor_principal_ref TEXT NOT NULL, actor_session_id UUID NOT NULL,
    decision_ref TEXT NOT NULL, applied_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,application_id), UNIQUE (holding_id,command_id),
    UNIQUE (holding_id,record_id,owner_revision)
);

CREATE TABLE erpx_customer_360_write.record_field_patch_applications (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    application_id UUID NOT NULL, command_id UUID NOT NULL, suggestion_id UUID NOT NULL,
    record_id UUID NOT NULL, record_type TEXT NOT NULL CHECK (record_type='customer-360.note'),
    source_record_version BIGINT NOT NULL CHECK (source_record_version>0),
    schema_version BIGINT NOT NULL CHECK (schema_version>0),
    owner_revision BIGINT NOT NULL CHECK (owner_revision>0), previous_application_id UUID,
    field_values JSONB NOT NULL CHECK (jsonb_typeof(field_values)='object'),
    patch_document JSONB NOT NULL, human_review_document JSONB NOT NULL,
    request_digest TEXT NOT NULL CHECK (request_digest~'^[0-9A-Fa-f]{64}$'),
    human_apply_permission TEXT NOT NULL,
    actor_principal_ref TEXT NOT NULL, actor_session_id UUID NOT NULL,
    decision_ref TEXT NOT NULL, applied_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,application_id), UNIQUE (holding_id,command_id),
    UNIQUE (holding_id,record_id,owner_revision)
);

CREATE TABLE erpx_procurement_write.record_field_patch_applications (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    application_id UUID NOT NULL, command_id UUID NOT NULL, suggestion_id UUID NOT NULL,
    record_id UUID NOT NULL, record_type TEXT NOT NULL CHECK (record_type='procurement.requisition'),
    source_record_version BIGINT NOT NULL CHECK (source_record_version>0),
    schema_version BIGINT NOT NULL CHECK (schema_version>0),
    owner_revision BIGINT NOT NULL CHECK (owner_revision>0), previous_application_id UUID,
    field_values JSONB NOT NULL CHECK (jsonb_typeof(field_values)='object'),
    patch_document JSONB NOT NULL, human_review_document JSONB NOT NULL,
    request_digest TEXT NOT NULL CHECK (request_digest~'^[0-9A-Fa-f]{64}$'),
    human_apply_permission TEXT NOT NULL,
    actor_principal_ref TEXT NOT NULL, actor_session_id UUID NOT NULL,
    decision_ref TEXT NOT NULL, applied_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,application_id), UNIQUE (holding_id,command_id),
    UNIQUE (holding_id,record_id,owner_revision)
);

CREATE TABLE erpx_maintenance_write.record_field_patch_applications (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    application_id UUID NOT NULL, command_id UUID NOT NULL, suggestion_id UUID NOT NULL,
    record_id UUID NOT NULL, record_type TEXT NOT NULL CHECK (record_type='maintenance.request'),
    source_record_version BIGINT NOT NULL CHECK (source_record_version>0),
    schema_version BIGINT NOT NULL CHECK (schema_version>0),
    owner_revision BIGINT NOT NULL CHECK (owner_revision>0), previous_application_id UUID,
    field_values JSONB NOT NULL CHECK (jsonb_typeof(field_values)='object'),
    patch_document JSONB NOT NULL, human_review_document JSONB NOT NULL,
    request_digest TEXT NOT NULL CHECK (request_digest~'^[0-9A-Fa-f]{64}$'),
    human_apply_permission TEXT NOT NULL,
    actor_principal_ref TEXT NOT NULL, actor_session_id UUID NOT NULL,
    decision_ref TEXT NOT NULL, applied_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,application_id), UNIQUE (holding_id,command_id),
    UNIQUE (holding_id,record_id,owner_revision)
);

CREATE TABLE erpx_marketing_write.record_field_patch_applications (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    application_id UUID NOT NULL, command_id UUID NOT NULL, suggestion_id UUID NOT NULL,
    record_id UUID NOT NULL, record_type TEXT NOT NULL CHECK (record_type='marketing.content'),
    source_record_version BIGINT NOT NULL CHECK (source_record_version>0),
    schema_version BIGINT NOT NULL CHECK (schema_version>0),
    owner_revision BIGINT NOT NULL CHECK (owner_revision>0), previous_application_id UUID,
    field_values JSONB NOT NULL CHECK (jsonb_typeof(field_values)='object'),
    patch_document JSONB NOT NULL, human_review_document JSONB NOT NULL,
    request_digest TEXT NOT NULL CHECK (request_digest~'^[0-9A-Fa-f]{64}$'),
    human_apply_permission TEXT NOT NULL,
    actor_principal_ref TEXT NOT NULL, actor_session_id UUID NOT NULL,
    decision_ref TEXT NOT NULL, applied_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,application_id), UNIQUE (holding_id,command_id),
    UNIQUE (holding_id,record_id,owner_revision)
);

DO $$
DECLARE
    owned_table REGCLASS;
    policy_name TEXT;
BEGIN
    FOREACH owned_table IN ARRAY ARRAY[
        'erpx_sales_write.record_field_patch_applications'::REGCLASS,
        'erpx_customer_360_write.record_field_patch_applications'::REGCLASS,
        'erpx_procurement_write.record_field_patch_applications'::REGCLASS,
        'erpx_maintenance_write.record_field_patch_applications'::REGCLASS,
        'erpx_marketing_write.record_field_patch_applications'::REGCLASS
    ] LOOP
        EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY',owned_table);
        EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY',owned_table);
        policy_name := replace(owned_table::TEXT,'.','_')||'_holding_isolation';
        EXECUTE format(
            'CREATE POLICY %I ON %s USING (holding_id=erpx_cqrs_write.current_holding_id()) WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())',
            policy_name,owned_table
        );
        EXECUTE format(
            'CREATE TRIGGER owner_field_patch_append_only BEFORE UPDATE OR DELETE ON %s FOR EACH ROW EXECUTE FUNCTION erpx_agent_write.reject_owner_field_patch_mutation()',
            owned_table
        );
    END LOOP;
END;
$$;

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
