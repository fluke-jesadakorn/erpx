-- Collaboration authoritative records live in the same tenant data plane as
-- their first-class anchor. Cross-scope inbox rows intentionally contain only
-- opaque identifiers; no comment body or business descriptor is replicated.

CREATE SCHEMA erpx_collaboration;

CREATE TABLE erpx_collaboration.threads (
    tenant_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    anchor_record_id UUID NOT NULL,
    owner_context TEXT NOT NULL CHECK (octet_length(owner_context) BETWEEN 1 AND 64),
    record_type TEXT NOT NULL CHECK (octet_length(record_type) BETWEEN 1 AND 128),
    classification TEXT NOT NULL CHECK (
        classification IN ('public', 'internal', 'confidential', 'restricted')
    ),
    thread_state TEXT NOT NULL CHECK (thread_state IN ('open', 'resolved')),
    pinned BOOLEAN NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, thread_id),
    CHECK (updated_at >= created_at)
);

CREATE INDEX collaboration_thread_anchor
    ON erpx_collaboration.threads
       (tenant_id, anchor_record_id, updated_at DESC, thread_id);

-- Concern-specific bounded documents are not serialized Thread aggregates.
CREATE TABLE erpx_collaboration.thread_policies (
    tenant_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    policy_document JSONB NOT NULL,
    PRIMARY KEY (tenant_id, thread_id),
    FOREIGN KEY (tenant_id, thread_id)
        REFERENCES erpx_collaboration.threads (tenant_id, thread_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.thread_anchors (
    tenant_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    anchor_document JSONB NOT NULL,
    source_revision BIGINT,
    source_digest BYTEA CHECK (source_digest IS NULL OR octet_length(source_digest) = 32),
    stale BOOLEAN NOT NULL,
    PRIMARY KEY (tenant_id, thread_id),
    FOREIGN KEY (tenant_id, thread_id)
        REFERENCES erpx_collaboration.threads (tenant_id, thread_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.thread_audiences (
    tenant_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    audience_document JSONB NOT NULL,
    audience_realm TEXT NOT NULL CHECK (audience_realm IN ('internal','external_portal')),
    external_portal TEXT CHECK (
        external_portal IN ('customer','supplier','candidate','commerce','signatory')
    ),
    PRIMARY KEY (tenant_id, thread_id),
    FOREIGN KEY (tenant_id, thread_id)
        REFERENCES erpx_collaboration.threads (tenant_id, thread_id) ON DELETE RESTRICT,
    CHECK ((audience_realm = 'internal') = (external_portal IS NULL))
);

CREATE TABLE erpx_collaboration.thread_retention (
    tenant_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    retention_document JSONB NOT NULL,
    parent_retain_until TIMESTAMPTZ NOT NULL,
    comment_retain_until TIMESTAMPTZ NOT NULL,
    jurisdiction_retain_until TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, thread_id),
    FOREIGN KEY (tenant_id, thread_id)
        REFERENCES erpx_collaboration.threads (tenant_id, thread_id) ON DELETE RESTRICT
);

-- Opaque locator used only to discover an anchor identity before invoking the
-- record-policy projection. It carries no title, record type, comment body,
-- audience, count, or other protected business metadata.
CREATE TABLE erpx_collaboration.thread_anchor_authority_index (
    tenant_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    anchor_record_id UUID NOT NULL,
    authorization_policy_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    PRIMARY KEY (tenant_id, thread_id),
    FOREIGN KEY (tenant_id, thread_id)
        REFERENCES erpx_collaboration.threads (tenant_id, thread_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.comments (
    tenant_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    comment_id UUID NOT NULL,
    author_principal_id UUID NOT NULL,
    author_kind TEXT NOT NULL CHECK (author_kind IN ('human','agent')),
    visibility TEXT NOT NULL CHECK (
        visibility IN ('visible','quarantined','hidden','redacted')
    ),
    tombstoned_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, thread_id, comment_id),
    FOREIGN KEY (tenant_id, thread_id)
        REFERENCES erpx_collaboration.threads (tenant_id, thread_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.comment_revisions (
    tenant_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    comment_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    author_principal_id UUID NOT NULL,
    author_kind TEXT NOT NULL CHECK (author_kind IN ('human','agent')),
    document JSONB NOT NULL,
    plain_text TEXT NOT NULL CHECK (
        octet_length(plain_text) BETWEEN 1 AND 65536
    ),
    content_digest BYTEA NOT NULL CHECK (octet_length(content_digest) = 32),
    attachments JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, thread_id, comment_id, revision),
    FOREIGN KEY (tenant_id, thread_id, comment_id)
        REFERENCES erpx_collaboration.comments (tenant_id, thread_id, comment_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.mention_events (
    tenant_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    mention_event_id UUID NOT NULL,
    comment_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    target JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, mention_event_id),
    FOREIGN KEY (tenant_id, thread_id)
        REFERENCES erpx_collaboration.threads (tenant_id, thread_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, thread_id, comment_id, revision)
        REFERENCES erpx_collaboration.comment_revisions
            (tenant_id, thread_id, comment_id, revision)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.mention_withdrawals (
    tenant_id UUID NOT NULL,
    mention_event_id UUID NOT NULL,
    withdrawn_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, mention_event_id),
    FOREIGN KEY (tenant_id, mention_event_id)
        REFERENCES erpx_collaboration.mention_events (tenant_id, mention_event_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.events (
    event_cursor BIGSERIAL NOT NULL,
    tenant_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_type TEXT NOT NULL CHECK (
        event_type ~ '^collaboration[.][a-z0-9_]+$'
    ),
    event_version INTEGER NOT NULL CHECK (event_version = 1),
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    correlation_id UUID NOT NULL,
    PRIMARY KEY (tenant_id, event_cursor),
    FOREIGN KEY (tenant_id, thread_id)
        REFERENCES erpx_collaboration.threads (tenant_id, thread_id)
        ON DELETE RESTRICT
);

CREATE INDEX collaboration_event_stream
    ON erpx_collaboration.events (tenant_id, thread_id, event_cursor);

CREATE TABLE erpx_collaboration.command_receipts (
    tenant_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (
        octet_length(idempotency_key) BETWEEN 1 AND 256
    ),
    command_id UUID NOT NULL,
    request_digest BYTEA NOT NULL CHECK (octet_length(request_digest) = 32),
    thread_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    last_event_cursor BIGINT NOT NULL CHECK (last_event_cursor > 0),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, idempotency_key),
    UNIQUE (tenant_id, command_id),
    FOREIGN KEY (tenant_id, thread_id)
        REFERENCES erpx_collaboration.threads (tenant_id, thread_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, last_event_cursor)
        REFERENCES erpx_collaboration.events (tenant_id, event_cursor)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.opaque_mention_inbox (
    inbox_cursor BIGSERIAL NOT NULL,
    event_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    recipient_principal_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    mention_event_id UUID NOT NULL,
    anchor_record_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, inbox_cursor),
    UNIQUE (tenant_id, event_id),
    UNIQUE (tenant_id, recipient_principal_id, mention_event_id),
    FOREIGN KEY (tenant_id, thread_id)
        REFERENCES erpx_collaboration.threads (tenant_id, thread_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, mention_event_id)
        REFERENCES erpx_collaboration.mention_events (tenant_id, mention_event_id)
        ON DELETE RESTRICT
);

CREATE INDEX collaboration_inbox_recipient
    ON erpx_collaboration.opaque_mention_inbox
       (tenant_id, recipient_principal_id, inbox_cursor);

CREATE TABLE erpx_collaboration.encrypted_drafts (
    tenant_id UUID NOT NULL,
    draft_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    owner_principal_id UUID NOT NULL,
    ciphertext BYTEA NOT NULL CHECK (
        octet_length(ciphertext) BETWEEN 1 AND 262144
    ),
    key_version INTEGER NOT NULL CHECK (key_version > 0),
    expires_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, draft_id),
    UNIQUE (tenant_id, thread_id, owner_principal_id),
    FOREIGN KEY (tenant_id, thread_id)
        REFERENCES erpx_collaboration.threads (tenant_id, thread_id)
        ON DELETE CASCADE
);

CREATE INDEX collaboration_expired_drafts
    ON erpx_collaboration.encrypted_drafts (tenant_id, expires_at);

-- Only sealed object identity and digest are stored here. The encrypted
-- original resides in governed object storage and is never projected.
CREATE TABLE erpx_collaboration.moderation_evidence (
    tenant_id UUID NOT NULL,
    case_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    encrypted_object_id UUID NOT NULL,
    evidence_digest BYTEA NOT NULL CHECK (octet_length(evidence_digest) = 32),
    key_version INTEGER NOT NULL CHECK (key_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, case_id),
    FOREIGN KEY (tenant_id, thread_id)
        REFERENCES erpx_collaboration.threads (tenant_id, thread_id)
        ON DELETE RESTRICT
);

-- Mutable lifecycle entities are normalized by identity. Their bounded
-- documents contain one entity only and never contain comments or a Thread.
CREATE TABLE erpx_collaboration.thread_entities (
    tenant_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    entity_kind TEXT NOT NULL CHECK (entity_kind IN (
        'reaction','follower','relation','moderation_case','promotion','action_card'
    )),
    entity_id TEXT NOT NULL CHECK (octet_length(entity_id) BETWEEN 1 AND 255),
    entity_document JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, thread_id, entity_kind, entity_id),
    FOREIGN KEY (tenant_id, thread_id)
        REFERENCES erpx_collaboration.threads (tenant_id, thread_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.audit_log (
    tenant_id UUID NOT NULL,
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    actor_principal_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    action TEXT NOT NULL CHECK (octet_length(action) BETWEEN 1 AND 128),
    evidence_digest BYTEA NOT NULL CHECK (octet_length(evidence_digest) = 32),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, audit_id),
    UNIQUE (tenant_id, command_id),
    FOREIGN KEY (tenant_id, thread_id)
        REFERENCES erpx_collaboration.threads (tenant_id, thread_id) ON DELETE RESTRICT
);

CREATE FUNCTION erpx_collaboration.reject_append_only_change()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'collaboration evidence is append-only';
END;
$$;

CREATE TRIGGER collaboration_revision_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.comment_revisions
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();
CREATE TRIGGER collaboration_mention_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.mention_events
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();
CREATE TRIGGER collaboration_withdrawal_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.mention_withdrawals
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();
CREATE TRIGGER collaboration_event_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.events
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();
CREATE TRIGGER collaboration_inbox_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.opaque_mention_inbox
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();
CREATE TRIGGER collaboration_moderation_evidence_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.moderation_evidence
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();
CREATE TRIGGER collaboration_audit_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.audit_log
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();

DO $$
DECLARE
    table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'threads', 'thread_policies', 'thread_anchors', 'thread_audiences',
        'thread_retention', 'thread_anchor_authority_index', 'comments',
        'comment_revisions', 'mention_events',
        'mention_withdrawals', 'events', 'command_receipts',
        'opaque_mention_inbox', 'encrypted_drafts', 'moderation_evidence',
        'thread_entities', 'audit_log'
    ]
    LOOP
        EXECUTE format(
            'ALTER TABLE erpx_collaboration.%I ENABLE ROW LEVEL SECURITY',
            table_name
        );
        EXECUTE format(
            'ALTER TABLE erpx_collaboration.%I FORCE ROW LEVEL SECURITY',
            table_name
        );
        EXECUTE format(
            'CREATE POLICY collaboration_tenant_isolation ON erpx_collaboration.%I '
            'USING (tenant_id::text = current_setting(''app.tenant_id'', true)) '
            'WITH CHECK (tenant_id::text = current_setting(''app.tenant_id'', true))',
            table_name
        );
    END LOOP;
END;
$$;
