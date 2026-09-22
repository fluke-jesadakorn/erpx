CREATE TABLE organization_node_changes (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  target_node_id TEXT REFERENCES organization_nodes(id),
  action TEXT NOT NULL CHECK(action IN ('create','rename','suspend','reactivate')),
  node_kind TEXT NOT NULL CHECK(node_kind IN ('branch','department')),
  slug TEXT,
  display_name TEXT NOT NULL,
  branch_id TEXT REFERENCES organization_nodes(id),
  expected_node_version BIGINT,
  state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','submitted','applied','rejected','cancelled')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  requested_by TEXT NOT NULL,
  reviewed_by TEXT,
  reason TEXT NOT NULL,
  review_reason TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  reviewed_at BIGINT,
  applied_at BIGINT,
  CHECK((action='create' AND target_node_id IS NULL AND expected_node_version IS NULL AND slug IS NOT NULL) OR
        (action<>'create' AND target_node_id IS NOT NULL AND expected_node_version IS NOT NULL)),
  CHECK((node_kind='branch' AND branch_id IS NULL) OR (node_kind='department' AND branch_id IS NOT NULL))
);
CREATE UNIQUE INDEX organization_node_change_pending_target
  ON organization_node_changes(company_id,target_node_id)
  WHERE target_node_id IS NOT NULL AND state IN ('draft','submitted');
CREATE INDEX organization_node_change_queue
  ON organization_node_changes(company_id,state,updated_at DESC);

CREATE TABLE organization_node_change_events (
  id TEXT PRIMARY KEY,
  change_id TEXT NOT NULL REFERENCES organization_node_changes(id),
  company_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('drafted','submitted','applied','rejected','cancelled')),
  actor_id TEXT NOT NULL,
  version BIGINT NOT NULL,
  snapshot_json TEXT NOT NULL CHECK(erpx_json_valid(snapshot_json)),
  created_at BIGINT NOT NULL
);
CREATE INDEX organization_node_change_history
  ON organization_node_change_events(change_id,created_at,id);

CREATE FUNCTION organization_node_change_event_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'IMMUTABLE_ORGANIZATION_NODE_CHANGE_EVENT' USING ERRCODE='23514';
END $$;
CREATE TRIGGER organization_node_change_event_immutable
  BEFORE UPDATE OR DELETE ON organization_node_change_events
  FOR EACH ROW EXECUTE FUNCTION organization_node_change_event_immutable_fn();
