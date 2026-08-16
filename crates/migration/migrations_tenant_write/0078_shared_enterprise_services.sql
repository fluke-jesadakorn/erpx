-- Shared enterprise-service authority. Source-context business records remain
-- authoritative. JSON columns below are bounded documents/evidence only; no
-- table stores a serialized aggregate snapshot as write authority.

CREATE SCHEMA IF NOT EXISTS erpx_shared_write;

CREATE OR REPLACE FUNCTION erpx_shared_write.current_tenant_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.tenant_id', true), '')::uuid
$$;

CREATE TABLE erpx_shared_write.approval_sessions (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_write.current_tenant_id(),
    session_id UUID NOT NULL,
    scope_key TEXT NOT NULL CHECK (octet_length(scope_key) BETWEEN 1 AND 2048),
    source_record_id UUID NOT NULL,
    source_record_version BIGINT NOT NULL CHECK (source_record_version > 0),
    policy_id UUID NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    policy_sha256 TEXT NOT NULL CHECK (policy_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    session_status TEXT NOT NULL CHECK (session_status IN (
        'pending','approved','rejected','changes_requested','invalidated','expired'
    )),
    current_gate_id TEXT,
    due_at TIMESTAMPTZ NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    -- Evidence for evaluated gates; never an approval-session snapshot.
    gate_evidence JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, session_id),
    UNIQUE (tenant_id, session_id, aggregate_version)
);

CREATE TABLE erpx_shared_write.approval_decisions (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_write.current_tenant_id(),
    session_id UUID NOT NULL,
    decision_sequence BIGINT NOT NULL CHECK (decision_sequence > 0),
    gate_id TEXT NOT NULL,
    actor_ref TEXT NOT NULL,
    represented_actor_ref TEXT,
    outcome TEXT NOT NULL CHECK (outcome IN ('approve','reject','request_changes')),
    authority_version BIGINT NOT NULL CHECK (authority_version > 0),
    authority_sha256 TEXT NOT NULL CHECK (authority_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    -- Immutable versions/digests used by the decision, not mutable business state.
    evidence_versions JSONB NOT NULL,
    decided_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, session_id, decision_sequence),
    FOREIGN KEY (tenant_id, session_id)
        REFERENCES erpx_shared_write.approval_sessions(tenant_id, session_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_shared_write.notifications (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_write.current_tenant_id(),
    notification_id UUID NOT NULL,
    scope_key TEXT NOT NULL CHECK (octet_length(scope_key) BETWEEN 1 AND 2048),
    tenant_scope JSONB NOT NULL,
    source_id TEXT NOT NULL CHECK (octet_length(source_id) BETWEEN 1 AND 255),
    source_record_ref TEXT NOT NULL CHECK (octet_length(source_record_ref) BETWEEN 1 AND 255),
    recipient_ref TEXT NOT NULL CHECK (octet_length(recipient_ref) BETWEEN 1 AND 255),
    classification TEXT NOT NULL CHECK (classification IN (
        'public','internal','confidential','restricted'
    )),
    preview TEXT CHECK (octet_length(preview) <= 1024),
    inbox_state TEXT NOT NULL CHECK (inbox_state IN (
        'unread','read','acknowledged','action_required','withdrawn'
    )),
    mandatory_policy_version TEXT,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    delivery_version BIGINT NOT NULL CHECK (delivery_version > 0),
    evidence_sha256 TEXT NOT NULL CHECK (evidence_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, notification_id),
    UNIQUE (tenant_id, notification_id, aggregate_version)
);

CREATE TABLE erpx_shared_write.notification_delivery_attempts (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_write.current_tenant_id(),
    notification_id UUID NOT NULL,
    recipient_ref TEXT NOT NULL,
    channel TEXT NOT NULL CHECK (channel IN ('in_app','email','sms','push','webhook')),
    delivery_version BIGINT NOT NULL CHECK (delivery_version > 0),
    dedupe_key TEXT NOT NULL,
    delivery_status TEXT NOT NULL CHECK (delivery_status IN (
        'queued','accepted','delivered','failed','uncertain','suppressed'
    )),
    provider_message_ref TEXT,
    last_provider_sequence BIGINT NOT NULL DEFAULT 0 CHECK (last_provider_sequence >= 0),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error_code TEXT,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, notification_id, recipient_ref, channel, delivery_version),
    UNIQUE (tenant_id, dedupe_key),
    FOREIGN KEY (tenant_id, notification_id)
        REFERENCES erpx_shared_write.notifications(tenant_id, notification_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_shared_write.notification_delivery_callbacks (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_write.current_tenant_id(),
    notification_id UUID NOT NULL,
    recipient_ref TEXT NOT NULL,
    channel TEXT NOT NULL,
    delivery_version BIGINT NOT NULL,
    callback_id TEXT NOT NULL,
    callback_ordinal INTEGER NOT NULL CHECK (callback_ordinal > 0),
    processed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, notification_id, recipient_ref, channel, delivery_version, callback_id),
    UNIQUE (tenant_id, notification_id, recipient_ref, channel, delivery_version, callback_ordinal),
    FOREIGN KEY (tenant_id, notification_id, recipient_ref, channel, delivery_version)
        REFERENCES erpx_shared_write.notification_delivery_attempts(
            tenant_id, notification_id, recipient_ref, channel, delivery_version
        ) ON DELETE CASCADE
);

CREATE TABLE erpx_shared_write.signature_envelopes (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_write.current_tenant_id(),
    envelope_id UUID NOT NULL,
    scope_key TEXT NOT NULL CHECK (octet_length(scope_key) BETWEEN 1 AND 2048),
    tenant_scope JSONB NOT NULL,
    document_id UUID NOT NULL,
    document_version BIGINT NOT NULL CHECK (document_version > 0),
    document_sha256 TEXT NOT NULL CHECK (document_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    provider_key TEXT NOT NULL CHECK (octet_length(provider_key) BETWEEN 1 AND 255),
    provider_policy_version TEXT NOT NULL CHECK (
        octet_length(provider_policy_version) BETWEEN 1 AND 255
    ),
    signature_order TEXT NOT NULL CHECK (signature_order IN ('parallel','serial')),
    retention_until TIMESTAMPTZ NOT NULL,
    envelope_status TEXT NOT NULL CHECK (envelope_status IN (
        'draft','submitted','in_progress','completed','failed','cancelled','expired'
    )),
    provider_envelope_ref TEXT,
    last_provider_sequence BIGINT NOT NULL DEFAULT 0 CHECK (last_provider_sequence >= 0),
    evidence_sha256 TEXT CHECK (evidence_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    mutation_evidence_sha256 TEXT NOT NULL CHECK (
        mutation_evidence_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'
    ),
    created_by TEXT NOT NULL CHECK (octet_length(created_by) BETWEEN 1 AND 255),
    cancelled_by TEXT,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, envelope_id),
    UNIQUE (tenant_id, envelope_id, aggregate_version),
    UNIQUE (tenant_id, provider_envelope_ref)
);

CREATE TABLE erpx_shared_write.signature_recipients (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_write.current_tenant_id(),
    envelope_id UUID NOT NULL,
    recipient_id UUID NOT NULL,
    recipient_order INTEGER NOT NULL CHECK (recipient_order > 0),
    subject_realm TEXT NOT NULL CHECK (subject_realm IN ('internal','external_one_time')),
    principal_ref TEXT,
    external_subject_ref TEXT,
    access_grant_sha256 TEXT CHECK (access_grant_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    access_expires_at TIMESTAMPTZ,
    signed_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, envelope_id, recipient_id),
    FOREIGN KEY (tenant_id, envelope_id)
        REFERENCES erpx_shared_write.signature_envelopes(tenant_id, envelope_id)
        ON DELETE CASCADE,
    CHECK (
        (subject_realm = 'internal' AND principal_ref IS NOT NULL
            AND external_subject_ref IS NULL AND access_grant_sha256 IS NULL
            AND access_expires_at IS NULL)
        OR
        (subject_realm = 'external_one_time' AND principal_ref IS NULL
            AND external_subject_ref IS NOT NULL AND access_grant_sha256 IS NOT NULL
            AND access_expires_at IS NOT NULL)
    )
);

CREATE TABLE erpx_shared_write.signature_provider_events (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_write.current_tenant_id(),
    envelope_id UUID NOT NULL,
    provider_event_id TEXT NOT NULL,
    event_ordinal INTEGER NOT NULL CHECK (event_ordinal > 0),
    processed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, envelope_id, provider_event_id),
    UNIQUE (tenant_id, envelope_id, event_ordinal),
    FOREIGN KEY (tenant_id, envelope_id)
        REFERENCES erpx_shared_write.signature_envelopes(tenant_id, envelope_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_shared_write.report_export_manifests (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_write.current_tenant_id(),
    export_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    definition_id UUID NOT NULL,
    definition_version BIGINT NOT NULL CHECK (definition_version > 0),
    manifest_sha256 TEXT NOT NULL CHECK (manifest_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    classification TEXT NOT NULL CHECK (classification IN (
        'public','internal','confidential','restricted'
    )),
    status TEXT NOT NULL CHECK (status IN (
        'queued','running','completed','failed','cancelled','expired','deletion_pending','deleted'
    )),
    expires_at TIMESTAMPTZ NOT NULL,
    -- Signed delivery manifest; it is not a report-job aggregate snapshot.
    manifest JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, export_id),
    UNIQUE (tenant_id, export_id, manifest_sha256)
);

CREATE TABLE erpx_shared_write.import_row_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_write.current_tenant_id(),
    batch_id UUID NOT NULL,
    row_number INTEGER NOT NULL CHECK (row_number > 0),
    command_id UUID NOT NULL,
    source_receipt_ref TEXT,
    rejection_code TEXT,
    reconciled_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, batch_id, row_number),
    UNIQUE (tenant_id, command_id),
    CHECK ((source_receipt_ref IS NULL) <> (rejection_code IS NULL))
);

CREATE TABLE erpx_shared_write.extension_schema_versions (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_write.current_tenant_id(),
    extension_id UUID NOT NULL,
    extension_version BIGINT NOT NULL CHECK (extension_version > 0),
    scope_key TEXT NOT NULL,
    owner_ref TEXT NOT NULL,
    affected_context TEXT NOT NULL,
    extension_kind TEXT NOT NULL CHECK (
        extension_kind IN ('custom_field','form','workflow_template')
    ),
    approval_sha256 TEXT CHECK (approval_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    active BOOLEAN NOT NULL,
    -- Versioned field/form/workflow schema, never executable code or SQL.
    definition JSONB NOT NULL,
    published_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, extension_id, extension_version)
);

CREATE TABLE erpx_shared_write.signature_callback_inbox (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_write.current_tenant_id(),
    provider_key TEXT NOT NULL,
    provider_event_id TEXT NOT NULL,
    provider_envelope_ref TEXT NOT NULL,
    provider_sequence BIGINT NOT NULL CHECK (provider_sequence > 0),
    callback_sha256 TEXT NOT NULL CHECK (callback_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    verified BOOLEAN NOT NULL,
    received_at TIMESTAMPTZ NOT NULL,
    applied_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, provider_key, provider_event_id),
    UNIQUE (tenant_id, provider_key, provider_envelope_ref, provider_sequence)
);

CREATE INDEX shared_approval_due
    ON erpx_shared_write.approval_sessions(tenant_id, scope_key, due_at)
    WHERE session_status = 'pending';
CREATE INDEX shared_notification_recipient
    ON erpx_shared_write.notifications(tenant_id, recipient_ref, inbox_state, updated_at DESC);
CREATE INDEX shared_notification_retry
    ON erpx_shared_write.notification_delivery_attempts(tenant_id, delivery_status, updated_at)
    WHERE delivery_status IN ('queued','uncertain');
CREATE INDEX shared_signature_retention
    ON erpx_shared_write.signature_envelopes(tenant_id, retention_until, envelope_status);
CREATE INDEX shared_report_expiry
    ON erpx_shared_write.report_export_manifests(tenant_id, expires_at)
    WHERE status IN ('completed','deletion_pending');

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'approval_sessions','approval_decisions','notifications',
        'notification_delivery_attempts','notification_delivery_callbacks',
        'signature_envelopes','signature_recipients','signature_provider_events',
        'report_export_manifests','import_row_receipts',
        'extension_schema_versions','signature_callback_inbox'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_shared_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_shared_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY shared_tenant_isolation ON erpx_shared_write.%I '
            'USING (tenant_id = erpx_shared_write.current_tenant_id()) '
            'WITH CHECK (tenant_id = erpx_shared_write.current_tenant_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- Final generation-two role marker: erpx_meta.schema_manifest,
-- schema_generation=2, database_role=tenant_write, migration_set=tenant-write-v2.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_generation = 2
  AND database_role = 'tenant_write' AND migration_set = 'tenant-write-v2';
