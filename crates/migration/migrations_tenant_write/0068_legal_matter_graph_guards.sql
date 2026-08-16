-- Legal owns the typed, versioned relationship graph. Migration 0081 promotes
-- exact scope and legal-hold columns to relational command authority; the JSON
-- columns below remain compatibility evidence only.

CREATE TABLE erpx_legal_write.matter_guards (
    matter_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    reference TEXT NOT NULL CHECK (octet_length(reference) BETWEEN 1 AND 255),
    title TEXT NOT NULL CHECK (octet_length(title) BETWEEN 1 AND 255),
    matter_status TEXT NOT NULL CHECK (matter_status IN ('open', 'closed')),
    legal_hold JSONB NOT NULL,
    created_by TEXT NOT NULL CHECK (octet_length(created_by) BETWEEN 1 AND 255),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, reference)
);

CREATE TABLE erpx_legal_write.matter_link_guards (
    matter_id UUID NOT NULL REFERENCES erpx_legal_write.matter_guards (matter_id),
    link_ordinal BIGINT NOT NULL CHECK (link_ordinal BETWEEN 1 AND 1024),
    relation TEXT NOT NULL CHECK (relation IN (
        'governed_by', 'supports', 'billed_under', 'attributed_to',
        'disputes', 'settlement_for', 'supersedes', 'accounting_for'
    )),
    target_context TEXT NOT NULL CHECK (octet_length(target_context) BETWEEN 1 AND 64),
    target_aggregate_kind TEXT NOT NULL CHECK (
        octet_length(target_aggregate_kind) BETWEEN 1 AND 128
    ),
    target_aggregate_id UUID NOT NULL,
    target_aggregate_version BIGINT NOT NULL CHECK (target_aggregate_version > 0),
    target_scope JSONB NOT NULL,
    target_scope_key TEXT NOT NULL,
    target_digest TEXT NOT NULL CHECK (
        target_digest ~ '^(sha256:)?[0-9A-Fa-f]{64}$'
    ),
    linked_by TEXT NOT NULL CHECK (octet_length(linked_by) BETWEEN 1 AND 255),
    linked_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (matter_id, link_ordinal),
    UNIQUE (
        matter_id, relation, target_context, target_aggregate_kind,
        target_aggregate_id
    )
);

CREATE TRIGGER legal_matter_link_immutable
    BEFORE UPDATE OR DELETE ON erpx_legal_write.matter_link_guards
    FOR EACH ROW EXECUTE FUNCTION erpx_legal_write.reject_append_only_mutation();

CREATE INDEX legal_matter_scope_status
    ON erpx_legal_write.matter_guards
        (scope_key, matter_status, updated_at DESC, matter_id DESC);
CREATE INDEX legal_matter_link_target
    ON erpx_legal_write.matter_link_guards
        (target_context, target_aggregate_kind, target_aggregate_id, matter_id);
