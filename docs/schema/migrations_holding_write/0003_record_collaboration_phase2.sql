-- Phase 2 record collaboration is authoritative in the anchor's holding
-- write database. Stable record identity is retained in full; denormalized
-- UUID columns exist only for indexed joins and are constrained to the JSON
-- identity. Relation publication crosses the outbox as a draft and obtains
-- its durable sequence only from the database outbox trigger installed by
-- migration 0002.

CREATE TABLE erpx_collaboration.record_relation_graphs (
    holding_id UUID NOT NULL,
    source_record_id UUID NOT NULL,
    source_record_ref JSONB NOT NULL,
    graph_thread_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, source_record_id),
    UNIQUE (holding_id, graph_thread_id),
    CHECK (source_record_ref ->> 'id' = source_record_id::TEXT),
    CHECK (octet_length(source_record_ref ->> 'owner_context') BETWEEN 1 AND 64),
    CHECK (octet_length(source_record_ref ->> 'record_type') BETWEEN 1 AND 128),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_collaboration.record_relations (
    holding_id UUID NOT NULL,
    relation_id UUID NOT NULL,
    source_record_id UUID NOT NULL,
    target_record_id UUID NOT NULL,
    source_record_ref JSONB NOT NULL,
    target_record_ref JSONB NOT NULL,
    graph_thread_id UUID NOT NULL,
    relation_kind TEXT NOT NULL CHECK (relation_kind IN (
        'relates_to','blocks','depends_on','implements',
        'supersedes','derived_from','evidence_for'
    )),
    relation_version BIGINT NOT NULL CHECK (relation_version > 0),
    graph_version BIGINT NOT NULL CHECK (graph_version > 0),
    relation_document JSONB NOT NULL,
    relation_state TEXT NOT NULL CHECK (relation_state IN ('active','revoked')),
    created_by_principal_id UUID NOT NULL,
    created_by_session_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    revoked_by_principal_id UUID,
    revoked_by_session_id UUID,
    revoked_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, relation_id),
    FOREIGN KEY (holding_id, source_record_id)
        REFERENCES erpx_collaboration.record_relation_graphs
            (holding_id, source_record_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, graph_thread_id)
        REFERENCES erpx_collaboration.record_relation_graphs
            (holding_id, graph_thread_id) ON DELETE RESTRICT,
    CHECK (source_record_id <> target_record_id),
    CHECK (source_record_ref ->> 'id' = source_record_id::TEXT),
    CHECK (target_record_ref ->> 'id' = target_record_id::TEXT),
    CHECK (octet_length(source_record_ref ->> 'owner_context') BETWEEN 1 AND 64),
    CHECK (octet_length(source_record_ref ->> 'record_type') BETWEEN 1 AND 128),
    CHECK (octet_length(target_record_ref ->> 'owner_context') BETWEEN 1 AND 64),
    CHECK (octet_length(target_record_ref ->> 'record_type') BETWEEN 1 AND 128),
    CHECK (
        (relation_state='active' AND revoked_by_principal_id IS NULL
            AND revoked_by_session_id IS NULL AND revoked_at IS NULL)
        OR
        (relation_state='revoked' AND revoked_by_principal_id IS NOT NULL
            AND revoked_by_session_id IS NOT NULL AND revoked_at IS NOT NULL
            AND revoked_at >= created_at)
    ),
    CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX collaboration_active_relation_identity
    ON erpx_collaboration.record_relations
       (holding_id, source_record_id, target_record_id, relation_kind)
    WHERE relation_state='active';
CREATE INDEX collaboration_relation_source
    ON erpx_collaboration.record_relations
       (holding_id, source_record_id, graph_version, relation_id);

CREATE TABLE erpx_collaboration.record_relation_events (
    holding_id UUID NOT NULL,
    event_id UUID NOT NULL,
    relation_id UUID NOT NULL,
    source_record_id UUID NOT NULL,
    graph_thread_id UUID NOT NULL,
    graph_version BIGINT NOT NULL CHECK (graph_version > 0),
    event_kind TEXT NOT NULL CHECK (event_kind IN ('added','revoked')),
    event_document JSONB NOT NULL,
    actor_principal_id UUID NOT NULL,
    actor_session_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, event_id),
    UNIQUE (holding_id, source_record_id, graph_version),
    FOREIGN KEY (holding_id, relation_id)
        REFERENCES erpx_collaboration.record_relations
            (holding_id, relation_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, graph_thread_id)
        REFERENCES erpx_collaboration.record_relation_graphs
            (holding_id, graph_thread_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.record_relation_publication_drafts (
    holding_id UUID NOT NULL,
    event_id UUID NOT NULL,
    relation_id UUID NOT NULL,
    source_record_id UUID NOT NULL,
    graph_version BIGINT NOT NULL CHECK (graph_version > 0),
    fact_document JSONB NOT NULL,
    committed_sequence BIGINT,
    projected_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, event_id),
    FOREIGN KEY (holding_id, event_id)
        REFERENCES erpx_collaboration.record_relation_events
            (holding_id, event_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, relation_id)
        REFERENCES erpx_collaboration.record_relations
            (holding_id, relation_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, event_id)
        REFERENCES erpx_cqrs_write.outbox_events
            (holding_id, event_id) ON DELETE RESTRICT,
    CHECK ((committed_sequence IS NULL) = (projected_at IS NULL)),
    CHECK (committed_sequence IS NULL OR committed_sequence > 0)
);

CREATE TABLE erpx_collaboration.record_relation_command_receipts (
    holding_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) BETWEEN 1 AND 256),
    command_id UUID NOT NULL,
    request_digest BYTEA NOT NULL CHECK (octet_length(request_digest)=32),
    source_record_id UUID NOT NULL,
    relation_id UUID NOT NULL,
    graph_version BIGINT NOT NULL CHECK (graph_version > 0),
    event_id UUID NOT NULL,
    result_relation_document JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, idempotency_key),
    UNIQUE (holding_id, command_id),
    FOREIGN KEY (holding_id, event_id)
        REFERENCES erpx_collaboration.record_relation_events
            (holding_id, event_id) ON DELETE RESTRICT
);

-- Upload registration precedes any object-store URL. Finalization only moves
-- a matching pending upload into the scanner queue; no browser request can
-- mark an attachment Ready.
CREATE TABLE erpx_collaboration.attachments (
    holding_id UUID NOT NULL,
    attachment_id UUID NOT NULL,
    object_id UUID NOT NULL,
    object_key TEXT NOT NULL CHECK (octet_length(object_key) BETWEEN 1 AND 512),
    anchor_record_id UUID NOT NULL,
    anchor_record_ref JSONB NOT NULL,
    thread_id UUID,
    uploader_principal_id UUID NOT NULL,
    uploader_session_id UUID NOT NULL,
    classification TEXT NOT NULL CHECK (
        classification IN ('public','internal','confidential','restricted')
    ),
    file_name TEXT NOT NULL CHECK (octet_length(file_name) BETWEEN 1 AND 255),
    media_type TEXT NOT NULL CHECK (octet_length(media_type) BETWEEN 1 AND 255),
    expected_size_bytes BIGINT NOT NULL CHECK (expected_size_bytes BETWEEN 1 AND 104857600),
    expected_digest BYTEA NOT NULL CHECK (octet_length(expected_digest)=32),
    verified_size_bytes BIGINT,
    verified_digest BYTEA CHECK (verified_digest IS NULL OR octet_length(verified_digest)=32),
    lifecycle TEXT NOT NULL CHECK (
        lifecycle IN ('pending_upload','pending_scan','ready','quarantined','expired')
    ),
    lifecycle_version BIGINT NOT NULL CHECK (lifecycle_version > 0),
    upload_expires_at TIMESTAMPTZ NOT NULL,
    finalized_at TIMESTAMPTZ,
    scanned_at TIMESTAMPTZ,
    scan_engine TEXT,
    scan_evidence_digest BYTEA CHECK (
        scan_evidence_digest IS NULL OR octet_length(scan_evidence_digest)=32
    ),
    quarantine_reason TEXT CHECK (
        quarantine_reason IS NULL OR octet_length(quarantine_reason) BETWEEN 1 AND 128
    ),
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, attachment_id),
    UNIQUE (holding_id, object_id),
    UNIQUE (holding_id, object_key),
    CHECK (anchor_record_ref ->> 'id' = anchor_record_id::TEXT),
    CHECK (octet_length(anchor_record_ref ->> 'owner_context') BETWEEN 1 AND 64),
    CHECK (octet_length(anchor_record_ref ->> 'record_type') BETWEEN 1 AND 128),
    CHECK (upload_expires_at > created_at),
    CHECK (updated_at >= created_at),
    CHECK (
        (lifecycle='pending_upload' AND finalized_at IS NULL AND scanned_at IS NULL)
        OR (lifecycle='pending_scan' AND finalized_at IS NOT NULL AND scanned_at IS NULL
            AND verified_size_bytes=expected_size_bytes
            AND verified_digest=expected_digest)
        OR (lifecycle='ready' AND thread_id IS NOT NULL
            AND finalized_at IS NOT NULL AND scanned_at IS NOT NULL
            AND scan_engine IS NOT NULL AND scan_evidence_digest IS NOT NULL
            AND quarantine_reason IS NULL)
        OR (lifecycle='quarantined' AND thread_id IS NOT NULL
            AND finalized_at IS NOT NULL AND scanned_at IS NOT NULL
            AND scan_engine IS NOT NULL AND scan_evidence_digest IS NOT NULL
            AND quarantine_reason IS NOT NULL)
        OR lifecycle='expired'
    )
);

CREATE INDEX collaboration_attachment_scan_queue
    ON erpx_collaboration.attachments
       (holding_id, updated_at, attachment_id)
    WHERE lifecycle='pending_scan';
CREATE INDEX collaboration_attachment_expiry
    ON erpx_collaboration.attachments
       (holding_id, upload_expires_at, attachment_id)
    WHERE lifecycle IN ('pending_upload','pending_scan');

CREATE TABLE erpx_collaboration.attachment_events (
    holding_id UUID NOT NULL,
    event_id UUID NOT NULL,
    attachment_id UUID NOT NULL,
    lifecycle_version BIGINT NOT NULL CHECK (lifecycle_version > 0),
    lifecycle TEXT NOT NULL CHECK (
        lifecycle IN ('pending_upload','pending_scan','ready','quarantined','expired')
    ),
    event_document JSONB NOT NULL,
    actor_principal_id UUID,
    actor_session_id UUID,
    command_id UUID,
    correlation_id UUID,
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, event_id),
    UNIQUE (holding_id, attachment_id, lifecycle_version),
    FOREIGN KEY (holding_id, attachment_id)
        REFERENCES erpx_collaboration.attachments
            (holding_id, attachment_id) ON DELETE RESTRICT,
    CHECK (
        (actor_principal_id IS NULL) = (actor_session_id IS NULL)
    )
);

CREATE TABLE erpx_collaboration.attachment_command_receipts (
    holding_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) BETWEEN 1 AND 256),
    command_id UUID NOT NULL,
    request_digest BYTEA NOT NULL CHECK (octet_length(request_digest)=32),
    attachment_id UUID NOT NULL,
    lifecycle_version BIGINT NOT NULL CHECK (lifecycle_version > 0),
    event_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, idempotency_key),
    UNIQUE (holding_id, command_id),
    FOREIGN KEY (holding_id, event_id)
        REFERENCES erpx_collaboration.attachment_events
            (holding_id, event_id) ON DELETE RESTRICT
);

CREATE FUNCTION erpx_collaboration.enforce_attachment_transition()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.holding_id <> OLD.holding_id
       OR NEW.attachment_id <> OLD.attachment_id
       OR NEW.object_id <> OLD.object_id
       OR NEW.object_key <> OLD.object_key
       OR NEW.anchor_record_ref <> OLD.anchor_record_ref
       OR NEW.anchor_record_id <> OLD.anchor_record_id
       OR (
           NEW.thread_id IS DISTINCT FROM OLD.thread_id
           AND NOT (
               OLD.lifecycle='pending_upload' AND NEW.lifecycle='pending_scan'
               AND OLD.thread_id IS NULL AND NEW.thread_id IS NOT NULL
           )
       )
       OR NEW.uploader_principal_id <> OLD.uploader_principal_id
       OR NEW.uploader_session_id <> OLD.uploader_session_id
       OR NEW.classification <> OLD.classification
       OR NEW.file_name <> OLD.file_name
       OR NEW.media_type <> OLD.media_type
       OR NEW.expected_size_bytes <> OLD.expected_size_bytes
       OR NEW.expected_digest <> OLD.expected_digest
       OR NEW.created_at <> OLD.created_at
       OR NEW.lifecycle_version <> OLD.lifecycle_version + 1
       OR NOT (
           (OLD.lifecycle='pending_upload' AND NEW.lifecycle IN ('pending_scan','expired'))
           OR (OLD.lifecycle='pending_scan' AND NEW.lifecycle IN ('ready','quarantined','expired'))
       )
    THEN
        RAISE EXCEPTION 'invalid collaboration attachment lifecycle transition'
            USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER collaboration_attachment_transition
    BEFORE UPDATE ON erpx_collaboration.attachments
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.enforce_attachment_transition();

CREATE TRIGGER collaboration_relation_event_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.record_relation_events
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();
CREATE TRIGGER collaboration_attachment_event_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.attachment_events
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'record_relation_graphs','record_relations','record_relation_events',
        'record_relation_publication_drafts','record_relation_command_receipts',
        'attachments','attachment_events','attachment_command_receipts'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_collaboration.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_collaboration.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY collaboration_holding_isolation ON erpx_collaboration.%I '
            'USING (holding_id::text = current_setting(''app.holding_id'', true)) '
            'WITH CHECK (holding_id::text = current_setting(''app.holding_id'', true))',
            table_name
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
