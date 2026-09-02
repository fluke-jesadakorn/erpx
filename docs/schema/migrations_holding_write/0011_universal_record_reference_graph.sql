-- 0011 universal `@` record reference graph — broadens Phase 2 graph to all 31 contexts
-- with ordered deduped soft cap 50, role-labeled kinds, and reference routing coverage.
-- Additive to 0003; does not recreate tables.

-- Broaden suggestion envelope owner check to all 31 contexts (covers per-module field AI)
ALTER TABLE erpx_agent_write.suggestion_envelopes
    DROP CONSTRAINT IF EXISTS suggestion_envelopes_owner_context_check;
ALTER TABLE erpx_agent_write.suggestion_envelopes
    ADD CONSTRAINT suggestion_envelopes_owner_context_check CHECK (
        owner_context IN (
            'identity-access','platform-administration','organization','commercial',
            'brand','customer-authority','customer-360','support','finance','sales',
            'point-of-sale','procurement','inventory','assets','maintenance','quality',
            'operations','people-payroll','legal','marketing','crm','manufacturing',
            'planning','projects','ecommerce','recruiting-talent','learning','benefits',
            'field-service','collaboration','agent-orchestration'
        )
    );

-- Broaden relation kinds to include plan role labels (billed_by, fulfills, authored_with)
ALTER TABLE erpx_collaboration.record_relations
    DROP CONSTRAINT IF EXISTS record_relations_relation_kind_check;
ALTER TABLE erpx_collaboration.record_relations
    ADD CONSTRAINT record_relations_relation_kind_check CHECK (relation_kind IN (
        'relates_to','blocks','depends_on','implements',
        'supersedes','derived_from','evidence_for',
        'billed_by','fulfills','authored_with'
    ));

-- Soft cap 50 helper: indexed ordinal ensures ListBacklinks pagination remains cheap
CREATE INDEX IF NOT EXISTS record_relations_host_ordinal
    ON erpx_collaboration.record_relations (holding_id, source_record_id, graph_version);

-- Backlinks index already on target; ensure holding+target path
CREATE INDEX IF NOT EXISTS record_relations_target_backlinks
    ON erpx_collaboration.record_relations (holding_id, target_record_id, relation_state);

-- Document shared edge view as canonical record_reference_edges alias for Plan/traceability
COMMENT ON TABLE erpx_collaboration.record_relations IS
  'Universal @ record reference edges (alias record_reference_edges): host→target ordered deduped append-only, soft cap 50, explicit reorder via graph_version, backlink-indexed for ListBacklinks. See Plan/01-vision-and-scope.md and adr/ADR-019.';
