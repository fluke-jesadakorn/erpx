-- Forward-only completion of the Platform collaboration attachment registry.
-- Public commands receive idempotent receipts; lifecycle evidence remains
-- append-only and Ready/Quarantined transitions are worker-only.

CREATE TABLE erpx_global_collaboration.attachment_events (
    event_cursor BIGSERIAL PRIMARY KEY,
    event_id UUID NOT NULL UNIQUE,
    attachment_id UUID NOT NULL REFERENCES erpx_global_collaboration.attachments(attachment_id)
        ON DELETE RESTRICT,
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
    UNIQUE (attachment_id,lifecycle_version),
    CHECK ((actor_principal_id IS NULL)=(actor_session_id IS NULL)),
    CHECK ((command_id IS NULL)=(correlation_id IS NULL))
);

CREATE TABLE erpx_global_collaboration.attachment_command_receipts (
    idempotency_key TEXT PRIMARY KEY CHECK (octet_length(idempotency_key) BETWEEN 1 AND 256),
    command_id UUID NOT NULL UNIQUE,
    request_digest BYTEA NOT NULL CHECK (octet_length(request_digest)=32),
    attachment_id UUID NOT NULL REFERENCES erpx_global_collaboration.attachments(attachment_id)
        ON DELETE RESTRICT,
    lifecycle_version BIGINT NOT NULL CHECK (lifecycle_version > 0),
    event_cursor BIGINT NOT NULL REFERENCES erpx_global_collaboration.attachment_events(event_cursor),
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TRIGGER global_collaboration_attachment_event_append_only
    BEFORE UPDATE OR DELETE ON erpx_global_collaboration.attachment_events
    FOR EACH ROW EXECUTE FUNCTION erpx_global_collaboration.reject_append_only_change();

ALTER TABLE erpx_global_collaboration.gateway_audit
    DROP CONSTRAINT gateway_audit_operation_check;
ALTER TABLE erpx_global_collaboration.gateway_audit
    ADD CONSTRAINT gateway_audit_operation_check CHECK (operation IN (
        'reference.resolve','reference.search',
        'relation.list','relation.add','relation.revoke',
        'thread.get','thread.list','thread.start','thread.mutate',
        'thread.comment.add','thread.comment.edit','thread.comment.tombstone',
        'thread.resolve','thread.reopen','thread.pin','thread.unpin',
        'thread.reaction.add','thread.reaction.remove','thread.following.set',
        'attachment.prepare','attachment.finalize','attachment.download'
    ));

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
