-- Platform typed mention authority. Mention identity comes only from typed
-- comment tokens; delivery evidence and withdrawals are append-only.

ALTER TABLE erpx_global_collaboration.followers
    ALTER COLUMN principal_session_id DROP NOT NULL;

CREATE TABLE erpx_global_collaboration.mention_events (
    mention_event_id UUID PRIMARY KEY,
    thread_id UUID NOT NULL REFERENCES erpx_global_collaboration.threads(thread_id)
        ON DELETE RESTRICT,
    comment_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    target JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (thread_id,comment_id)
        REFERENCES erpx_global_collaboration.comments(thread_id,comment_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_global_collaboration.mention_withdrawals (
    mention_event_id UUID PRIMARY KEY
        REFERENCES erpx_global_collaboration.mention_events(mention_event_id) ON DELETE RESTRICT,
    thread_id UUID NOT NULL REFERENCES erpx_global_collaboration.threads(thread_id)
        ON DELETE RESTRICT,
    withdrawn_by_principal_id UUID NOT NULL,
    withdrawn_by_session_id UUID NOT NULL,
    withdrawn_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE erpx_global_collaboration.opaque_mention_inbox (
    inbox_cursor BIGSERIAL PRIMARY KEY,
    event_id UUID NOT NULL UNIQUE,
    recipient_principal_id UUID NOT NULL,
    thread_id UUID NOT NULL REFERENCES erpx_global_collaboration.threads(thread_id)
        ON DELETE RESTRICT,
    mention_event_id UUID NOT NULL
        REFERENCES erpx_global_collaboration.mention_events(mention_event_id) ON DELETE RESTRICT,
    anchor_record_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE (recipient_principal_id,mention_event_id)
);

CREATE INDEX global_collaboration_mention_inbox_recipient
    ON erpx_global_collaboration.opaque_mention_inbox
       (recipient_principal_id,inbox_cursor);

CREATE TRIGGER global_collaboration_mention_event_append_only
    BEFORE UPDATE OR DELETE ON erpx_global_collaboration.mention_events
    FOR EACH ROW EXECUTE FUNCTION erpx_global_collaboration.reject_append_only_change();
CREATE TRIGGER global_collaboration_mention_withdrawal_append_only
    BEFORE UPDATE OR DELETE ON erpx_global_collaboration.mention_withdrawals
    FOR EACH ROW EXECUTE FUNCTION erpx_global_collaboration.reject_append_only_change();
CREATE TRIGGER global_collaboration_mention_inbox_append_only
    BEFORE UPDATE OR DELETE ON erpx_global_collaboration.opaque_mention_inbox
    FOR EACH ROW EXECUTE FUNCTION erpx_global_collaboration.reject_append_only_change();

ALTER TABLE erpx_global_collaboration.gateway_audit
    DROP CONSTRAINT gateway_audit_operation_check;
ALTER TABLE erpx_global_collaboration.gateway_audit
    ADD CONSTRAINT gateway_audit_operation_check CHECK (operation IN (
        'reference.resolve','reference.search',
        'relation.list','relation.add','relation.revoke',
        'thread.get','thread.list','thread.start','thread.mutate',
        'thread.comment.add','thread.comment.edit','thread.comment.tombstone',
        'thread.mention.withdraw',
        'thread.resolve','thread.reopen','thread.pin','thread.unpin',
        'thread.reaction.add','thread.reaction.remove','thread.following.set',
        'mention.inbox',
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
