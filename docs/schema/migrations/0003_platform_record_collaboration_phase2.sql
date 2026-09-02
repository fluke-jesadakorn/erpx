-- Platform-scoped records never enter a holding data plane. This control-side
-- store is reachable only through the audited global Collaboration gateway;
-- its caller must carry authenticated Platform-workforce authority.

CREATE SCHEMA erpx_global_collaboration;

CREATE TABLE erpx_global_collaboration.relation_graphs (
    source_record_id UUID PRIMARY KEY,
    source_record_ref JSONB NOT NULL,
    graph_thread_id UUID NOT NULL UNIQUE,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (source_record_ref ->> 'id' = source_record_id::TEXT),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_global_collaboration.relations (
    relation_id UUID PRIMARY KEY,
    source_record_id UUID NOT NULL REFERENCES
        erpx_global_collaboration.relation_graphs(source_record_id) ON DELETE RESTRICT,
    target_record_id UUID NOT NULL,
    source_record_ref JSONB NOT NULL,
    target_record_ref JSONB NOT NULL,
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
    CHECK (source_record_id <> target_record_id),
    CHECK (source_record_ref ->> 'id' = source_record_id::TEXT),
    CHECK (target_record_ref ->> 'id' = target_record_id::TEXT),
    CHECK (
        (relation_state='active' AND revoked_by_principal_id IS NULL
            AND revoked_by_session_id IS NULL AND revoked_at IS NULL)
        OR
        (relation_state='revoked' AND revoked_by_principal_id IS NOT NULL
            AND revoked_by_session_id IS NOT NULL AND revoked_at IS NOT NULL)
    )
);

CREATE UNIQUE INDEX global_collaboration_active_relation_identity
    ON erpx_global_collaboration.relations
       (source_record_id,target_record_id,relation_kind)
    WHERE relation_state='active';

CREATE TABLE erpx_global_collaboration.relation_events (
    event_id UUID PRIMARY KEY,
    relation_id UUID NOT NULL REFERENCES erpx_global_collaboration.relations(relation_id),
    source_record_id UUID NOT NULL,
    graph_version BIGINT NOT NULL CHECK (graph_version > 0),
    event_kind TEXT NOT NULL CHECK (event_kind IN ('added','revoked')),
    event_document JSONB NOT NULL,
    actor_principal_id UUID NOT NULL,
    actor_session_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    UNIQUE (source_record_id,graph_version)
);

CREATE TABLE erpx_global_collaboration.relation_publication_drafts (
    event_id UUID PRIMARY KEY REFERENCES erpx_global_collaboration.relation_events(event_id),
    relation_id UUID NOT NULL REFERENCES erpx_global_collaboration.relations(relation_id),
    fact_document JSONB NOT NULL,
    committed_sequence BIGINT,
    projected_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (event_id) REFERENCES erpx_cqrs_write.outbox_events(event_id),
    CHECK ((committed_sequence IS NULL) = (projected_at IS NULL)),
    CHECK (committed_sequence IS NULL OR committed_sequence > 0)
);

CREATE TABLE erpx_global_collaboration.command_receipts (
    idempotency_key TEXT PRIMARY KEY CHECK (octet_length(idempotency_key) BETWEEN 1 AND 256),
    command_id UUID NOT NULL UNIQUE,
    request_digest BYTEA NOT NULL CHECK (octet_length(request_digest)=32),
    relation_id UUID NOT NULL REFERENCES erpx_global_collaboration.relations(relation_id),
    graph_version BIGINT NOT NULL CHECK (graph_version > 0),
    event_id UUID NOT NULL REFERENCES erpx_global_collaboration.relation_events(event_id),
    result_relation_document JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

-- Platform Activity Rail authority mirrors the holding collaboration model,
-- but deliberately has no holding discriminator. It is reachable only through
-- the audited global gateway below.
CREATE TABLE erpx_global_collaboration.threads (
    thread_id UUID PRIMARY KEY,
    anchor_record_id UUID NOT NULL,
    anchor_record_ref JSONB NOT NULL,
    owner_context TEXT NOT NULL CHECK (octet_length(owner_context) BETWEEN 1 AND 64),
    record_type TEXT NOT NULL CHECK (octet_length(record_type) BETWEEN 1 AND 128),
    authorization_policy_id UUID NOT NULL,
    policy_document JSONB NOT NULL,
    anchor_document JSONB NOT NULL,
    audience_document JSONB NOT NULL,
    retention_document JSONB NOT NULL,
    classification TEXT NOT NULL CHECK (
        classification IN ('public','internal','confidential','restricted')
    ),
    thread_state TEXT NOT NULL CHECK (thread_state IN ('open','resolved')),
    pinned BOOLEAN NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_by_principal_id UUID NOT NULL,
    created_by_session_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (anchor_record_ref ->> 'id'=anchor_record_id::TEXT),
    CHECK (updated_at >= created_at)
);

CREATE INDEX global_collaboration_thread_anchor
    ON erpx_global_collaboration.threads(anchor_record_id,updated_at DESC,thread_id);

CREATE TABLE erpx_global_collaboration.comments (
    thread_id UUID NOT NULL REFERENCES erpx_global_collaboration.threads(thread_id) ON DELETE RESTRICT,
    comment_id UUID NOT NULL,
    author_principal_id UUID NOT NULL,
    author_session_id UUID NOT NULL,
    visibility TEXT NOT NULL CHECK (visibility IN ('visible','quarantined','hidden','redacted')),
    tombstoned_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (thread_id,comment_id),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_global_collaboration.comment_revisions (
    thread_id UUID NOT NULL,
    comment_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    author_principal_id UUID NOT NULL,
    author_session_id UUID NOT NULL,
    document JSONB NOT NULL,
    plain_text TEXT NOT NULL CHECK (octet_length(plain_text) BETWEEN 1 AND 65536),
    content_digest BYTEA NOT NULL CHECK (octet_length(content_digest)=32),
    attachments JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (thread_id,comment_id,revision),
    FOREIGN KEY (thread_id,comment_id)
        REFERENCES erpx_global_collaboration.comments(thread_id,comment_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_global_collaboration.reactions (
    thread_id UUID NOT NULL,
    comment_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    principal_session_id UUID NOT NULL,
    reaction TEXT NOT NULL CHECK (octet_length(reaction) BETWEEN 1 AND 64),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (thread_id,comment_id,principal_id,reaction),
    FOREIGN KEY (thread_id,comment_id)
        REFERENCES erpx_global_collaboration.comments(thread_id,comment_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_global_collaboration.followers (
    thread_id UUID NOT NULL REFERENCES erpx_global_collaboration.threads(thread_id) ON DELETE RESTRICT,
    principal_id UUID NOT NULL,
    principal_session_id UUID NOT NULL,
    muted_until TIMESTAMPTZ,
    snoozed_until TIMESTAMPTZ,
    followed_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (thread_id,principal_id),
    CHECK (updated_at >= followed_at)
);

CREATE TABLE erpx_global_collaboration.thread_events (
    event_cursor BIGSERIAL PRIMARY KEY,
    event_id UUID NOT NULL UNIQUE,
    thread_id UUID NOT NULL REFERENCES erpx_global_collaboration.threads(thread_id) ON DELETE RESTRICT,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_type TEXT NOT NULL CHECK (event_type ~ '^collaboration[.][a-z0-9_]+$'),
    event_version INTEGER NOT NULL CHECK (event_version IN (1,2)),
    payload JSONB NOT NULL,
    actor_principal_id UUID NOT NULL,
    actor_session_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    UNIQUE (thread_id,aggregate_version)
);

CREATE TABLE erpx_global_collaboration.thread_command_receipts (
    idempotency_key TEXT PRIMARY KEY CHECK (octet_length(idempotency_key) BETWEEN 1 AND 256),
    command_id UUID NOT NULL UNIQUE,
    request_digest BYTEA NOT NULL CHECK (octet_length(request_digest)=32),
    thread_id UUID NOT NULL REFERENCES erpx_global_collaboration.threads(thread_id) ON DELETE RESTRICT,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    last_event_cursor BIGINT NOT NULL REFERENCES erpx_global_collaboration.thread_events(event_cursor),
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE erpx_global_collaboration.attachments (
    attachment_id UUID PRIMARY KEY,
    object_id UUID NOT NULL UNIQUE,
    object_key TEXT NOT NULL UNIQUE CHECK (octet_length(object_key) BETWEEN 1 AND 512),
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
    quarantine_reason TEXT,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (anchor_record_ref ->> 'id' = anchor_record_id::TEXT),
    CHECK (upload_expires_at > created_at),
    CHECK (updated_at >= created_at),
    CHECK (
        (lifecycle='pending_upload' AND finalized_at IS NULL AND scanned_at IS NULL)
        OR (lifecycle='pending_scan' AND finalized_at IS NOT NULL AND scanned_at IS NULL
            AND verified_size_bytes=expected_size_bytes AND verified_digest=expected_digest)
        OR (lifecycle='ready' AND thread_id IS NOT NULL AND finalized_at IS NOT NULL
            AND scanned_at IS NOT NULL AND scan_engine IS NOT NULL
            AND octet_length(scan_engine) BETWEEN 1 AND 128
            AND scan_evidence_digest IS NOT NULL AND quarantine_reason IS NULL)
        OR (lifecycle='quarantined' AND thread_id IS NOT NULL AND finalized_at IS NOT NULL
            AND scanned_at IS NOT NULL AND scan_engine IS NOT NULL
            AND octet_length(scan_engine) BETWEEN 1 AND 128
            AND scan_evidence_digest IS NOT NULL AND quarantine_reason IS NOT NULL)
        OR lifecycle='expired'
    )
);

CREATE INDEX global_collaboration_attachment_scan_queue
    ON erpx_global_collaboration.attachments(updated_at,attachment_id)
    WHERE lifecycle='pending_scan';

CREATE FUNCTION erpx_global_collaboration.enforce_attachment_transition()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.object_id <> OLD.object_id
       OR NEW.object_key <> OLD.object_key
       OR NEW.anchor_record_id <> OLD.anchor_record_id
       OR NEW.anchor_record_ref <> OLD.anchor_record_ref
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
       OR NEW.lifecycle_version <> OLD.lifecycle_version + 1
       OR NOT (
           (OLD.lifecycle='pending_upload' AND NEW.lifecycle IN ('pending_scan','expired'))
           OR (OLD.lifecycle='pending_scan' AND NEW.lifecycle IN ('ready','quarantined','expired'))
       )
    THEN
        RAISE EXCEPTION 'invalid global collaboration attachment lifecycle transition'
            USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER global_collaboration_attachment_transition
    BEFORE UPDATE ON erpx_global_collaboration.attachments
    FOR EACH ROW EXECUTE FUNCTION erpx_global_collaboration.enforce_attachment_transition();

-- Every global gateway operation records exact actor/session/scope, endpoint
-- records, result category, and correlation before commit. Protected record
-- payload and raw search text are deliberately absent.
CREATE TABLE erpx_global_collaboration.gateway_audit (
    audit_id UUID PRIMARY KEY,
    operation TEXT NOT NULL CHECK (operation IN (
        'reference.resolve','reference.search',
        'relation.list','relation.add','relation.revoke',
        'thread.get','thread.list','thread.start',
        'attachment.prepare','attachment.finalize','attachment.download'
    )),
    actor_principal_id UUID NOT NULL,
    actor_session_id UUID NOT NULL,
    authority_scope JSONB NOT NULL CHECK (authority_scope = to_jsonb('Platform'::TEXT)),
    source_record_id UUID,
    target_record_id UUID,
    attachment_id UUID,
    command_id UUID,
    correlation_id UUID NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('allowed','denied','conflict','unavailable')),
    occurred_at TIMESTAMPTZ NOT NULL
);

CREATE FUNCTION erpx_global_collaboration.reject_append_only_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'global collaboration evidence is append-only';
END;
$$;

CREATE TRIGGER global_collaboration_relation_event_append_only
    BEFORE UPDATE OR DELETE ON erpx_global_collaboration.relation_events
    FOR EACH ROW EXECUTE FUNCTION erpx_global_collaboration.reject_append_only_change();
CREATE TRIGGER global_collaboration_thread_event_append_only
    BEFORE UPDATE OR DELETE ON erpx_global_collaboration.thread_events
    FOR EACH ROW EXECUTE FUNCTION erpx_global_collaboration.reject_append_only_change();
CREATE TRIGGER global_collaboration_gateway_audit_append_only
    BEFORE UPDATE OR DELETE ON erpx_global_collaboration.gateway_audit
    FOR EACH ROW EXECUTE FUNCTION erpx_global_collaboration.reject_append_only_change();

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
